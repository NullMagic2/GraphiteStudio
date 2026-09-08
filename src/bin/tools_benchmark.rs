//! CPU eraser/smudge benchmark; compare every material channel and reservoir exactly.
use graphite_studio::core::{
    document::{CanvasSpec, Document},
    history::EditTransaction,
    paper::PaperPreset,
    pencil::{PencilTipState, ToolKind, ToolSettings},
    stroke::{StrokeEngine, StrokePoint},
};
fn main() {
    let prefix = std::env::args().nth(1).unwrap();
    let reference = std::env::args().nth(2);
    let mut report = String::new();
    for tool in [ToolKind::Eraser, ToolKind::Smudge] {
        for size in [100., 200., 400.] {
            let mut times = Vec::new();
            let mut hash = 0u64;
            for run in 0..3 {
                let spec = CanvasSpec::from_physical("Tools", 125., 125., 120.);
                let n = spec.pixel_count();
                let mut doc =
                    Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; n]);
                for i in 0..n {
                    if (i % doc.spec.width_px) % 80 < 35 {
                        doc.surface.graphite_mass[i] = 0.3;
                        doc.surface.clay_mass[i] = 0.12;
                        doc.surface.wax_mass[i] = 0.03;
                        doc.surface.loose_mass[i] = 0.3;
                        doc.surface.compacted_mass[i] = 0.15;
                        doc.surface.color_r_mass[i] = 0.2;
                        doc.surface.color_g_mass[i] = 0.1;
                        doc.surface.color_b_mass[i] = 0.15;
                    }
                }
                let s = ToolSettings {
                    tool,
                    eraser_strength: 0.6,
                    eraser_diameter_mm: size * 25.4 / 120.,
                    smudge_size_px: size,
                    ..Default::default()
                };
                let mut tip = PencilTipState::default();
                let mut engine = StrokeEngine::default();
                let mut tx = EditTransaction::default();
                let point = |i: f32| StrokePoint {
                    x: 220. + i * 8.,
                    y: 240. + (i * 0.2).sin() * 15.,
                    pressure: 0.6,
                    tilt_deg: 8.,
                    azimuth_deg: 20.,
                    rotation_deg: None,
                };
                let start = std::time::Instant::now();
                for i in 1..=12 {
                    engine.apply_segment(
                        &mut doc,
                        &s,
                        &mut tip,
                        point((i - 1) as f32),
                        point(i as f32),
                        &mut tx,
                    );
                }
                times.push(start.elapsed().as_secs_f64() * 1000. / 12.);
                if run == 0 {
                    hash = 0xcbf29ce484222325;
                    for i in 0..n {
                        let p = doc.surface.pixel(i);
                        for b in [
                            p.current_height,
                            p.abrasion,
                            p.graphite_mass,
                            p.clay_mass,
                            p.wax_mass,
                            p.loose_mass,
                            p.compacted_mass,
                            p.orientation_x,
                            p.orientation_y,
                            p.color_r_mass,
                            p.color_g_mass,
                            p.color_b_mass,
                        ]
                        .iter()
                        .flat_map(|v| v.to_le_bytes())
                        {
                            hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
                        }
                    }
                    for b in serde_json::to_vec(&engine).unwrap() {
                        hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
                    }
                }
            }
            times.sort_by(f64::total_cmp);
            let key = format!("{tool:?}-{size:.0}");
            let fingerprint = format!("{hash:016x}");
            if let Some(reference) = &reference {
                assert_eq!(
                    std::fs::read_to_string(format!("{reference}-{key}.hash")).unwrap(),
                    fingerprint
                );
            }
            std::fs::write(format!("{prefix}-{key}.hash"), fingerprint).unwrap();
            let line = format!("{key}: {:.3} ms/segment\n", times[1]);
            print!("{line}");
            report += &line;
        }
    }
    std::fs::write(format!("{prefix}.txt"), report).unwrap();
}
