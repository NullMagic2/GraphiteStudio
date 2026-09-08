//! Reproducible marks through the production engine, with identical paths between versions.
use graphite_studio::{
    core::{
        document::{CanvasSpec, Document},
        history::EditTransaction,
        paper::PaperPreset,
        pencil::{PencilGrade, PencilTipState, ToolSettings},
        stroke::{StrokeEngine, StrokePoint},
    },
    render::{DocumentRenderer, RasterRenderer},
};
use image::{Rgba, RgbaImage};
use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or("study".into());
    fs::create_dir_all(&out)?;
    for dpi in [120.0, 300.0] {
        let scale = dpi / 300.0;
        let (w, h) = ((1200.0 * scale) as usize, (750.0 * scale) as usize);
        let spec = CanvasSpec {
            width_px: w,
            height_px: h,
            width_mm: 101.6,
            height_mm: 63.5,
            dpi,
            name: "Realism study".into(),
        };
        let mut doc = Document::new(
            spec,
            PaperPreset::DrawingMedium,
            "White",
            vec![[1.0; 3]; w * h],
        );
        for (col, grade) in [PencilGrade::HB, PencilGrade::B2, PencilGrade::B4]
            .into_iter()
            .enumerate()
        {
            for (row, pressure) in [0.15, 0.45, 0.85].into_iter().enumerate() {
                let settings = ToolSettings {
                    grade,
                    auto_azimuth: false,
                    ..Default::default()
                };
                let mut tip = PencilTipState::fresh_for_formulation(2.0, grade.formulation());
                let mut engine = StrokeEngine::default();
                let ox = col as f32 * 400.0;
                let oy = row as f32 * 250.0;
                for (tilt, y, passes) in [(8.0, 65.0, 1), (72.0, 135.0, 1), (72.0, 215.0, 3)] {
                    for pass in 0..passes {
                        let mut tx = EditTransaction::default();
                        let mut prev = StrokePoint {
                            x: (ox + 45.0) * scale,
                            y: (oy + y) * scale,
                            pressure: 0.0,
                            tilt_deg: tilt,
                            azimuth_deg: 85.0,
                            rotation_deg: None,
                        };
                        engine.begin_pencil_stroke(prev);
                        for j in 1..=160 {
                            let t = j as f32 / 160.0;
                            let envelope = (t / 0.09).min(1.0) * ((1.0 - t) / 0.13).min(1.0);
                            let point = StrokePoint {
                                x: (ox + 45.0 + 310.0 * t) * scale,
                                y: (oy + y + 3.5 * (t * 5.0).sin() + pass as f32 * 1.2) * scale,
                                pressure: pressure * envelope,
                                ..prev
                            };
                            engine
                                .apply_segment(&mut doc, &settings, &mut tip, prev, point, &mut tx);
                            prev = point;
                        }
                        tip.commit_pending_wear();
                    }
                }
            }
        }
        let mut renderer = RasterRenderer::default();
        let mut img = RgbaImage::from_raw(w as u32, h as u32, renderer.rgba8(&doc)).unwrap();
        for (col, grade) in ["HB", "2B", "4B"].into_iter().enumerate() {
            for (row, pressure) in [15, 45, 85].into_iter().enumerate() {
                draw_text_5x7(
                    &mut img,
                    ((col * 400 + 15) as f32 * scale) as u32,
                    ((row * 250 + 10) as f32 * scale) as u32,
                    &format!("{grade} P{pressure}"),
                    Rgba([50, 50, 50, 255]),
                );
            }
        }
        img.save(out.join(format!("study-{dpi:.0}dpi.png")))?;
    }
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
