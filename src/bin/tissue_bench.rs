//! Repeatable production-engine tissue workload; no window or input control.
use graphite_studio::{
    core::{
        document::{CanvasSpec, Document},
        history::EditTransaction,
        paper::PaperPreset,
        pencil::{PencilTipState, ToolKind, ToolSettings},
        stroke::{StrokeEngine, StrokePoint},
    },
    render::{DocumentRenderer, RasterRenderer},
};
fn main() {
    let mut report = String::new();
    for size in [200., 400., 800.] {
        let spec = CanvasSpec::a4(120.);
        let n = spec.pixel_count();
        let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; n]);
        let settings = ToolSettings {
            tool: ToolKind::Tissue,
            tissue_size_px: size,
            ..Default::default()
        };
        let mut engine = StrokeEngine::default();
        let mut tip = PencilTipState::default();
        let mut tx = EditTransaction::default();
        let point = |i: f32| StrokePoint {
            x: 250. + i * 6.,
            y: 600. + (i * 0.12).sin() * 30.,
            pressure: 0.65,
            tilt_deg: 8.,
            azimuth_deg: 20.,
            rotation_deg: None,
        };
        let start = std::time::Instant::now();
        for i in 1..=60 {
            engine.apply_segment(
                &mut doc,
                &settings,
                &mut tip,
                point((i - 1) as f32),
                point(i as f32),
                &mut tx,
            );
        }
        let ms = start.elapsed().as_secs_f64() * 1000.;
        report.push_str(&format!(
            "{size:.0} px: 60 segments in {ms:.2} ms ({:.2} ms/segment)\n",
            ms / 60.
        ));
        if let Some(prefix) = std::env::args().nth(1) {
            let pixels = RasterRenderer::default().rgba8(&doc);
            image::save_buffer(
                format!("{prefix}-{size:.0}.png"),
                &pixels,
                doc.spec.width_px as u32,
                doc.spec.height_px as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            if let Some(reference) = std::env::args().nth(2) {
                let before = image::open(format!("{reference}-{size:.0}.png"))
                    .unwrap()
                    .to_rgba8()
                    .into_raw();
                let max_error = before
                    .iter()
                    .zip(&pixels)
                    .map(|(&a, &b)| a.abs_diff(b))
                    .max()
                    .unwrap();
                assert!(max_error <= 1, "Tissue appearance changed: {max_error}");
                report.push_str(&format!(
                    "  Pixel comparison: max channel difference {max_error}/255\n"
                ));
            }
            let mut history = graphite_studio::core::history::History::default();
            history.push(tx, &doc);
            assert!(history.undo(&mut doc));
            assert!(doc.surface.graphite_mass.iter().all(|&m| m == 0.));
            assert!(history.redo(&mut doc));
            assert_eq!(RasterRenderer::default().rgba8(&doc), pixels);
            report.push_str("  Large-stroke undo/redo: exact\n");
        }
    }
    if let Some(prefix) = std::env::args().nth(1) {
        std::fs::write(format!("{prefix}.txt"), &report).unwrap();
    }
    print!("{report}");
}
