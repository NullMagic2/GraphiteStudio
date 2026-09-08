//! Repeatable CPU rendering timings, excluding document creation and file IO.
use graphite_studio::{
    core::{
        document::{CanvasSpec, DirtyRect, Document},
        paper::PaperPreset,
    },
    render::{DocumentRenderer, RasterRenderer},
};
use std::{hint::black_box, time::Instant};
fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}
fn main() {
    let spec = CanvasSpec::a5(300.0);
    let n = spec.pixel_count();
    let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.0; 3]; n]);
    for i in 0..n / 2 {
        let mass = 0.12 + (i % 37) as f32 * 0.015;
        doc.surface.graphite_mass[i] = mass * 0.68;
        doc.surface.clay_mass[i] = mass * 0.27;
        doc.surface.wax_mass[i] = mass * 0.05;
        doc.surface.loose_mass[i] = mass * 0.55;
        doc.surface.compacted_mass[i] = mass * 0.45;
        doc.surface.color_r_mass[i] = mass * 0.408;
        doc.surface.color_g_mass[i] = mass * 0.408;
        doc.surface.color_b_mass[i] = mass * 0.408;
        doc.surface.orientation_x[i] = mass * 0.48;
        doc.surface.orientation_y[i] = mass * 0.36;
    }
    let mut renderer = RasterRenderer::default();
    black_box(renderer.render_full(&doc));
    let mut times = vec![];
    for _ in 0..5 {
        let t = Instant::now();
        black_box(renderer.render_full(&doc));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("full_render_median_ms={:.3}", median(times));
    let mut times = vec![];
    for _ in 0..31 {
        let t = Instant::now();
        black_box(renderer.render_region(
            &doc,
            DirtyRect {
                min_x: 64,
                min_y: 64,
                max_x: 320,
                max_y: 192,
            },
        ));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("256x128_patch_median_ms={:.3}", median(times));
    let mut times = vec![];
    for _ in 0..11 {
        let t = Instant::now();
        black_box((doc.graphite_coverage(), doc.mean_paper_disturbance()));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("sidebar_scan_median_ms={:.3}", median(times));
    println!(
        "document={}x{}; half covered; 300 DPI; optimized dev build",
        doc.spec.width_px, doc.spec.height_px
    );
    for _ in 0..5 { doc.add_layer(); }
    let mut times = vec![];
    for _ in 0..5 {
        let t = Instant::now();
        black_box(renderer.render_full(&doc));
        times.push(t.elapsed().as_secs_f64() * 1000.);
    }
    println!("six_layer_full_render_median_ms={:.3}", median(times));
    if let Some(path) = std::env::args().nth(1) {
        let rgba = renderer.rgba8(&doc);
        if let Some(reference) = std::env::args().nth(2) {
            assert_eq!(std::fs::read(reference).unwrap(), rgba, "Layer render pixels changed");
            println!("six_layer_pixels=identical");
        }
        std::fs::write(path, rgba).unwrap();
    }
}
