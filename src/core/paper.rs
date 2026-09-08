use std::path::Path;

use image::{imageops::FilterType, DynamicImage, ImageReader, RgbaImage};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperPreset {
    SmoothBristol,
    DrawingMedium,
    RoughSketch,
}

impl PaperPreset {
    pub const ALL: [Self; 3] = [Self::SmoothBristol, Self::DrawingMedium, Self::RoughSketch];

    pub fn label(self) -> &'static str {
        match self {
            Self::SmoothBristol => "Smooth Bristol",
            Self::DrawingMedium => "Drawing paper",
            Self::RoughSketch => "Rough sketch paper",
        }
    }

    pub fn tooth_strength(self) -> f32 {
        match self {
            Self::SmoothBristol => 0.34,
            Self::DrawingMedium => 0.62,
            Self::RoughSketch => 0.92,
        }
    }

    pub fn fiber_strength(self) -> f32 {
        match self {
            Self::SmoothBristol => 0.18,
            Self::DrawingMedium => 0.42,
            Self::RoughSketch => 0.62,
        }
    }

    pub fn compliance(self) -> f32 {
        match self {
            Self::SmoothBristol => 0.30,
            Self::DrawingMedium => 0.52,
            Self::RoughSketch => 0.70,
        }
    }

    pub fn abrasion_resistance(self) -> f32 {
        match self {
            Self::SmoothBristol => 0.82,
            Self::DrawingMedium => 0.68,
            Self::RoughSketch => 0.56,
        }
    }

    pub fn capture_bias(self) -> f32 {
        match self {
            Self::SmoothBristol => 0.72,
            Self::DrawingMedium => 0.91,
            Self::RoughSketch => 1.08,
        }
    }

    fn seed(self) -> u32 {
        match self {
            Self::SmoothBristol => 0x25A1_91B3,
            Self::DrawingMedium => 0x6CB4_713D,
            Self::RoughSketch => 0xA307_E95F,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperTexturePreset {
    White,
    Recycled,
    Ivory,
}

impl PaperTexturePreset {
    pub const ALL: [Self; 3] = [Self::White, Self::Recycled, Self::Ivory];

    pub fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Recycled => "Recycled",
            Self::Ivory => "Ivory",
        }
    }

    fn seed(self) -> u32 {
        match self {
            Self::White => 0xF3A2_117D,
            Self::Recycled => 0x6F42_B1E9,
            Self::Ivory => 0xAE81_37C5,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CustomPaperTextureSource {
    pub name: String,
    #[serde(with = "project_image")]
    image: RgbaImage,
}

impl CustomPaperTextureSource {
    pub(crate) fn project_size_valid(&self) -> bool {
        u64::from(self.image.width()) * u64::from(self.image.height()) <= 32_000_000
    }

    pub fn label(&self) -> String {
        format!("Custom: {}", self.name)
    }

    pub fn render_albedo(&self, width: usize, height: usize) -> Vec<[f32; 3]> {
        let resized = image::imageops::resize(
            &self.image,
            width.max(1) as u32,
            height.max(1) as u32,
            FilterType::CatmullRom,
        );
        resized
            .pixels()
            .map(|p| {
                let a = p[3] as f32 / 255.0;
                let bg = 1.0;
                [
                    ((p[0] as f32 / 255.0) * a + bg * (1.0 - a)).clamp(0.0, 1.0),
                    ((p[1] as f32 / 255.0) * a + bg * (1.0 - a)).clamp(0.0, 1.0),
                    ((p[2] as f32 / 255.0) * a + bg * (1.0 - a)).clamp(0.0, 1.0),
                ]
            })
            .collect()
    }
}

pub fn load_custom_texture_source(path: &Path) -> Result<CustomPaperTextureSource, String> {
    let reader =
        ImageReader::open(path).map_err(|error| format!("failed to open image: {error}"))?;
    let image = reader
        .decode()
        .map_err(|error| format!("failed to decode image: {error}"))?;
    let rgba = flatten_dynamic_to_rgba(image);
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("texture")
        .to_owned();
    Ok(CustomPaperTextureSource { name, image: rgba })
}

fn flatten_dynamic_to_rgba(image: DynamicImage) -> RgbaImage {
    image.to_rgba8()
}

pub fn generate_paper(
    width: usize,
    height: usize,
    dpi: f32,
    preset: PaperPreset,
) -> (Vec<f32>, Vec<f32>) {
    let mut tooth = vec![0.5; width * height];
    let mut fiber = vec![0.5; width * height];
    let seed = preset.seed();
    let px_per_mm = (dpi / 25.4).max(0.001);
    let tooth_strength = preset.tooth_strength();
    let fiber_strength = preset.fiber_strength();
    let fiber_field = generate_fibers(width, height, px_per_mm, seed);

    for y in 0..height {
        for x in 0..width {
            let x_mm = x as f32 / px_per_mm;
            let y_mm = y as f32 / px_per_mm;

            // Fiber bundles cross at oblique angles; an axis-aligned cloud field looks like
            // mottled paint. Integrate four physical locations per pixel to reduce aliasing.
            let mut relief = 0.0;
            let mut fibers = 0.0;
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let px = x_mm + sx / px_per_mm;
                let py = y_mm + sy / px_per_mm;
                let warp = value_noise(px / 0.85, py / 0.85, seed ^ 0xA9) - 0.5;
                let warp_y = value_noise(px / 0.67, py / 0.67, seed ^ 0x53) - 0.5;
                let grain = value_noise(
                    px / 0.18 + warp * 1.5,
                    py / 0.18 + warp_y * 1.5,
                    seed ^ 0x01,
                );
                let fine = value_noise(px / 0.075 + warp_y, py / 0.075 - warp, seed ^ 0xC7);
                let formation = value_noise(px / 0.81, py / 0.81, seed ^ 0x17);
                fibers += fiber_field[y * width + x] * 0.25;
                relief += (0.48 * grain
                    + 0.12 * formation
                    + 0.20 * fine
                    + 0.20 * fiber_field[y * width + x])
                    * 0.25;
            }

            let i = y * width + x;
            tooth[i] = (0.5 + (relief - 0.5) * tooth_strength * 2.8).clamp(0.02, 0.98);
            fiber[i] = (0.5 + (fibers - 0.5) * fiber_strength * 1.45).clamp(0.0, 1.0);
        }
    }

    (tooth, fiber)
}

/// Finite fiber bundles with individually varied orientation and length. Unlike
/// crossed anisotropic noise bands these cannot make a woven diagonal grid.
fn generate_fibers(width: usize, height: usize, px_per_mm: f32, seed: u32) -> Vec<f32> {
    let mut field = vec![0.0f32; width * height];
    let cell = 0.32;
    let cols = (width as f32 / px_per_mm / cell).ceil() as i32;
    let rows = (height as f32 / px_per_mm / cell).ceil() as i32;
    for cy in -2..rows + 2 {
        for cx in -2..cols + 2 {
            let mx = (cx as f32 + hash01(cx, cy, seed ^ 0x14)) * cell;
            let my = (cy as f32 + hash01(cx, cy, seed ^ 0x25)) * cell;
            let angle = hash01(cx, cy, seed ^ 0x39) * std::f32::consts::TAU;
            let (sn, cs) = angle.sin_cos();
            let length = 0.18 + 0.30 * hash01(cx, cy, seed ^ 0x41);
            let radius = 0.018 + 0.028 * hash01(cx, cy, seed ^ 0x57);
            let rx = (length * cs.abs() + radius * sn.abs()) * px_per_mm;
            let ry = (length * sn.abs() + radius * cs.abs()) * px_per_mm;
            let x0 = (mx * px_per_mm - rx - 1.0).floor().max(0.0) as usize;
            let y0 = (my * px_per_mm - ry - 1.0).floor().max(0.0) as usize;
            let x1 = (mx * px_per_mm + rx + 1.0).ceil().clamp(0.0, width as f32) as usize;
            let y1 = (my * px_per_mm + ry + 1.0).ceil().clamp(0.0, height as f32) as usize;
            for y in y0..y1 {
                for x in x0..x1 {
                    for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                        let dx = (x as f32 + sx) / px_per_mm - mx;
                        let dy = (y as f32 + sy) / px_per_mm - my;
                        let u = (dx * cs + dy * sn) / length;
                        let v = (-dx * sn + dy * cs) / radius;
                        let a = (1.0 - u * u).max(0.0);
                        let b = (1.0 - v * v).max(0.0);
                        field[y * width + x] += 0.25 * a * a * b * b;
                    }
                }
            }
        }
    }
    for v in &mut field {
        *v = (0.38 + *v * 0.75).clamp(0.0, 1.0);
    }
    field
}

/// Precomputed mesoscopic paper response fields used by the real-time pencil solver.
///
/// These fields are *not* literal fibers or holes. One document pixel is much larger than an
/// individual cellulose fiber at ordinary drawing DPI, so each value represents the statistical
/// fraction/arrangement of unresolved fiber tops inside that pixel. Keeping the response
/// continuous prevents the old bright square "pore" artifacts while preserving tooth variation.
pub fn generate_contact_response(
    width: usize,
    height: usize,
    preset: PaperPreset,
    rest_height: &[f32],
    fiber: &[f32],
) -> (Vec<u8>, Vec<u8>) {
    let n = width.saturating_mul(height);
    assert_eq!(rest_height.len(), n);
    assert_eq!(fiber.len(), n);
    let mut support = vec![184u8; n];
    let mut edge_grain = vec![128u8; n];
    let edge_gain = 0.7 + preset.tooth_strength();

    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let relief = rest_height[i] - 0.5;
            let fibers = fiber[i] - 0.5;
            // All response channels derive from the same sheet, without pixel-grid random noise.
            let contact = (0.5 + relief * 0.85 + fibers * 0.3).clamp(0.02, 0.98);
            support[i] = (contact * 255.0).round() as u8;

            let edge = (0.5 + relief * edge_gain + fibers * edge_gain * 0.30).clamp(0.05, 0.95);
            edge_grain[i] = (edge * 255.0).round() as u8;
        }
    }

    (support, edge_grain)
}

