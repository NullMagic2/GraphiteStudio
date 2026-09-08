use graphite_studio::core::{
    document::{CanvasSpec, Document},
    history::EditTransaction,
    paper::PaperPreset,
    pencil::{PencilTipState, ToolSettings},
    stroke::{StrokeEngine, StrokePoint},
};

fn line(dpi: f32, packets: usize, pressure: f32, tilt: f32, azimuth: f32) -> Document {
    let spec = CanvasSpec::from_physical("Test", 32.0, 14.0, dpi);
    let n = spec.pixel_count();
    let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.0; 3]; n]);
    let settings = ToolSettings::default();
    let mut tip = PencilTipState::fresh(2.0);
    let mut engine = StrokeEngine::default();
    let mut tx = EditTransaction::default();
    let from = StrokePoint {
        x: 5.0 * dpi / 25.4,
        y: 8.0 * dpi / 25.4,
        pressure,
        tilt_deg: tilt,
        azimuth_deg: azimuth,
        rotation_deg: None,
    };
    let to = StrokePoint {
        x: 27.0 * dpi / 25.4,
        ..from
    };
    for j in 0..packets {
        engine.apply_segment(
            &mut doc,
            &settings,
            &mut tip,
            from.lerp(to, j as f32 / packets as f32),
            from.lerp(to, (j + 1) as f32 / packets as f32),
            &mut tx,
        );
    }
    doc
}
fn mass(doc: &Document) -> f32 {
    doc.surface.graphite_mass.iter().sum::<f32>() / (doc.spec.dpi / 25.4).powi(2)
}

#[test]
fn input_packet_rate_does_not_change_deposited_mass() {
    for tilt in [8.0, 72.0] {
        let coarse = line(120.0, 12, 0.45, tilt, 85.0);
        let dense = line(120.0, 900, 0.45, tilt, 85.0);
        let error = (mass(&coarse) - mass(&dense)).abs() / mass(&dense);
        assert!(error < 0.025, "tilt {tilt}: packet mass error {error}");
    }
}
#[test]
fn physical_deposition_is_consistent_across_dpi() {
    for tilt in [8.0, 72.0] {
        let low = line(120.0, 180, 0.45, tilt, 85.0);
        let high = line(300.0, 180, 0.45, tilt, 85.0);
        let error = (mass(&low) - mass(&high)).abs() / mass(&high);
        assert!(error < 0.15, "tilt {tilt}: DPI mass error {error}");
    }
}
#[test]
fn a_stationary_motion_packet_does_not_add_material() {
    let mut doc = line(120.0, 60, 0.45, 8.0, 0.0);
    let before = doc.surface.graphite_mass.clone();
    let p = StrokePoint {
        x: 40.0,
        y: 40.0,
        pressure: 0.8,
        tilt_deg: 8.0,
        azimuth_deg: 0.0,
        rotation_deg: None,
    };
    let mut engine = StrokeEngine::default();
    for _ in 0..100 {
        engine.apply_segment(
            &mut doc,
            &ToolSettings::default(),
            &mut PencilTipState::fresh(2.0),
            p,
            p,
            &mut EditTransaction::default(),
        );
    }
    assert_eq!(before, doc.surface.graphite_mass);
}
#[test]
fn light_marks_follow_the_sheet_relief() {
    let doc = line(300.0, 180, 0.20, 72.0, 85.0);
    let mut peaks = Vec::new();
    let mut valleys = Vec::new();
    for y in 52..82 {
        for x in 110..250 {
            let i = doc.index(x, y);
            if doc.surface.rest_height[i] > 0.56 {
                peaks.push(doc.surface.graphite_mass[i]);
            }
            if doc.surface.rest_height[i] < 0.44 {
                valleys.push(doc.surface.graphite_mass[i]);
            }
        }
    }
    assert!(!peaks.is_empty() && !valleys.is_empty());
    let peak = peaks.iter().sum::<f32>() / peaks.len() as f32;
    let valley = valleys.iter().sum::<f32>() / valleys.len() as f32;
    assert!(peak > valley * 2.0, "peak {peak}, valley {valley}");
}

