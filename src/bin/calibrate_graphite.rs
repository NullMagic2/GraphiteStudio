use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use graphite_studio::{
    core::{
        calibration::{CalibrationCase, CalibrationMetrics, CalibrationReport},
        document::{CanvasSpec, Document},
        history::EditTransaction,
        paper::PaperPreset,
        pencil::{PencilGrade, PencilTipState, ToolSettings},
        stroke::{StrokeEngine, StrokePoint},
    },
    render::{DocumentRenderer, RasterRenderer},
};
use image::{Rgba, RgbaImage};

const CELL_W: u32 = 180;
const CELL_H: u32 = 140;
const COLS: usize = 10;
const STROKE_Y: f32 = 82.0;
const STROKE_X0: f32 = 48.0;
const STROKE_X1: f32 = 132.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("calibration-output"));
    fs::create_dir_all(&output_dir)?;
    validate_calibration_baseline()?;

    let cases = build_cases();
    let rows = cases.len().div_ceil(COLS);
    let mut sheet = RgbaImage::from_pixel(
        CELL_W * COLS as u32,
        CELL_H * rows as u32,
        Rgba([255, 255, 255, 255]),
    );
    let mut report = CalibrationReport::default();

    for (case_index, case) in cases.iter().copied().enumerate() {
        let (cell, metrics) = render_case(case)?;
        let col = case_index % COLS;
        let row = case_index / COLS;
        image::imageops::overlay(
            &mut sheet,
            &cell,
            (col as u32 * CELL_W) as i64,
            (row as u32 * CELL_H) as i64,
        );
        report.rows.push((case, metrics));
    }

    sheet.save(output_dir.join("graphite_calibration_swatches.png"))?;
    write_csv(
        &output_dir.join("graphite_calibration_metrics.csv"),
        &report,
    )?;
    write_json(
        &output_dir.join("graphite_calibration_metrics.json"),
        &report,
    )?;

    println!(
        "Rendered {} controlled HB/2B/4B cases to {}",
        report.rows.len(),
        output_dir.display()
    );
    Ok(())
}

fn validate_calibration_baseline() -> Result<(), Box<dyn std::error::Error>> {
    // Calibration must start from physically meaningful invariants, not an arbitrary brush-size
    // sweep. Check a standard 2 mm core at ordinary tablet pressures both upright and laid over,
    // plus the supported core-diameter endpoints.
    for (core_diameter_mm, pressure, tilt_deg) in [
        (1.5, 0.26, 8.0),
        (2.0, 0.26, 8.0),
        (2.0, 0.48, 72.0),
        (4.0, 0.48, 72.0),
    ] {
        let case = CalibrationCase {
            grade: PencilGrade::HB,
            core_diameter_mm,
            pressure,
            tilt_deg,
            pass_count: 1,
        };
        let (_, metrics) = render_case(case)?;
        if metrics.occupied_area_px == 0 || metrics.mean_darkness <= 0.001 {
            return Err(format!(
                "calibration baseline failed at core={core_diameter_mm:.1} mm pressure={pressure:.2} tilt={tilt_deg:.0}: occupied={} mean_darkness={:.6}",
                metrics.occupied_area_px,
                metrics.mean_darkness,
            )
            .into());
        }
    }
    Ok(())
}

fn push_unique(cases: &mut Vec<CalibrationCase>, candidate: CalibrationCase) {
    let duplicate = cases.iter().any(|c| {
        c.grade == candidate.grade
            && (c.core_diameter_mm - candidate.core_diameter_mm).abs() < 1.0e-5
            && (c.pressure - candidate.pressure).abs() < 1.0e-5
            && (c.tilt_deg - candidate.tilt_deg).abs() < 1.0e-5
            && c.pass_count == candidate.pass_count
    });
    if !duplicate {
        cases.push(candidate);
    }
}

fn build_cases() -> Vec<CalibrationCase> {
    // ~70 high-information cases instead of the old 900-case Cartesian product. The large sweep
    // belongs in regression validation; empirical calibration should isolate relationships.
    let grades = [PencilGrade::HB, PencilGrade::B2, PencilGrade::B4];
    let mut cases = Vec::with_capacity(80);

    // 1) Pressure response for each grade, both near-upright and broad-side.
    for grade in grades {
        for tilt_deg in [8.0, 65.0] {
            for pressure in [0.10, 0.25, 0.48, 0.70, 1.00] {
                push_unique(
                    &mut cases,
                    CalibrationCase {
                        grade,
                        core_diameter_mm: 2.0,
                        pressure,
                        tilt_deg,
                        pass_count: 1,
                    },
                );
            }
        }
    }

    // 2) Finer HB tilt mapping at representative light/default/hard pressures.
    for tilt_deg in [8.0, 25.0, 45.0, 65.0, 78.0] {
        for pressure in [0.25, 0.48, 0.85] {
            push_unique(
                &mut cases,
                CalibrationCase {
                    grade: PencilGrade::HB,
                    core_diameter_mm: 2.0,
                    pressure,
                    tilt_deg,
                    pass_count: 1,
                },
            );
        }
    }

    // 3) Repeated-pass buildup/compaction. Pass 1 overlaps the pressure family and is deduplicated.
    for grade in grades {
        for pressure in [0.25, 0.48, 0.85] {
            for pass_count in [1, 3, 6] {
                push_unique(
                    &mut cases,
                    CalibrationCase {
                        grade,
                        core_diameter_mm: 2.0,
                        pressure,
                        tilt_deg: 8.0,
                        pass_count,
                    },
                );
            }
        }
    }

    // 4) Small physical-core check. Core diameter is a material/tool property, not a brush-size
    // axis; only a few endpoints are needed to verify that geometry remains physically bounded.
    for core_diameter_mm in [1.5, 2.0, 3.8] {
        for tilt_deg in [8.0, 65.0] {
            for pressure in [0.48, 0.85] {
                push_unique(
                    &mut cases,
                    CalibrationCase {
                        grade: PencilGrade::HB,
                        core_diameter_mm,
                        pressure,
                        tilt_deg,
                        pass_count: 1,
                    },
                );
            }
        }
    }

    cases
}

