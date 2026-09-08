//! PSD version 1 layer and merged-image reader. Native Graphite PSDs use project::load first.
//! Layout: https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/
//! Supports raw, PackBits, ZIP and ZIP-prediction image data.
use super::{RasterDocument, RasterLayer};
use crate::core::document::BlendMode;
use std::io::Read;
mod layers;
mod vectors;

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("Invalid PSD section size")?;
        let out = self.data.get(self.pos..end).ok_or("Truncated PSD data")?;
        self.pos = end;
        Ok(out)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn section(&mut self) -> Result<&'a [u8], String> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

pub(super) fn decode(data: &[u8]) -> Result<RasterDocument, String> {
    let mut r = Cursor::new(data);
    if r.take(4)? != b"8BPS" || r.u16()? != 1 {
        return Err("Not a PSD version 1 document.".into());
    }
    if r.take(6)?.iter().any(|&v| v != 0) {
        return Err("Invalid PSD header.".into());
    }
    let channels = r.u16()? as usize;
    let height = r.u32()? as usize;
    let width = r.u32()? as usize;
    super::check_dimensions(width, height)?;
    if !(1..=56).contains(&channels) || width > 30000 || height > 30000 {
        return Err("Invalid PSD dimensions or channels.".into());
    }
    let depth = r.u16()?;
    let mode = r.u16()?;
    let base = match mode {
        0 | 1 | 2 | 8 => 1,
        3 | 9 => 3,
        4 => 4,
        _ => {
            return Err(
                "PSD multichannel color is not supported; convert it to RGB in Photoshop.".into(),
            )
        }
    };
    if channels < base
        || !matches!(depth, 1 | 8 | 16 | 32)
        || (depth == 1 && mode != 0)
        || (mode == 0 && depth != 1)
        || (mode == 2 && depth != 8)
    {
        return Err("Unsupported PSD color mode and bit depth combination.".into());
    }
    let palette = r.section()?;
    if mode == 2 && palette.len() != 768 {
        return Err("Invalid PSD indexed palette.".into());
    }
    let resources = r.section()?;
    let mut dpi = 300.;
    let mut transparent_index = None;
    let mut saved_paths = Vec::new();
    let mut resources = Cursor::new(resources);
    while resources.pos < resources.data.len() {
        if resources.take(4)? != b"8BIM" {
            return Err("Invalid PSD resource signature.".into());
        }
        let id = resources.u16()?;
        let name = resources.take(1)?[0] as usize;
        let name_bytes = resources.take(name)?;
        if (name + 1) % 2 != 0 {
            resources.take(1)?;
        }
        let name = mac_roman(name_bytes);
        let payload = resources.section()?;
        if payload.len() % 2 != 0 {
            resources.take(1)?;
        }
        if id == 1005 && payload.len() >= 16 {
            let value = u32::from_be_bytes(payload[..4].try_into().unwrap()) as f32 / 65536.;
            dpi = value
                * if u16::from_be_bytes(payload[4..6].try_into().unwrap()) == 2 {
                    2.54
                } else {
                    1.
                };
        }
        if id == 1047 && payload.len() == 2 {
            transparent_index = Some(u16::from_be_bytes(payload.try_into().unwrap()) as usize);
        }
        if (2000..=2997).contains(&id) || id == 1025 {
            let mut shape = vectors::paths(payload, width, height)?;
            shape.stroke = Some(crate::core::vector::shape::ShapeStroke {
                color: [0., 0., 0., 1.],
                width: 1.,
                cap: 1,
                join: 1,
                miter: 1.,
            });
            if !shape.valid() {
                return Err("Invalid saved Photoshop path.".into());
            }
            if !shape.paths.is_empty() {
                let mut layer = RasterLayer {
                    name: format!(
                        "Path / {}",
                        if name.is_empty() { "Work path" } else { &name }
                    ),
                    width: 0,
                    height: 0,
                    left: 0,
                    top: 0,
                    pixels: vec![].into(),
                    shape: None,
                    visible: false,
                    opacity: 255,
                    blend: BlendMode::Normal,
                };
                layer.set_shape(shape, width, height)?;
                saved_paths.push(layer);
            }
        }
    }
    let layer_mask = r.section()?;
    // A negative layer count marks the first extra composite channel as transparency.
    // Additional alpha/spot selection channels must not become image transparency.
    let mut merged_alpha = false;
    let mut layer_info = &[][..];
    if layer_mask.len() >= 6 {
        let mut layers = Cursor::new(layer_mask);
        let info = layers.section()?;
        layer_info = info;
        if info.len() >= 2 {
            merged_alpha = i16::from_be_bytes(info[..2].try_into().unwrap()) < 0;
        }
        if layers.pos + 4 <= layer_mask.len() {
            layers.section()?; // Global layer mask.
            while layers.pos + 12 <= layer_mask.len() {
                let signature = layers.take(4)?;
                if signature != b"8BIM" && signature != b"8B64" {
                    break;
                }
                let key = layers.take(4)?;
                let extra = layers.section()?;
                if matches!(key, b"Lr16" | b"Lr32") && extra.len() >= 2 {
                    layer_info = extra;
                    merged_alpha |= i16::from_be_bytes(extra[..2].try_into().unwrap()) < 0;
                }
                let padding = (4 - extra.len() % 4) % 4;
                layers.take(padding)?;
            }
        }
    }
    if layer_info.len() >= 2 && i16::from_be_bytes(layer_info[..2].try_into().unwrap()) != 0 {
        let mut layers = layers::read(layer_info, width, height, depth, mode, palette, dpi)?;
        layers.extend(saved_paths);
        return Ok(RasterDocument {
            width,
            height,
            dpi,
            layers,
        });
    }
    let raw = decode_channels(&r.data[r.pos..], width, height, depth, channels)?;
    let row_bytes = (width * depth as usize).div_ceil(8);
    let plane = row_bytes * height;
    let sample = |c: usize, i: usize| -> f32 {
        let offset = c * plane + (i / width) * row_bytes;
        let x = i % width;
        match depth {
            1 => {
                if raw[offset + x / 8] & (0x80 >> (x % 8)) == 0 {
                    1.
                } else {
                    0.
                }
            }
            8 => raw[offset + x] as f32 / 255.,
            16 => {
                u16::from_be_bytes(raw[offset + x * 2..offset + x * 2 + 2].try_into().unwrap())
                    as f32
                    / 65535.
            }
            32 => f32::from_be_bytes(raw[offset + x * 4..offset + x * 4 + 4].try_into().unwrap()),
            _ => unreachable!(),
        }
    };
    let mut pixels = Vec::with_capacity(width * height);
    for i in 0..width * height {
        let mut alpha = if merged_alpha && channels > base {
            sample(base, i)
        } else {
            1.
        };
        let rgb = match mode {
            0 | 1 | 8 => [sample(0, i); 3],
            2 => {
                let index = (sample(0, i) * 255.).round() as usize;
                if transparent_index == Some(index) {
                    alpha = 0.;
                }
                [
                    palette[index] as f32 / 255.,
                    palette[256 + index] as f32 / 255.,
                    palette[512 + index] as f32 / 255.,
                ]
            }
            3 => [sample(0, i), sample(1, i), sample(2, i)],
            // PSD stores inverted CMYK components. This is a device-independent
            // fallback, not an ICC color-managed press proof.
            4 => [
                sample(0, i) * sample(3, i),
                sample(1, i) * sample(3, i),
                sample(2, i) * sample(3, i),
            ],
            9 => lab_to_rgb(
                sample(0, i) * 100.,
                sample(1, i) * 255. - 128.,
                sample(2, i) * 255. - 128.,
            ),
            _ => unreachable!(),
        };
        if !alpha.is_finite() || rgb.iter().any(|v| !v.is_finite()) {
            return Err("PSD contains non-finite samples.".into());
        }
        pixels.push([
            rgb[0].clamp(0., 1.),
            rgb[1].clamp(0., 1.),
            rgb[2].clamp(0., 1.),
            alpha.clamp(0., 1.),
        ]);
    }
    let mut layers = vec![RasterLayer {
        name: "Background".into(),
        width,
        height,
        left: 0,
        top: 0,
        shape: None,
        pixels: pixels.into(),
        visible: true,
        opacity: 255,
        blend: BlendMode::Normal,
    }];
    layers.extend(saved_paths);
    Ok(RasterDocument {
        width,
        height,
        dpi,
        layers,
    })
}

