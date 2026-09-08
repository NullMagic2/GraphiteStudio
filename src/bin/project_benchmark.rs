//! Save/open timings with exact material and retained-path validation; no windows.
use graphite_studio::{
    core::{document::{CanvasSpec, Document}, history::EditTransaction, paper::PaperPreset,
        pencil::{PencilTipState, ToolSettings}, stroke::{StrokeEngine, StrokePoint}, vector::VectorStroke},
    project::{self, ProjectRef}, render::{DocumentRenderer, RasterRenderer}, export::PsdBitDepth,
};
use std::{sync::Arc, time::Instant};
fn main() {
    let prefix = std::env::args().nth(1).expect("Output prefix");
    let spec = CanvasSpec::a4(120.);
    let n = spec.pixel_count();
    let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; n]);
    let settings = ToolSettings { pressure_width: 0.6, ..Default::default() };
    let tip = PencilTipState::default();
    let engine = StrokeEngine::default();
    for layer in 0..2 {
        if layer > 0 { doc.add_layer(); }
        for line in 0..10 {
            let stroke = Arc::new(VectorStroke {
                shape: None,
                label: "Pencil".into(), settings: settings.clone(), tip: tip.clone(), engine: engine.clone(),
                points: (0..50).map(|i| StrokePoint { x: 100. + i as f32 * 10.,
                    y: 180. + (layer * 350 + line * 20) as f32 + (i as f32 * 0.15).sin() * 30.,
                    pressure: 0.2 + i as f32 * 0.012, tilt_deg: 8., azimuth_deg: 20., rotation_deg: None }).collect(),
                polyline: false, selection: Default::default(),
            });
            stroke.apply(&mut doc, &mut EditTransaction::default());
            Arc::make_mut(&mut doc.layers[layer].vectors).strokes.push(stroke);
        }
    }
    let mut renderer = RasterRenderer::default();
    let expected = renderer.rgba8(&doc);
    let reference = ProjectRef { document: &doc, settings: &settings, tip: &tip, engine: &engine,
        brushes: &[], custom_paper: &None, zoom: 1., pan: [0.; 2] };
    let mut report = String::from("A4 120 DPI; two layers; 20 editable pencil strokes; optimized dev build\n");
    for ext in ["graphite", "psd"] {
        let path = std::path::PathBuf::from(format!("{prefix}.{ext}"));
        let start = Instant::now();
        if ext == "psd" { project::save_psd(&path, &reference, &mut renderer, PsdBitDepth::Sixteen).unwrap(); }
        else { project::save(&path, &reference).unwrap(); }
        let save_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        let loaded = project::load(&path).unwrap();
        let load_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(renderer.rgba8(&loaded.document), expected);
        assert_eq!(loaded.document.layers.iter().map(|l| l.vectors.strokes.len()).sum::<usize>(), 20);
        assert_eq!(loaded.document.surface.current_height, doc.surface.current_height);
        assert_eq!(loaded.document.surface.graphite_mass, doc.surface.graphite_mass);
        if let Some(old_prefix) = std::env::args().nth(2) {
            let old = project::load(&std::path::PathBuf::from(format!("{old_prefix}.{ext}"))).unwrap();
            assert_eq!(renderer.rgba8(&old.document), expected);
            assert_eq!(old.document.surface.current_height, loaded.document.surface.current_height);
            assert_eq!(old.document.surface.graphite_mass, loaded.document.surface.graphite_mass);
        }
        report += &format!("{ext}: save {save_ms:.1} ms; open {load_ms:.1} ms; {} bytes; pixels/material/paths exact\n", std::fs::metadata(&path).unwrap().len());
    }
    std::fs::write(format!("{prefix}.txt"), &report).unwrap();
    print!("{report}");
}