pub fn generate_builtin_albedo(
    width: usize,
    height: usize,
    dpi: f32,
    preset: PaperTexturePreset,
) -> Vec<[f32; 3]> {
    let mut albedo = vec![[1.0, 1.0, 1.0]; width * height];
    let seed = preset.seed();
    let px_per_mm = (dpi / 25.4).max(0.001);

    for y in 0..height {
        for x in 0..width {
            let x_mm = x as f32 / px_per_mm;
            let y_mm = y as f32 / px_per_mm;
            let broad = value_noise(x_mm / 5.8, y_mm / 5.8, seed ^ 0x03);
            let medium = value_noise(x_mm / 1.2, y_mm / 1.2, seed ^ 0x11);
            let fine = value_noise(x_mm / 0.24, y_mm / 0.24, seed ^ 0x29);
            let fiber = value_noise(x_mm / 2.6, y_mm / 0.31, seed ^ 0x53);
            let fleck_source = value_noise(x_mm / 0.42, y_mm / 0.42, seed ^ 0x97);
            let fleck = (((fleck_source - 0.86) / 0.14).max(0.0)).powf(2.0);
            let i = y * width + x;
            albedo[i] = match preset {
                PaperTexturePreset::White => {
                    let base = 0.986
                        + (broad - 0.5) * 0.012
                        + (medium - 0.5) * 0.008
                        + (fine - 0.5) * 0.006;
                    let warmth = (fiber - 0.5) * 0.004;
                    [
                        (base + warmth).clamp(0.94, 1.0),
                        base.clamp(0.94, 1.0),
                        (base - 0.006 - warmth * 0.5).clamp(0.93, 1.0),
                    ]
                }
                PaperTexturePreset::Recycled => {
                    let base = 0.918
                        + (broad - 0.5) * 0.040
                        + (medium - 0.5) * 0.020
                        + (fine - 0.5) * 0.012;
                    let warm = (fiber - 0.5) * 0.016;
                    let speck_drop = fleck * 0.075;
                    [
                        (base - 0.008 + warm * 0.8 - speck_drop).clamp(0.72, 0.99),
                        (base - 0.020 + warm * 0.25 - speck_drop * 0.92).clamp(0.70, 0.98),
                        (base - 0.050 - warm * 0.6 - speck_drop * 0.84).clamp(0.66, 0.96),
                    ]
                }
                PaperTexturePreset::Ivory => {
                    let base = 0.970
                        + (broad - 0.5) * 0.022
                        + (medium - 0.5) * 0.010
                        + (fine - 0.5) * 0.008;
                    let warmth = 0.014 + (fiber - 0.5) * 0.006;
                    [
                        base.clamp(0.90, 1.0),
                        (base - 0.010 + warmth * 0.25).clamp(0.88, 1.0),
                        (base - 0.035 - warmth * 0.5).clamp(0.84, 0.99),
                    ]
                }
            };
        }
    }

    albedo
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = smoothstep(x - x.floor());
    let ty = smoothstep(y - y.floor());

    let a = hash01(x0, y0, seed);
    let b = hash01(x0 + 1, y0, seed);
    let c = hash01(x0, y0 + 1, seed);
    let d = hash01(x0 + 1, y0 + 1, seed);

    let top = lerp(a, b, tx);
    let bottom = lerp(c, d, tx);
    lerp(top, bottom, ty)
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hash01(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = seed ^ (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h as f32) / (u32::MAX as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_generation_is_deterministic() {
        let a = generate_paper(32, 32, 120.0, PaperPreset::DrawingMedium);
        let b = generate_paper(32, 32, 120.0, PaperPreset::DrawingMedium);
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
    }

    #[test]
    fn paper_fields_stay_normalized() {
        let (tooth, fiber) = generate_paper(64, 48, 120.0, PaperPreset::RoughSketch);
        assert!(tooth.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(fiber.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn paper_scale_matches_area_averaged_higher_resolution() {
        let low = generate_paper(128, 128, 120.0, PaperPreset::DrawingMedium);
        let high = generate_paper(256, 256, 240.0, PaperPreset::DrawingMedium);
        // Pixels represent areas, so compare against a 2x2 average, not one high-DPI point.
        let mut error = 0.0;
        for y in 4..124 {
            for x in 4..124 {
                let hi = 2 * y * 256 + 2 * x;
                let average =
                    (high.0[hi] + high.0[hi + 1] + high.0[hi + 256] + high.0[hi + 257]) * 0.25;
                error += (low.0[y * 128 + x] - average).abs();
            }
        }
        assert!(error / (120.0 * 120.0) < 0.055);
    }

    #[test]
    fn contact_response_is_continuous_and_compact() {
        let (height, fiber) = generate_paper(48, 48, 300.0, PaperPreset::DrawingMedium);
        let (support, edge) =
            generate_contact_response(48, 48, PaperPreset::DrawingMedium, &height, &fiber);
        assert_eq!(support.len(), 48 * 48);
        assert_eq!(edge.len(), 48 * 48);
        assert!(support.iter().all(|&v| v > 0 && v < 255));
        assert!(edge.iter().all(|&v| v > 0 && v < 255));
    }
    #[test]
    fn white_paper_is_lighter_than_recycled() {
        let white = generate_builtin_albedo(16, 16, 120.0, PaperTexturePreset::White);
        let recycled = generate_builtin_albedo(16, 16, 120.0, PaperTexturePreset::Recycled);
        let white_mean = white
            .iter()
            .map(|rgb| (rgb[0] + rgb[1] + rgb[2]) / 3.0)
            .sum::<f32>()
            / white.len() as f32;
        let recycled_mean = recycled
            .iter()
            .map(|rgb| (rgb[0] + rgb[1] + rgb[2]) / 3.0)
            .sum::<f32>()
            / recycled.len() as f32;
        assert!(white_mean > recycled_mean);
    }
}

/// Continuous pigment grain in physical coordinates. Averaging subpixel samples
/// preserves its scale at different document resolutions without pixel-grid speckle.
pub fn generate_color_grain(width: usize, height: usize, dpi: f32) -> Vec<u8> {
    let px_per_mm = dpi / 25.4;
    let mut field = vec![128; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut grain = 0.0;
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let mx = (x as f32 + sx) / px_per_mm;
                let my = (y as f32 + sy) / px_per_mm;
                grain += 0.25
                    * (0.65 * value_noise(mx / 0.085, my / 0.085, 0x9217)
                        + 0.35 * value_noise(mx / 0.29, my / 0.29, 0x6813));
            }
            field[y * width + x] =
                ((0.5 + (grain - 0.5) * 2.8).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    field
}

// Preserve the original custom paper bitmap inside a native project.
mod project_image {
    use image::RgbaImage;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    #[derive(Serialize)]
    struct ImageRef<'a> {
        width: u32,
        height: u32,
        pixels: &'a [u8],
    }
    #[derive(Deserialize)]
    struct ImageData {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    }
    pub fn serialize<S: Serializer>(image: &RgbaImage, s: S) -> Result<S::Ok, S::Error> {
        ImageRef {
            width: image.width(),
            height: image.height(),
            pixels: image.as_raw(),
        }
        .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<RgbaImage, D::Error> {
        let image = ImageData::deserialize(d)?;
        if image.width == 0
            || image.height == 0
            || u64::from(image.width) * u64::from(image.height) > 32_000_000
        {
            return Err(serde::de::Error::custom(
                "Custom paper bitmap is too large or empty",
            ));
        }
        RgbaImage::from_raw(image.width, image.height, image.pixels)
            .ok_or_else(|| serde::de::Error::custom("Invalid custom paper bitmap length"))
    }
}