#[test]
fn graphite_has_related_light_and_deep_tones_without_white_interior_cutouts() {
    use graphite_studio::render::{DocumentRenderer, RasterRenderer};
    let doc = line(300.0, 180, 0.45, 72.0, 85.0);
    let mut tones = Vec::new();
    let image = RasterRenderer::default().render_full(&doc);
    let mut bright = 0;
    for y in 55..80 {
        for x in 110..250 {
            let i = doc.index(x, y);
            let total = doc.surface.total_deposit(i);
            if total > 1e-6 {
                tones.push(doc.surface.color_r_mass[i] / total);
            }
            if image[(x, y)].r() > 245 {
                bright += 1;
            }
        }
    }
    assert!(
        tones.iter().any(|&t| t < 0.38),
        "must contain deeper shades of the selected graphite"
    );
    assert!(
        tones.iter().any(|&t| t > 0.44),
        "must contain lighter related shades"
    );
    assert!(tones.iter().all(|&t| t > 0.27 && t < 0.51));
    assert!(
        bright < 100,
        "too many near-white pixels inside an ordinary shading stroke: {bright}"
    );
}

#[test]
fn layer_reordering_preserves_active_and_inactive_material() {
    let mut doc = line(120.0, 60, 0.45, 8.0, 0.0);
    let first = doc.layers[0].id;
    let first_mass = doc.surface.graphite_mass.iter().sum::<f32>();
    doc.add_layer();
    let second = doc.layers[1].id;
    doc.surface.graphite_mass[0] = 0.2;
    doc.surface.loose_mass[0] = 0.2;
    assert!(doc.move_active_layer_down());
    assert_eq!(doc.layers[doc.active_layer_index()].id, second);
    assert_eq!(doc.layer_deposit_pixel(0, 0).graphite_mass, 0.2);
    assert!(doc.activate_layer_by_id(first));
    assert!((doc.surface.graphite_mass.iter().sum::<f32>() - first_mass).abs() < 1e-5);
    assert!(doc.move_active_layer_down());
    assert!(doc.set_layer_visible(0, false));
    assert!(doc.activate_layer_by_id(second));
    assert_eq!(doc.surface.graphite_mass[0], 0.2);
    assert!(doc.remove_active_layer());
    assert_eq!(doc.layer_count(), 1);
    assert_eq!(doc.layers[0].id, first);
    assert!((doc.surface.graphite_mass.iter().sum::<f32>() - first_mass).abs() < 1e-5);
    assert!(!doc.remove_active_layer());
}

#[test]
fn dragging_layers_keeps_identity_material_visibility_and_blend_mode() {
    use graphite_studio::core::document::BlendMode;
    let mut doc = line(120.0, 60, 0.45, 8.0, 0.0);
    let first = doc.active_layer_id();
    let original = doc.surface.graphite_mass.clone();
    doc.add_layer();
    let second = doc.active_layer_id();
    doc.surface.graphite_mass[0] = 0.123;
    doc.set_layer_blend_mode(second, BlendMode::Screen);
    doc.set_layer_visible(doc.active_layer_index(), false);
    doc.add_layer();
    let third = doc.active_layer_id();
    doc.surface.graphite_mass[0] = 0.456;
    assert!(doc
        .layers
        .iter()
        .filter(|l| l.id != second)
        .all(|l| l.blend_mode == BlendMode::Multiply));
    assert!(doc.move_layer_relative(second, third, true));
    assert_eq!(
        doc.layers.iter().map(|l| l.id).collect::<Vec<_>>(),
        vec![first, third, second]
    );
    assert_eq!(doc.active_layer_id(), third);
    assert_eq!(doc.surface.graphite_mass[0], 0.456);
    assert!(doc.move_layer_relative(third, first, false));
    assert_eq!(doc.active_layer_id(), third);
    assert_eq!(doc.surface.graphite_mass[0], 0.456);
    assert!(!doc.move_layer_relative(third, third, false));
    assert!(!doc.move_layer_relative(99, first, true));
    doc.activate_layer_by_id(second);
    assert_eq!(doc.surface.graphite_mass[0], 0.123);
    assert!(!doc.layers[doc.active_layer_index()].visible);
    assert_eq!(
        doc.layers[doc.active_layer_index()].blend_mode,
        BlendMode::Screen
    );
    doc.activate_layer_by_id(first);
    assert_eq!(doc.surface.graphite_mass, original);
}
