use graphite_studio::{
    core::{
        document::{CanvasSpec, Document},
        history::{EditTransaction, History},
        orientation::QuarterTurn,
        paper::PaperPreset,
        pencil::{PencilTipState, ToolKind, ToolSettings},
        stroke::{StrokeEngine, StrokePoint},
        vector::{VectorLayer, VectorStroke},
    },
    render::{DocumentRenderer, RasterRenderer},
};
use std::sync::Arc;
#[test]
fn orientation_is_lossless_across_layers_paths_selection_and_undo_redo() {
    let spec = CanvasSpec::from_physical("Portrait", 24., 36., 60.);
    let (w, h) = (spec.width_px, spec.height_px);
    let mut doc = Document::new(
        spec,
        PaperPreset::DrawingMedium,
        "White",
        vec![[1.; 3]; w * h],
    );
    let mut history = History::default();
    let mut stroke = VectorStroke {
        label: "Test".into(),
        settings: ToolSettings {
            tool: ToolKind::Brush,
            brush_size_px: 8.,
            ..Default::default()
        },
        tip: PencilTipState::default(),
        engine: StrokeEngine::default(),
        points: vec![
            StrokePoint {
                x: 12.,
                y: 20.,
                pressure: 0.7,
                tilt_deg: 30.,
                azimuth_deg: 12.,
                rotation_deg: Some(20.),
            },
            StrokePoint {
                x: 38.,
                y: 60.,
                pressure: 0.8,
                tilt_deg: 35.,
                azimuth_deg: 14.,
                rotation_deg: Some(30.),
            },
        ],
        polyline: false,
        selection: Default::default(),
    };
    for layer in 0..2 {
        if layer == 1 {
            doc.add_layer();
            stroke.settings.pencil_color_rgb = [210, 45, 30];
        }
        let mut tx = EditTransaction::default();
        stroke.apply(&mut doc, &mut tx);
        doc.layers[layer].vectors = Arc::new(VectorLayer {
            base: None,
            strokes: vec![Arc::new(stroke.clone())],
        });
        doc.layers[layer].opacity = 140 + layer as u8 * 80;
        history.push(tx, &doc);
    }
    doc.selection.set(
        vec![
            eframe::egui::vec2(8., 10.),
            eframe::egui::vec2(48., 12.),
            eframe::egui::vec2(40., 68.),
        ],
        w,
        h,
    );
    let original = doc.clone();
    let mut pixels: Vec<[u8; 4]> = RasterRenderer::default()
        .rgba8(&doc)
        .chunks_exact(4)
        .map(|p| p.try_into().unwrap())
        .collect();
    let turn = QuarterTurn::new(w, h, true);
    turn.grid(&mut pixels);
    history.rotate_document(&mut doc, true);
    assert_eq!((doc.spec.width_px, doc.spec.height_px), (h, w));
    assert_eq!(doc.spec.dpi, 60.);
    let rendered: Vec<[u8; 4]> = RasterRenderer::default()
        .rgba8(&doc)
        .chunks_exact(4)
        .map(|p| p.try_into().unwrap())
        .collect();
    assert_eq!(pixels, rendered);
    for i in 0..w * h {
        assert_eq!(
            original.selection.allows(i),
            doc.selection.allows(turn.index(i))
        );
    }
    for i in 0..2 {
        assert_eq!(doc.layers[i].id, original.layers[i].id);
        assert_eq!(doc.layers[i].opacity, original.layers[i].opacity);
        assert_eq!(doc.layers[i].vectors.strokes.len(), 1);
        let p = doc.layers[i].vectors.strokes[0].points[0];
        assert_eq!((p.x, p.y), (h as f32 - 20., 12.));
    }
    assert!(history.undo(&mut doc));
    assert_eq!((doc.spec.width_px, doc.spec.height_px), (w, h));
    assert_eq!(
        RasterRenderer::default().rgba8(&doc),
        RasterRenderer::default().rgba8(&original)
    );
    assert!(history.undo(&mut doc)); // earlier stroke remains undoable in original coordinates
    assert!(doc.surface.graphite_mass.iter().all(|&v| v == 0.));
    assert!(history.redo(&mut doc));
    assert_eq!(
        RasterRenderer::default().rgba8(&doc),
        RasterRenderer::default().rgba8(&original)
    );
    assert!(history.redo(&mut doc));
    assert_eq!((doc.spec.width_px, doc.spec.height_px), (h, w));
    history.rotate_document(&mut doc, false);
    assert_eq!(
        RasterRenderer::default().rgba8(&doc),
        RasterRenderer::default().rgba8(&original)
    );
}