fn render_case(
    case: CalibrationCase,
) -> Result<(RgbaImage, CalibrationMetrics), Box<dyn std::error::Error>> {
    let spec = CanvasSpec {
        width_px: CELL_W as usize,
        height_px: CELL_H as usize,
        width_mm: CELL_W as f32 / 120.0 * 25.4,
        height_mm: CELL_H as f32 / 120.0 * 25.4,
        dpi: 120.0,
        name: "calibration-cell".to_owned(),
    };
    let paper = vec![[1.0, 1.0, 1.0]; spec.pixel_count()];
    let mut document = Document::new(spec, PaperPreset::DrawingMedium, "Calibration white", paper);
    let mut settings = ToolSettings::default();
    settings.grade = case.grade;
    settings.pencil_core_diameter_mm = case.core_diameter_mm;
    settings.tilt_deg = case.tilt_deg;
    settings.azimuth_deg = 0.0;
    settings.auto_azimuth = false;
    settings.flow = 1.0;
    settings.particle_variation = 0.0;

    let formulation = case.grade.formulation();
    let mut tip = PencilTipState::fresh_for_formulation(case.core_diameter_mm, formulation);
    let mut engine = StrokeEngine::default();

    for pass in 0..case.pass_count {
        // Alternate direction so repeated passes do not get an artificial one-sided raster bias.
        let left_to_right = pass % 2 == 0;
        let (x0, x1) = if left_to_right {
            (STROKE_X0, STROKE_X1)
        } else {
            (STROKE_X1, STROKE_X0)
        };
        let from = StrokePoint {
            x: x0,
            y: STROKE_Y,
            pressure: case.pressure,
            tilt_deg: case.tilt_deg,
            azimuth_deg: 0.0,
            rotation_deg: None,
        };
        let to = StrokePoint { x: x1, ..from };
        let mut tx = EditTransaction::default();
        engine.begin_pencil_stroke(from);
        let _ = engine.apply_segment(&mut document, &settings, &mut tip, from, to, &mut tx);
        tip.commit_pending_wear();
    }

    let mut renderer = RasterRenderer::default();
    let rgba = renderer.rgba8(&document);
    let mut image = RgbaImage::from_raw(CELL_W, CELL_H, rgba)
        .ok_or("renderer returned an unexpected calibration-cell size")?;

    let metrics = measure_case(&document, &image, tip.wear_fraction());
    let label = format!(
        "{} P{} T{} N{} D{:.1}",
        case.grade.label(),
        (case.pressure * 100.0).round() as u32,
        case.tilt_deg.round() as u32,
        case.pass_count,
        case.core_diameter_mm,
    );
    draw_text_5x7(&mut image, 5, 5, &label, Rgba([28, 28, 28, 255]));
    Ok((image, metrics))
}

fn measure_case(document: &Document, image: &RgbaImage, final_tip_wear: f32) -> CalibrationMetrics {
    let width = document.spec.width_px;
    let height = document.spec.height_px;
    let mut occupied = vec![false; width * height];
    let mut occupied_area = 0usize;
    let mut darkness_sum = 0.0f64;
    let mut tone_sum = 0.0f64;
    let mut tone_sq_sum = 0.0f64;

    for y in 12..height {
        for x in 0..width {
            let index = document.index(x, y);
            if document.surface.total_deposit(index) <= 1.0e-8 {
                continue;
            }
            occupied[index] = true;
            occupied_area += 1;
            let p = image.get_pixel(x as u32, y as u32).0;
            let lum = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
            let darkness = 1.0 - lum;
            darkness_sum += darkness as f64;
            tone_sum += lum as f64;
            tone_sq_sum += (lum * lum) as f64;
        }
    }

    let mut widths = Vec::new();
    let mut top_edges = Vec::new();
    let mut bottom_edges = Vec::new();
    for x in 0..width {
        let ys: Vec<usize> = (12..height)
            .filter(|&y| occupied[document.index(x, y)])
            .collect();
        if let (Some(top), Some(bottom)) = (ys.first(), ys.last()) {
            widths.push((x, *bottom - *top + 1));
            top_edges.push((x, *top as f32));
            bottom_edges.push((x, *bottom as f32));
        }
    }

    let contact_width_px = if widths.is_empty() {
        0.0
    } else {
        widths.iter().map(|(_, w)| *w as f32).sum::<f32>() / widths.len() as f32
    };
    let edge_roughness_px = 0.5 * (edge_mad(&top_edges) + edge_mad(&bottom_edges));
    let taper_length_px = estimate_taper_length(&widths);
    let mean_darkness = if occupied_area > 0 {
        (darkness_sum / occupied_area as f64) as f32
    } else {
        0.0
    };
    let tone_variance = if occupied_area > 0 {
        let mean = tone_sum / occupied_area as f64;
        (tone_sq_sum / occupied_area as f64 - mean * mean).max(0.0) as f32
    } else {
        0.0
    };

    CalibrationMetrics {
        mean_darkness,
        occupied_area_px: occupied_area,
        edge_roughness_px,
        tone_variance,
        contact_width_px,
        taper_length_px,
        final_tip_wear,
    }
}

