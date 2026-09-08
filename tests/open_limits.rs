use graphite_studio::{
    export::PsdBitDepth,
    import,
    project::{self, Project, ProjectRef},
    render::{layer_pixel_rgba, RasterRenderer},
};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

fn temp(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("graphite-no-quotas-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    root.join(name)
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/open/rgba.PNG")
}
fn data(p: &Project) -> ProjectRef<'_> {
    ProjectRef {
        document: &p.document,
        settings: &p.settings,
        tip: &p.tip,
        engine: &p.engine,
        brushes: &p.brushes,
        custom_paper: &p.custom_paper,
        zoom: p.zoom,
        pan: p.pan,
    }
}

#[test]
fn source_files_larger_than_512_mb_open_in_raster_and_native_paths() {
    let png = temp("large-source.png");
    fs::copy(fixture(), &png).unwrap();
    // Padding extends the real on-disk source beyond the old metadata gate.
    OpenOptions::new()
        .write(true)
        .open(&png)
        .unwrap()
        .set_len(513 * 1024 * 1024)
        .unwrap();
    let p = import::open(&png).unwrap().project;
    let native = temp("large-source.graphite");
    project::save(&native, &data(&p)).unwrap();
    OpenOptions::new()
        .write(true)
        .open(&native)
        .unwrap()
        .set_len(513 * 1024 * 1024)
        .unwrap();
    let reopened = import::open(&native).unwrap();
    assert!(reopened.editable_source);
    assert_eq!(
        p.document.surface.graphite_mass,
        reopened.project.document.surface.graphite_mass
    );
    fs::remove_file(png).unwrap();
    fs::remove_file(native).unwrap();
}

#[test]
fn eight_hundred_layers_save_and_reopen_natively_and_as_external_psd() {
    let mut p = import::open(&fixture()).unwrap().project;
    for i in 1..800 {
        p.document.add_layer();
        let layer = &mut p.document.layers[i];
        layer.name = format!("Layer {i}");
        layer.visible = i % 3 != 0;
        layer.opacity = (i % 256) as u8;
    }
    for ext in ["graphite", "psd"] {
        let path = temp(&format!("800-layers.{ext}"));
        if ext == "graphite" {
            project::save(&path, &data(&p)).unwrap();
        } else {
            project::save_psd(
                &path,
                &data(&p),
                &mut RasterRenderer::default(),
                PsdBitDepth::Eight,
            )
            .unwrap();
        }
        let opened = import::open(&path).unwrap();
        assert!(opened.editable_source);
        assert_eq!(opened.project.document.layers.len(), 800);
        for (a, b) in p
            .document
            .layers
            .iter()
            .zip(&opened.project.document.layers)
        {
            assert_eq!(a.name, b.name);
            assert_eq!(a.visible, b.visible);
            assert_eq!(a.opacity, b.opacity);
        }
        fs::remove_file(path).unwrap();
    }
    let path = temp("800-external.psd");
    graphite_studio::export::save_rendered_document(
        &mut RasterRenderer::default(),
        &p.document,
        &path,
        graphite_studio::export::SaveOptions {
            psd_bit_depth: PsdBitDepth::Eight,
        },
    )
    .unwrap();
    let external = import::open(&path).unwrap();
    assert!(!external.editable_source);
    assert_eq!(external.project.document.layers.len(), 801); // Export includes paper.
    for (a, b) in p
        .document
        .layers
        .iter()
        .zip(&external.project.document.layers[1..])
    {
        assert_eq!(a.name, b.name);
        assert_eq!(a.visible, b.visible);
        assert_eq!(a.opacity, b.opacity);
    }
    fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "Allocates a full-resolution canvas beyond the former 16-million-pixel cap"]
fn canvas_above_sixteen_million_pixels_opens_saves_and_reopens() {
    let png = temp("4001x4000.png");
    image::RgbImage::from_pixel(4001, 4000, image::Rgb([70, 110, 190]))
        .save(&png)
        .unwrap();
    let p = import::open(&png).unwrap().project;
    assert_eq!(
        (p.document.spec.width_px, p.document.spec.height_px),
        (4001, 4000)
    );
    let native = temp("4001x4000.graphite");
    project::save(&native, &data(&p)).unwrap();
    drop(p);
    let reopened = import::open(&native).unwrap().project;
    assert_eq!(
        (
            reopened.document.spec.width_px,
            reopened.document.spec.height_px
        ),
        (4001, 4000)
    );
    for i in [0, 4000, 8_000_000, 16_003_999] {
        let rgba =
            layer_pixel_rgba(&reopened.document, Some(0), i).map(|v| (v * 255.).round() as u8);
        assert_eq!(rgba, [70, 110, 190, 255]);
    }
    fs::remove_file(png).unwrap();
    fs::remove_file(native).unwrap();
}
