use graphite_studio::{
    export::PsdBitDepth,
    import,
    project::{self, Project, ProjectRef},
    render::{layer_pixel_rgba, DocumentRenderer, RasterRenderer},
};
use std::path::{Path, PathBuf};
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/open")
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
fn temp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("graphite-open-{}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p.join(name)
}

#[test]
fn independent_psds_preserve_layers_at_all_depths_and_compressions() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixtures().join("manifest.json")).unwrap()).unwrap();
    for fixture in manifest.as_array().unwrap() {
        let filename = fixture["file"].as_str().unwrap();
        let loaded =
            import::open(&fixtures().join(filename)).unwrap_or_else(|e| panic!("{filename}: {e}"));
        assert!(!loaded.editable_source);
        let doc = &loaded.project.document;
        assert_eq!((doc.spec.width_px, doc.spec.height_px), (7, 5));
        let layers = fixture["layers"].as_array().unwrap();
        assert_eq!(doc.layers.len(), layers.len(), "{filename}");
        for (l, expected) in layers.iter().enumerate() {
            assert_eq!(doc.layers[l].name, expected["name"].as_str().unwrap());
            assert_eq!(
                doc.layers[l].visible,
                expected["visible"].as_bool().unwrap()
            );
            assert_eq!(
                doc.layers[l].opacity,
                expected["opacity"].as_u64().unwrap() as u8
            );
            assert_eq!(
                doc.layers[l].blend_mode,
                if l == 1 {
                    graphite_studio::core::document::BlendMode::Multiply
                } else {
                    graphite_studio::core::document::BlendMode::Normal
                }
            );
            let left = expected["left"].as_i64().unwrap();
            let top = expected["top"].as_i64().unwrap();
            let width = expected["width"].as_u64().unwrap() as i64;
            let height = expected["height"].as_u64().unwrap() as i64;
            for y in 0..5 {
                for x in 0..7 {
                    let actual = layer_pixel_rgba(doc, Some(l), y * 7 + x);
                    let (sx, sy) = (x as i64 - left, y as i64 - top);
                    if sx < 0 || sy < 0 || sx >= width || sy >= height {
                        assert_eq!(actual[3], 0.);
                        continue;
                    }
                    let pixel = &expected["pixels"][(sy * width + sx) as usize];
                    for c in 0..4 {
                        assert!(
                            (actual[c] * 255. - pixel[c].as_f64().unwrap() as f32).abs() <= 1.1,
                            "{filename} layer {l} ({x},{y}) channel {c}: {} != {}",
                            actual[c] * 255.,
                            pixel[c]
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn pass_through_group_and_mask_keep_individual_layer_pixels() {
    let loaded = import::open(&fixtures().join("group-mask.psd")).unwrap();
    let doc = loaded.project.document;
    assert_eq!(doc.layers.len(), 1);
    assert_eq!(doc.layers[0].name, "Folder / Masked");
    for x in 0..4 {
        let p = layer_pixel_rgba(&doc, Some(0), 7 + 1 + x);
        assert!((p[3] - [0., 64. / 255., 128. / 255., 1.][x]).abs() < 0.0001);
    }
}

#[test]
fn clipping_keeps_layers_separate_and_rejects_unpreservable_translucency() {
    let p = import::open(&fixtures().join("clip-opaque.psd"))
        .unwrap()
        .project;
    assert_eq!(p.document.layers.len(), 2);
    for x in 0..7 {
        let pixel = layer_pixel_rgba(&p.document, Some(1), x);
        assert!((pixel[3] - if x < 4 { 1. } else { 0. }).abs() < 0.00001);
    }
    let error = import::open(&fixtures().join("clip-translucent.psd"))
        .err()
        .unwrap();
    assert!(error.contains("translucent base"), "{error}");
}

#[test]
fn solid_fill_layers_without_masks_preserve_color_and_fill_opacity() {
    let p = import::open(&fixtures().join("solid-fill.psd"))
        .unwrap()
        .project;
    assert_eq!(p.document.layers.len(), 1);
    assert!(p.document.layers[0].vectors.strokes[0].shape.is_some());
    let pixel = layer_pixel_rgba(&p.document, Some(0), 24 * 64 + 32);
    for (a, b) in pixel
        .into_iter()
        .zip([20. / 255., 100. / 255., 200. / 255., 128. / 255.])
    {
        assert!((a - b).abs() < 0.00001);
    }
}

#[test]
fn all_raster_extensions_open_with_color_alpha_and_orientation() {
    use image::ImageDecoder;
    for name in [
        "rgba.PNG",
        "palette.png",
        "gray16.png",
        "color.bmp",
        "color.jpeg",
        "color.jpg",
        "rotated.jpg",
    ] {
        let path = fixtures().join(name);
        let loaded = import::open(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut decoder = image::ImageReader::open(&path)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        let orientation = decoder.orientation().unwrap();
        let mut expected = image::DynamicImage::from_decoder(decoder).unwrap();
        expected.apply_orientation(orientation);
        let pixels = expected.to_rgba32f();
        let doc = &loaded.project.document;
        assert_eq!(
            (doc.spec.width_px, doc.spec.height_px),
            (pixels.width() as usize, pixels.height() as usize)
        );
        assert_eq!(doc.layers.len(), 1);
        for (i, p) in pixels.pixels().enumerate() {
            let got = layer_pixel_rgba(doc, Some(0), i);
            for c in 0..4 {
                assert!(
                    (got[c] - p[c]).abs() < 0.00001,
                    "{name}: pixel {i} channel {c}"
                );
            }
        }
        let rendered = RasterRenderer::default().rgba8(doc);
        for (i, p) in pixels.pixels().enumerate() {
            for c in 0..3 {
                let expected = (p[c] * p[3] + 1. - p[3]) * 255.;
                assert!((rendered[i * 4 + c] as f32 - expected).abs() <= 0.51);
            }
        }
    }
}

#[test]
fn imported_layers_survive_graphite_and_psd_save_reopen() {
    let imported = import::open(&fixtures().join("layers-16-3.psd"))
        .unwrap()
        .project;
    for name in [
        "layer-roundtrip.graphite",
        "layer-roundtrip-8.psd",
        "layer-roundtrip-16.psd",
    ] {
        let path = temp(name);
        if name.ends_with("graphite") {
            project::save(&path, &data(&imported)).unwrap();
        } else {
            project::save_psd(
                &path,
                &data(&imported),
                &mut RasterRenderer::default(),
                if name.ends_with("-8.psd") {
                    PsdBitDepth::Eight
                } else {
                    PsdBitDepth::Sixteen
                },
            )
            .unwrap();
        }
        let loaded = import::open(&path).unwrap();
        assert!(loaded.editable_source);
        assert_eq!(loaded.project.document.layers.len(), 3);
        for l in 0..3 {
            assert_eq!(
                loaded.project.document.layers[l].name,
                imported.document.layers[l].name
            );
            for i in 0..35 {
                assert_eq!(
                    layer_pixel_rgba(&loaded.project.document, Some(l), i),
                    layer_pixel_rgba(&imported.document, Some(l), i)
                );
            }
        }
        assert_eq!(
            RasterRenderer::default().rgba8(&loaded.project.document),
            RasterRenderer::default().rgba8(&imported.document)
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn invalid_and_unsupported_input_returns_errors_without_panics() {
    let original = std::fs::read(fixtures().join("layers-8-3.psd")).unwrap();
    let path = temp("invalid.psd");
    for length in [0, 1, 25, 34, 100, original.len() / 2] {
        std::fs::write(&path, &original[..length]).unwrap();
        assert!(import::open(&path).is_err());
    }
    let mut huge = original.clone();
    huge[14..22].fill(255);
    std::fs::write(&path, huge).unwrap();
    assert!(import::open(&path).is_err());
    let mut bad = original.clone();
    bad[22..24].copy_from_slice(&7u16.to_be_bytes());
    std::fs::write(&path, bad).unwrap();
    assert!(import::open(&path).is_err());
    assert!(import::open(&temp("unsupported.gif")).is_err());
    assert!(import::open(&temp("missing.png")).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn photoshop_shapes_retain_cubic_handles_holes_strokes_and_saved_paths() {
    use eframe::egui::{Pos2, Rect, Vec2};
    use graphite_studio::core::{history::EditTransaction, vector};
    let opened = import::open(&fixtures().join("shape-layers.psd")).unwrap();
    let mut p = opened.project;
    assert_eq!(p.document.layers.len(), 4);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixtures().join("shape-manifest.json")).unwrap())
            .unwrap();
    for (i, expected) in manifest.as_array().unwrap().iter().enumerate() {
        let layer = &p.document.layers[i + 1];
        assert_eq!(layer.name, expected["name"]);
        let stroke = &layer.vectors.strokes[0];
        let shape = stroke.shape.as_ref().expect("must remain vector geometry");
        let expected_paths: Vec<graphite_studio::core::vector::shape::Subpath> =
            serde_json::from_value(expected["paths"].clone()).unwrap();
        assert_eq!(shape.paths, expected_paths);
        let mut doc = p.document.clone();
        doc.activate_layer(i + 1);
        let before: Vec<_> = (0..64 * 48)
            .map(|n| layer_pixel_rgba(&doc, Some(i + 1), n))
            .collect();
        let layer = doc.layers[i + 1].vectors.clone();
        vector::replay(&mut doc, &layer, &mut EditTransaction::default());
        for (n, pixel) in before.iter().enumerate() {
            let got = layer_pixel_rgba(&doc, Some(i + 1), n);
            for c in 0..4 {
                assert!(
                    (pixel[c] - got[c]).abs() < 0.0001,
                    "replay layer {i} pixel {n} channel {c}"
                );
            }
        }
    }
    assert_eq!(p.document.layers[1].opacity, 180);
    assert_eq!(p.document.layers[3].name, "Path / Saved curve");
    assert!(!p.document.layers[3].visible);
    assert!(p.document.layers[3].vectors.strokes[0].shape.is_some());
    let original = p.document.layers[1].vectors.strokes[0].clone();
    assert!(original.hit_test(Vec2::new(12., 20.), 0.1, 300.));
    assert!(
        !original.hit_test(Vec2::new(28., 24.), 0.1, 300.),
        "hole must remain empty"
    );
    assert_eq!(layer_pixel_rgba(&p.document, Some(1), 24 * 64 + 28)[3], 0.);
    let a = Rect::from_min_size(Pos2::ZERO, Vec2::new(64., 48.));
    let b = Rect::from_min_size(Pos2::new(2., 3.), Vec2::new(32., 24.));
    let resized = original.transformed(a, b);
    let old = &original.shape.as_ref().unwrap().paths[0].knots[0];
    let new = &resized.shape.as_ref().unwrap().paths[0].knots[0];
    assert_eq!(new.after, Vec2::new(2., 3.) + old.after * 0.5);
    let restored = resized.transformed(b, a);
    for (p, q) in restored
        .shape
        .as_ref()
        .unwrap()
        .paths
        .iter()
        .zip(&original.shape.as_ref().unwrap().paths)
    {
        for (k, j) in p.knots.iter().zip(&q.knots) {
            for (x, y) in [
                (k.before, j.before),
                (k.anchor, j.anchor),
                (k.after, j.after),
            ] {
                assert!((x - y).length() < 0.00001);
            }
        }
    }
    assert_eq!(
        original
            .rotated(a.center(), std::f32::consts::PI)
            .shape
            .as_ref()
            .unwrap()
            .paths[0]
            .knots[0]
            .anchor
            .x,
        64. - old.anchor.x
    );
    p.document.activate_layer(1);
    let mut layer = (*p.document.layers[1].vectors).clone();
    layer.strokes[0] = std::sync::Arc::new(resized);
    vector::replay(&mut p.document, &layer, &mut EditTransaction::default());
    p.document.layers[1].vectors = std::sync::Arc::new(layer);
    for name in ["vector-roundtrip.graphite", "vector-roundtrip.psd"] {
        let path = temp(name);
        if name.ends_with("graphite") {
            project::save(&path, &data(&p)).unwrap();
        } else {
            project::save_psd(
                &path,
                &data(&p),
                &mut RasterRenderer::default(),
                PsdBitDepth::Sixteen,
            )
            .unwrap();
        }
        let reopened = import::open(&path).unwrap().project;
        for i in 1..4 {
            assert_eq!(
                reopened.document.layers[i].vectors.strokes[0].shape,
                p.document.layers[i].vectors.strokes[0].shape
            );
        }
        assert_eq!(
            RasterRenderer::default().rgba8(&reopened.document),
            RasterRenderer::default().rgba8(&p.document)
        );
        if name.ends_with("psd") {
            // Force standard PSD import, independent of Graphite's embedded state.
            let mut bytes=std::fs::read(&path).unwrap();
            let at=bytes.windows(8).position(|b|b==b"GSPROJ01").unwrap();bytes[at]=0;
            std::fs::write(&path,bytes).unwrap();
            let standard=import::open(&path).unwrap();assert!(!standard.editable_source);
            let shape_layer=standard.project.document.layers.iter().find(|l|l.name.starts_with("Vector - Bezier shape with hole")).unwrap();
            let shape=shape_layer.vectors.strokes[0].shape.as_ref().unwrap();
            assert_eq!(shape.paths.len(),2);assert_eq!(shape.paths[1].operation,2);
            let curve=standard.project.document.layers.iter().find(|l|l.name.starts_with("Vector - Blue curve")).unwrap();
            assert_eq!(curve.vectors.strokes[0].shape.as_ref().unwrap().stroke.as_ref().unwrap().width,3.);
        }
        std::fs::remove_file(path).unwrap();
    }
    let error = import::open(&fixtures().join("unsupported-vector-style.psd"))
        .err()
        .unwrap();
    assert!(error.contains("Inside/outside"), "{error}");
}

#[test]
fn photoshop_saturation_opens_and_survives_native_and_standard_psd_roundtrips() {
    use graphite_studio::{core::document::BlendMode, export::{save_rendered_document, SaveOptions}};
    for source_depth in [8,16] {
        let loaded = import::open(&fixtures().join(format!("saturation-{source_depth}.psd"))).unwrap();
        assert!(!loaded.editable_source);
        let p = loaded.project;
        assert_eq!(p.document.layers.len(),3);
        let layer = p.document.layers.iter().position(|l| l.blend_mode == BlendMode::Saturation).unwrap();
        let expected = RasterRenderer::default().rgba8(&p.document);
        let path = temp(&format!("sat-{source_depth}.graphite"));
        project::save(&path,&data(&p)).unwrap();
        let reopened = import::open(&path).unwrap().project;
        assert_eq!(reopened.document.layers[layer].blend_mode,BlendMode::Saturation);
        assert_eq!(RasterRenderer::default().rgba8(&reopened.document),expected);
        std::fs::remove_file(&path).unwrap();
        for depth in [PsdBitDepth::Eight,PsdBitDepth::Sixteen] {
            for native in [false,true] {
                let path = temp(&format!("sat-{source_depth}-{}-{native}.psd",depth.bits()));
                if native { project::save_psd(&path,&data(&p),&mut RasterRenderer::default(),depth).unwrap(); }
                else { save_rendered_document(&mut RasterRenderer::default(),&p.document,&path,
                    SaveOptions {psd_bit_depth:depth}).unwrap(); }
                let bytes = std::fs::read(&path).unwrap();
                assert!(bytes.windows(8).any(|b| b == b"8BIMsat "));
                let reopened = import::open(&path).unwrap();
                assert_eq!(reopened.editable_source,native);
                let doc = &reopened.project.document;
                let shift = usize::from(!native); // standard PSD contains explicit Paper
                assert_eq!(doc.layers.len(),p.document.layers.len()+shift);
                for (i,l) in p.document.layers.iter().enumerate() {
                    let got = &doc.layers[i+shift];
                    assert_eq!(got.name,l.name); assert_eq!(got.blend_mode,l.blend_mode);
                    assert_eq!(got.visible,l.visible); assert_eq!(got.opacity,l.opacity);
                }
                let rgba = RasterRenderer::default().rgba8(doc);
                assert!(rgba.iter().zip(&expected).all(|(a,b)| a.abs_diff(*b) <= 1));
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
}