fn lab_to_rgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let fy = (l + 16.) / 116.;
    let f = |v: f32| {
        if v > 6. / 29. {
            v * v * v
        } else {
            3. * (6_f32 / 29.).powi(2) * (v - 4. / 29.)
        }
    };
    let (x, y, z) = (
        f(fy + a / 500.) * 0.96422,
        f(fy),
        f(fy - b / 200.) * 0.82521,
    );
    // D50 Lab to D65 XYZ (Bradford adaptation), then sRGB.
    let (x, y, z) = (
        0.9555766 * x - 0.0230393 * y + 0.0631636 * z,
        -0.0282895 * x + 1.0099416 * y + 0.0210077 * z,
        0.0122982 * x - 0.020483 * y + 1.3299098 * z,
    );
    [
        3.2404542 * x - 1.5371385 * y - 0.4985314 * z,
        -0.969266 * x + 1.8760108 * y + 0.041556 * z,
        0.0556434 * x - 0.2040259 * y + 1.0572252 * z,
    ]
    .map(|v| {
        if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        }
    })
}

fn decode_channels(
    data: &[u8],
    width: usize,
    height: usize,
    depth: u16,
    channels: usize,
) -> Result<Vec<u8>, String> {
    let mut r = Cursor::new(data);
    let compression = r.u16()?;
    let row_bytes = (width * depth as usize).div_ceil(8);
    let expected = row_bytes
        .checked_mul(height)
        .and_then(|v| v.checked_mul(channels))
        .ok_or("PSD data is too large")?;
    let mut raw = match compression {
        0 => r.take(expected)?.to_vec(),
        1 => {
            let lengths = (0..height * channels)
                .map(|_| r.u16().map(usize::from))
                .collect::<Result<Vec<_>, _>>()?;
            let mut output = Vec::with_capacity(expected);
            for length in lengths {
                let mut row = Cursor::new(r.take(length)?);
                let start = output.len();
                while row.pos < row.data.len() {
                    let code = row.take(1)?[0] as i8;
                    let count = if code >= 0 {
                        code as usize + 1
                    } else if code != -128 {
                        (1 - code as i16) as usize
                    } else {
                        0
                    };
                    if output.len() - start + count > row_bytes {
                        return Err("PSD RLE row exceeds its width.".into());
                    }
                    if code >= 0 {
                        output.extend_from_slice(row.take(count)?);
                    } else if code != -128 {
                        let value = row.take(1)?[0];
                        output.resize(output.len() + count, value);
                    }
                }
                if output.len() - start != row_bytes {
                    return Err("Incomplete PSD RLE row.".into());
                }
            }
            output
        }
        2 | 3 => {
            let mut output = Vec::new();
            flate2::read::ZlibDecoder::new(&r.data[r.pos..])
                .take(expected as u64 + 1)
                .read_to_end(&mut output)
                .map_err(|e| format!("Invalid PSD ZIP image: {e}"))?;
            if output.len() != expected {
                return Err("PSD ZIP image size does not match its header.".into());
            }
            output
        }
        _ => return Err("Unsupported PSD image compression.".into()),
    };
    if compression == 3 {
        for row in raw.chunks_exact_mut(row_bytes) {
            match depth {
                8 => {
                    for x in 1..row.len() {
                        row[x] = row[x].wrapping_add(row[x - 1]);
                    }
                }
                16 => {
                    for x in 1..width {
                        let previous =
                            u16::from_be_bytes(row[(x - 1) * 2..x * 2].try_into().unwrap());
                        let delta = u16::from_be_bytes(row[x * 2..x * 2 + 2].try_into().unwrap());
                        row[x * 2..x * 2 + 2]
                            .copy_from_slice(&delta.wrapping_add(previous).to_be_bytes());
                    }
                }
                32 => {
                    for x in 1..row.len() {
                        row[x] = row[x].wrapping_add(row[x - 1]);
                    }
                    let shuffled = row.to_vec();
                    for x in 0..width {
                        for b in 0..4 {
                            row[x * 4 + b] = shuffled[b * width + x];
                        }
                    }
                }
                _ => return Err("PSD bitmap prediction is not supported.".into()),
            }
        }
    }
    Ok(raw)
}

