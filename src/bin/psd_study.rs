//! Layered PSD validation fixture: Unicode names, each blend mode, hidden/empty layers.
use graphite_studio::{
    core::{
        document::{BlendMode, CanvasSpec, Document},
        paper::PaperPreset,
    },
    export::{save_rendered_document, PsdBitDepth, SaveOptions},
    render::RasterRenderer,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or("psd-study".into()));
    std::fs::create_dir_all(&out)?;
    let spec = CanvasSpec::from_physical("PSD test", 16.0, 12.0, 120.0);
    let n = spec.pixel_count();
    let mut doc = Document::new(
        spec,
        PaperPreset::DrawingMedium,
        "White",
        vec![[0.97, 0.95, 0.92]; n],
    );
    doc.set_paper_color([230, 210, 180]);
    for (number, mode) in BlendMode::ALL.into_iter().enumerate() {
        if number > 0 {
            doc.add_layer();
        }
        doc.rename_layer(number, format!("{} – lápis ✎", mode.label()));
        doc.set_layer_blend_mode(doc.active_layer_id(), mode);
        for y in 4 + number * 3..28 + number * 3 {
            for x in 5 + number * 5..38 + number * 5 {
                let i = doc.index(x, y);
                let m = 0.12 + (x % 7) as f32 * 0.07;
                doc.surface.graphite_mass[i] = m;
                doc.surface.loose_mass[i] = m * 0.6;
                doc.surface.compacted_mass[i] = m * 0.4;
                doc.surface.color_r_mass[i] = m * (0.25 + 0.07 * number as f32);
                doc.surface.color_g_mass[i] = m * 0.3;
                doc.surface.color_b_mass[i] = m * 0.42;
            }
        }
    }
    doc.set_layer_visible(3, false);
    doc.add_layer();
    doc.rename_layer(5, "Empty layer".into());
    let mut renderer = RasterRenderer::default();
    for depth in [PsdBitDepth::Eight, PsdBitDepth::Sixteen] {
        save_rendered_document(
            &mut renderer,
            &doc,
            &out.join(format!("layers-{}.psd", depth.bits())),
            SaveOptions {
                psd_bit_depth: depth,
            },
        )?;
    }
    save_rendered_document(
        &mut renderer,
        &doc,
        &out.join("merged.png"),
        SaveOptions::default(),
    )?;
    println!("Wrote 8/16-bit PSDs with six drawing layers and separate paper");
    Ok(())
}
