use graphite_studio::{
    core::{document::{CanvasSpec, Document}, history::EditTransaction, paper::PaperPreset,
        pencil::{PencilTipState, ToolSettings}, stroke::{StrokeEngine, StrokePoint}},
    render::{DocumentRenderer, RasterRenderer},
};

#[test]
fn rendered_width_response_keeps_light_strokes_fine_and_increases_dynamic_range() {
    let mut widths = Vec::new();
    let mut preview = image::RgbaImage::new(240, 480);
    for (row, (response, pressure)) in [(0., 0.15), (0., 0.85), (0.8, 0.15), (0.8, 0.85)].into_iter().enumerate() {
        let spec = CanvasSpec { name: "Width response".into(), width_px: 240, height_px: 120,
            width_mm: 20.32, height_mm: 10.16, dpi: 300. };
        let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; 240 * 120]);
        let settings = ToolSettings { pressure_width: response, ..Default::default() };
        let mut tip = PencilTipState::fresh_for_formulation(2., settings.grade.formulation());
        let mut engine = StrokeEngine::default();
        let from = StrokePoint { x: 20., y: 60., pressure, tilt_deg: 8., azimuth_deg: 0., rotation_deg: None };
        engine.begin_pencil_stroke(from);
        engine.apply_segment(&mut doc, &settings, &mut tip, from, StrokePoint { x: 220., ..from }, &mut EditTransaction::default());
        let covered = (40..200).flat_map(|x| (0..120).map(move |y| y * 240 + x))
            .filter(|&i| doc.surface.graphite_mass[i] > 0.00001).count() as f32 / 160.;
        widths.push(covered);
        let bytes = RasterRenderer::default().rgba8(&doc);
        for y in 0..120 { for x in 0..240 {
            let i = (y * 240 + x) * 4;
            preview.put_pixel(x as u32, (row * 120 + y) as u32, image::Rgba(bytes[i..i+4].try_into().unwrap()));
        } }
    }
    assert!(widths[2] < widths[0] * 0.8, "light strokes should narrow: {widths:?}");
    assert!(widths[3] / widths[2] > widths[1] / widths[0] * 1.2, "pressure range should expand: {widths:?}");
    preview.save("../../width-response-v224.png").unwrap();
}