fn mac_roman(data: &[u8]) -> String {
    const HIGH: [char; 128] = [
        '\u{c4}', '\u{c5}', '\u{c7}', '\u{c9}', '\u{d1}', '\u{d6}', '\u{dc}', '\u{e1}', '\u{e0}',
        '\u{e2}', '\u{e4}', '\u{e3}', '\u{e5}', '\u{e7}', '\u{e9}', '\u{e8}', '\u{ea}', '\u{eb}',
        '\u{ed}', '\u{ec}', '\u{ee}', '\u{ef}', '\u{f1}', '\u{f3}', '\u{f2}', '\u{f4}', '\u{f6}',
        '\u{f5}', '\u{fa}', '\u{f9}', '\u{fb}', '\u{fc}', '\u{2020}', '\u{b0}', '\u{a2}', '\u{a3}',
        '\u{a7}', '\u{2022}', '\u{b6}', '\u{df}', '\u{ae}', '\u{a9}', '\u{2122}', '\u{b4}',
        '\u{a8}', '\u{2260}', '\u{c6}', '\u{d8}', '\u{221e}', '\u{b1}', '\u{2264}', '\u{2265}',
        '\u{a5}', '\u{b5}', '\u{2202}', '\u{2211}', '\u{220f}', '\u{3c0}', '\u{222b}', '\u{aa}',
        '\u{ba}', '\u{3a9}', '\u{e6}', '\u{f8}', '\u{bf}', '\u{a1}', '\u{ac}', '\u{221a}',
        '\u{192}', '\u{2248}', '\u{2206}', '\u{ab}', '\u{bb}', '\u{2026}', '\u{a0}', '\u{c0}',
        '\u{c3}', '\u{d5}', '\u{152}', '\u{153}', '\u{2013}', '\u{2014}', '\u{201c}', '\u{201d}',
        '\u{2018}', '\u{2019}', '\u{f7}', '\u{25ca}', '\u{ff}', '\u{178}', '\u{2044}', '\u{20ac}',
        '\u{2039}', '\u{203a}', '\u{fb01}', '\u{fb02}', '\u{2021}', '\u{b7}', '\u{201a}',
        '\u{201e}', '\u{2030}', '\u{c2}', '\u{ca}', '\u{c1}', '\u{cb}', '\u{c8}', '\u{cd}',
        '\u{ce}', '\u{cf}', '\u{cc}', '\u{d3}', '\u{d4}', '\u{f8ff}', '\u{d2}', '\u{da}', '\u{db}',
        '\u{d9}', '\u{131}', '\u{2c6}', '\u{2dc}', '\u{af}', '\u{2d8}', '\u{2d9}', '\u{2da}',
        '\u{b8}', '\u{2dd}', '\u{2db}', '\u{2c7}',
    ];
    data.iter()
        .map(|&b| {
            if b < 128 {
                b as char
            } else {
                HIGH[(b - 128) as usize]
            }
        })
        .collect()
}
