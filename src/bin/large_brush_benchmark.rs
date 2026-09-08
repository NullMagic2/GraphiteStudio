//! Large sampled-pencil workload with exact material, wear, image and undo fingerprints.
use graphite_studio::{
    core::{
        document::{CanvasSpec, Document},
        history::{EditTransaction, History},
        paper::PaperPreset,
        pencil::{PencilTipState, ToolSettings},
        stroke::{StrokeEngine, StrokePoint},
    },
    render::{DocumentRenderer, RasterRenderer},
};
use std::time::Instant;

fn fingerprint(doc: &Document, tip: &PencilTipState, pixels: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    let mut add = |bytes: &[u8]| {
        for &v in bytes {
            hash = (hash ^ v as u64).wrapping_mul(0x100000001b3);
        }
    };
    for values in [
        &doc.surface.current_height,
        &doc.surface.abrasion,
        &doc.surface.graphite_mass,
        &doc.surface.clay_mass,
        &doc.surface.wax_mass,
        &doc.surface.loose_mass,
        &doc.surface.compacted_mass,
        &doc.surface.orientation_x,
        &doc.surface.orientation_y,
        &doc.surface.color_r_mass,
        &doc.surface.color_g_mass,
        &doc.surface.color_b_mass,
    ] {
        for v in values {
            add(&v.to_le_bytes());
        }
    }
    add(&serde_json::to_vec(tip).unwrap());
    add(pixels);
    format!("{hash:016x}")
}

fn main() {
    let prefix = std::env::args().nth(1).expect("output prefix");
    let reference = std::env::args().nth(2).filter(|s| s != "-");
    let texture = if let Some(path) = std::env::args().nth(3) {
        graphite_studio::core::brush::import_abr(&std::fs::read(path).unwrap(), "Benchmark")
            .unwrap()
            .remove(0)
    } else {
        std::sync::Arc::new(graphite_studio::core::brush::BrushTip {
            name: "Sampled tip".into(),
            width: 256,
            height: 128,
            mask: (0..32768)
                .map(|i| {
                    if (90..166).contains(&(i % 256)) && (40..88).contains(&(i / 256)) {
                        0
                    } else {
                        140 + (i * 73 % 116) as u8
                    }
                })
                .collect(),
        })
    };
    let mut report =
        String::from("24 eight-pixel segments; median of 3; optimized dev; engine only\n");
    for (dpi, core) in [
        (120., 50. * 25.4 / 120.),
        (120., 100. * 25.4 / 120.),
        (120., 200. * 25.4 / 120.),
    ] {
        for tilt in [8., 65.] {
            let key = format!("{dpi:.0}-{core:.1}-{tilt:.0}");
            let mut timings = Vec::new();
            let mut expected = None;
            for run in 0..3 {
                let spec = CanvasSpec::from_physical("Pencil benchmark", 145., 125., dpi);
                let n = spec.pixel_count();
                let mut doc =
                    Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; n]);
                let settings = ToolSettings {
                    pencil_texture: Some(texture.clone()),
                    ..Default::default()
                };
                let mut tip = PencilTipState::fresh(core);
                tip.profile.set_orientation(37.);
                let mut engine = StrokeEngine::default();
                let mut tx = EditTransaction::default();
                let point = |i: usize| StrokePoint {
                    x: 170. + i as f32 * 8.,
                    y: 180. + (i as f32 * 0.2).sin() * 15.,
                    pressure: 0.25 + i as f32 * 0.025,
                    tilt_deg: tilt,
                    azimuth_deg: i as f32 * 7.,
                    rotation_deg: Some(37.),
                };
                let start = Instant::now();
                for i in 1..=24 {
                    engine.apply_segment(
                        &mut doc,
                        &settings,
                        &mut tip,
                        point(i - 1),
                        point(i),
                        &mut tx,
                    );
                }
                timings.push(start.elapsed().as_secs_f64() * 1000. / 24.);
                tip.commit_pending_wear();
                let pixels = RasterRenderer::default().rgba8(&doc);
                let hash = fingerprint(&doc, &tip, &pixels);
                if let Some(before) = &expected {
                    assert_eq!(before, &hash);
                }
                expected = Some(hash.clone());
                if run == 0 {
                    if let Some(reference) = &reference {
                        assert_eq!(
                            std::fs::read_to_string(format!("{reference}-{key}.hash")).unwrap(),
                            hash,
                            "Material, wear or pixels changed for {key}"
                        );
                    }
                    std::fs::write(format!("{prefix}-{key}.hash"), &hash).unwrap();
                    image::save_buffer(
                        format!("{prefix}-{key}.png"),
                        &pixels,
                        doc.spec.width_px as u32,
                        doc.spec.height_px as u32,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                    let mut history = History::default();
                    history.push(tx, &doc);
                    assert!(history.undo(&mut doc));
                    assert!(doc.surface.graphite_mass.iter().all(|&v| v == 0.));
                    assert!(history.redo(&mut doc));
                    assert_eq!(
                        fingerprint(&doc, &tip, &RasterRenderer::default().rgba8(&doc)),
                        hash
                    );
                }
            }
            timings.sort_by(f64::total_cmp);
            let line = format!(
                "{key}: {:.3} ms/segment; material/wear/pixels {}\n",
                timings[1],
                expected.unwrap()
            );
            print!("{line}");
            report += &line;
        }
    }
    std::fs::write(format!("{prefix}.txt"), report).unwrap();
}
