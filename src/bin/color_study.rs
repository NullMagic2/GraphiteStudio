//! Selected-color swatches beside marks made by the production material engine.
use graphite_studio::{
    core::{
        document::{CanvasSpec, Document},
        history::EditTransaction,
        paper::PaperPreset,
        pencil::{PencilTipState, ToolSettings},
        stroke::{StrokeEngine, StrokePoint},
    },
    render::{DocumentRenderer, RasterRenderer},
};
use image::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path =
        std::path::PathBuf::from(std::env::args().nth(1).unwrap_or("color-study.png".into()));
    let (w, h) = (780, 480);
    let spec = CanvasSpec {
        width_px: w,
        height_px: h,
        width_mm: 66.04,
        height_mm: 40.64,
        dpi: 300.,
        name: "Pigment study".into(),
    };
    let mut doc = Document::new(
        spec,
        PaperPreset::DrawingMedium,
        "White",
        vec![[1.; 3]; w * h],
    );
    let colors = [[235, 35, 55], [20, 160, 65], [35, 75, 240], [104, 104, 104]];
    for (row, color) in colors.into_iter().enumerate() {
        for (col, variation) in [0., 0.24].into_iter().enumerate() {
            let settings = ToolSettings {
                pencil_color_rgb: color,
                particle_variation: variation,
                auto_azimuth: false,
                ..Default::default()
            };
            let mut tip = PencilTipState::fresh_for_formulation(2., settings.grade.formulation());
            let mut engine = StrokeEngine::default();
            for (dy, tilt, passes) in [(0., 8., 1), (40., 72., 4)] {
                for pass in 0..passes {
                    let mut tx = EditTransaction::default();
                    let mut prev = StrokePoint {
                        x: 125. + col as f32 * 330.,
                        y: 32. + row as f32 * 120. + dy + pass as f32,
                        pressure: 0.,
                        tilt_deg: tilt,
                        azimuth_deg: 85.,
                        rotation_deg: None,
                    };
                    let start = prev;
                    engine.begin_pencil_stroke(prev);
                    for i in 1..=160 {
                        let t = i as f32 / 160.;
                        let point = StrokePoint {
                            x: start.x + 260. * t,
                            y: start.y + 7. * (t * 4.).sin(),
                            pressure: 0.8 * (t / 0.06).min(1.) * ((1. - t) / 0.08).min(1.),
                            ..start
                        };
                        engine.apply_segment(&mut doc, &settings, &mut tip, prev, point, &mut tx);
                        prev = point;
                    }
                    tip.commit_pending_wear();
                }
            }
        }
    }
    let mut renderer = RasterRenderer::default();
    let mut image = RgbaImage::from_raw(w as u32, h as u32, renderer.rgba8(&doc)).unwrap();
    for (row, color) in colors.into_iter().enumerate() {
        for y in 25 + row as u32 * 120..90 + row as u32 * 120 {
            for x in 25..90 {
                image.put_pixel(x, y, Rgba([color[0], color[1], color[2], 255]));
            }
        }
    }
    image.save(path)?;
    Ok(())
}