fn edge_mad(points: &[(usize, f32)]) -> f32 {
    if points.len() < 2 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut count = 0usize;
    for pair in points.windows(2) {
        if pair[1].0 == pair[0].0 + 1 {
            sum += (pair[1].1 - pair[0].1).abs();
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f32
    }
}

fn estimate_taper_length(widths: &[(usize, usize)]) -> f32 {
    if widths.len() < 5 {
        return 0.0;
    }
    let mut sorted_widths: Vec<usize> = widths.iter().map(|(_, w)| *w).collect();
    sorted_widths.sort_unstable();
    let median = sorted_widths[sorted_widths.len() / 2] as f32;
    let threshold = median * 0.90;
    let first_x = widths.first().unwrap().0;
    let last_x = widths.last().unwrap().0;
    let left_full = widths
        .iter()
        .find(|(_, w)| *w as f32 >= threshold)
        .map(|(x, _)| *x)
        .unwrap_or(first_x);
    let right_full = widths
        .iter()
        .rev()
        .find(|(_, w)| *w as f32 >= threshold)
        .map(|(x, _)| *x)
        .unwrap_or(last_x);
    ((left_full - first_x) as f32 + (last_x - right_full) as f32) * 0.5
}

fn write_csv(path: &Path, report: &CalibrationReport) -> std::io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(
        out,
        "grade,core_diameter_mm,pressure,tilt_deg,pass_count,mean_darkness,occupied_area_px,edge_roughness_px,tone_variance,contact_width_px,taper_length_px,final_tip_wear"
    )?;
    for (case, m) in &report.rows {
        writeln!(
            out,
            "{},{:.2},{:.3},{:.1},{},{:.6},{},{:.6},{:.8},{:.6},{:.6},{:.6}",
            case.grade.label(),
            case.core_diameter_mm,
            case.pressure,
            case.tilt_deg,
            case.pass_count,
            m.mean_darkness,
            m.occupied_area_px,
            m.edge_roughness_px,
            m.tone_variance,
            m.contact_width_px,
            m.taper_length_px,
            m.final_tip_wear,
        )?;
    }
    Ok(())
}

fn write_json(path: &Path, report: &CalibrationReport) -> std::io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(out, "[")?;
    for (i, (case, m)) in report.rows.iter().enumerate() {
        writeln!(
            out,
            "  {{\"grade\":\"{}\",\"core_diameter_mm\":{:.2},\"pressure\":{:.3},\"tilt_deg\":{:.1},\"pass_count\":{},\"mean_darkness\":{:.6},\"occupied_area_px\":{},\"edge_roughness_px\":{:.6},\"tone_variance\":{:.8},\"contact_width_px\":{:.6},\"taper_length_px\":{:.6},\"final_tip_wear\":{:.6}}}{}",
            case.grade.label(),
            case.core_diameter_mm,
            case.pressure,
            case.tilt_deg,
            case.pass_count,
            m.mean_darkness,
            m.occupied_area_px,
            m.edge_roughness_px,
            m.tone_variance,
            m.contact_width_px,
            m.taper_length_px,
            m.final_tip_wear,
            if i + 1 == report.rows.len() { "" } else { "," },
        )?;
    }
    writeln!(out, "]")?;
    Ok(())
}

fn draw_text_5x7(image: &mut RgbaImage, mut x: u32, y: u32, text: &str, color: Rgba<u8>) {
    for ch in text.chars() {
        if ch == ' ' {
            x += 4;
            continue;
        }
        if let Some(rows) = glyph(ch) {
            for (row, bits) in rows.iter().enumerate() {
                for col in 0..5u32 {
                    if *bits & (1u8 << (4 - col)) != 0 {
                        let px = x + col;
                        let py = y + row as u32;
                        if px < image.width() && py < image.height() {
                            image.put_pixel(px, py, color);
                        }
                    }
                }
            }
        }
        x += 6;
    }
}

fn glyph(ch: char) -> Option<[u8; 7]> {
    Some(match ch {
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'N' => [
            0b10001, 0b11001, 0b11001, 0b10101, 0b10011, 0b10011, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        _ => return None,
    })
}
