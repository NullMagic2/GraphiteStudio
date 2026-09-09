use super::{BlendMode, Cursor, RasterLayer};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Bounds {
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
}
impl Bounds {
    fn read(r: &mut Cursor<'_>) -> Result<Self, String> {
        Ok(Self {
            top: r.u32()? as i32,
            left: r.u32()? as i32,
            bottom: r.u32()? as i32,
            right: r.u32()? as i32,
        })
    }
    fn size(self) -> Result<(usize, usize), String> {
        let w = self.right as i64 - self.left as i64;
        let h = self.bottom as i64 - self.top as i64;
        if w < 0
            || h < 0
            || w.checked_mul(h)
                .and_then(|n| n.checked_mul(std::mem::size_of::<[f32; 4]>() as i64))
                .is_none_or(|bytes| bytes > isize::MAX as i64)
        {
            return Err("Invalid or overflowing PSD layer dimensions.".into());
        }
        Ok((w as usize, h as usize))
    }
}
struct Mask {
    bounds: Bounds,
    background: u8,
    flags: u8,
    density: f32,
}

#[cfg(test)]
mod bounds_tests {
    use super::Bounds;
    #[test]
    fn layer_bounds_validate_arithmetic_without_a_pixel_quota() {
        assert_eq!(
            Bounds {
                top: 0,
                left: 0,
                bottom: 8192,
                right: 8192
            }
            .size()
            .unwrap(),
            (8192, 8192)
        );
        assert!(Bounds {
            top: 1,
            left: 0,
            bottom: 0,
            right: 100
        }
        .size()
        .is_err());
        assert!(Bounds {
            top: i32::MIN,
            left: i32::MIN,
            bottom: i32::MAX,
            right: i32::MAX
        }
        .size()
        .is_err());
    }
}
struct Record {
    bounds: Bounds,
    channels: Vec<(i16, usize)>,
    name: String,
    opacity: u8,
    fill_opacity: u8,
    visible: bool,
    mode: [u8; 4],
    clipping: bool,
    group: u32,
    mask: Option<Mask>,
    effects: bool,
    vector: Option<Vec<u8>>,
    fill: Option<Vec<u8>>,
    style: Option<Vec<u8>>,
}

pub(super) fn read(
    data: &[u8],
    canvas_width: usize,
    canvas_height: usize,
    depth: u16,
    mode: u16,
    palette: &[u8],
    dpi: f32,
) -> Result<Vec<RasterLayer>, String> {
    let mut r = Cursor::new(data);
    let count = (r.u16()? as i16).unsigned_abs() as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let bounds = Bounds::read(&mut r)?;
        bounds.size()?;
        let channels = r.u16()? as usize;
        if channels > 56 {
            return Err("Too many PSD layer channels.".into());
        }
        let channels = (0..channels)
            .map(|_| Ok((r.u16()? as i16, r.u32()? as usize)))
            .collect::<Result<Vec<_>, String>>()?;
        if r.take(4)? != b"8BIM" {
            return Err("Invalid PSD layer signature.".into());
        }
        let mut blend = r.take(4)?.try_into().unwrap();
        let opacity = r.take(1)?[0];
        let clipping = r.take(1)?[0] != 0;
        let flags = r.take(1)?[0];
        r.take(1)?;
        let mut extra = Cursor::new(r.section()?);
        let mask_data = extra.section()?;
        let mut mask = if mask_data.is_empty() {
            None
        } else {
            let mut m = Cursor::new(mask_data);
            let bounds = Bounds::read(&mut m)?;
            let background = m.take(1)?[0];
            let flags = m.take(1)?[0];
            let mut density = 1.;
            if flags & 16 != 0 {
                let parameters = m.take(1)?[0];
                if parameters & 1 != 0 {
                    density = m.take(1)?[0] as f32 / 255.;
                }
                if parameters & 2 != 0 && m.take(8)?.iter().any(|&b| b != 0) {
                    return Err(
                        "PSD mask feathering is not supported without rasterizing that mask."
                            .into(),
                    );
                }
            }
            Some(Mask {
                bounds,
                background,
                flags,
                density,
            })
        };
        if let Some(m) = &mut mask {
            if m.flags & 1 != 0 {
                m.bounds.left = m
                    .bounds
                    .left
                    .checked_add(bounds.left)
                    .ok_or("Mask position overflow")?;
                m.bounds.right = m
                    .bounds
                    .right
                    .checked_add(bounds.left)
                    .ok_or("Mask position overflow")?;
                m.bounds.top = m
                    .bounds
                    .top
                    .checked_add(bounds.top)
                    .ok_or("Mask position overflow")?;
                m.bounds.bottom = m
                    .bounds
                    .bottom
                    .checked_add(bounds.top)
                    .ok_or("Mask position overflow")?;
            }
        }
        let ranges = extra.section()?;
        if !ranges.is_empty()
            && ranges
                .chunks_exact(8)
                .any(|v| v != [0, 0, 255, 255, 0, 0, 255, 255])
        {
            return Err("Custom PSD Blend If ranges are not supported.".into());
        }
        let n = extra.take(1)?[0] as usize;
        let mut name = super::mac_roman(extra.take(n)?);
        extra.take((4 - (n + 1) % 4) % 4)?;
        let mut group = 0;
        let mut effects = false;
        let mut fill_opacity = 255;
        let mut vector = None;
        let mut fill = None;
        let mut style = None;
        while extra.pos + 12 <= extra.data.len() {
            let signature = extra.take(4)?;
            if signature != b"8BIM" && signature != b"8B64" {
                return Err("Invalid PSD layer tag.".into());
            }
            let key = extra.take(4)?;
            let value = extra.section()?;
            if value.len() % 2 != 0 {
                extra.take(1)?;
            }
            match key {
                b"iOpa" => {
                    fill_opacity = *value.first().ok_or("Missing PSD fill opacity")?;
                }
                b"luni" => {
                    let mut v = Cursor::new(value);
                    let count = v.u32()? as usize;
                    if count > 32768 {
                        return Err("PSD layer name is too long.".into());
                    }
                    let units = (0..count).map(|_| v.u16()).collect::<Result<Vec<_>, _>>()?;
                    name = String::from_utf16_lossy(&units);
                }
                b"lsct" | b"lsdk" => {
                    let mut v = Cursor::new(value);
                    group = v.u32()?;
                    if value.len() >= 12 {
                        if v.take(4)? != b"8BIM" {
                            return Err("Invalid PSD group blend signature".into());
                        }
                        blend = v.take(4)?.try_into().unwrap();
                    }
                }
                b"vmsk" | b"vsms" => vector = Some(value.to_vec()),
                b"SoCo" => fill = Some(value.to_vec()),
                b"vscg" => {
                    if value.starts_with(b"SoCo") {
                        fill = Some(value[4..].to_vec());
                    } else {
                        effects = true;
                    }
                }
                b"vstk" => style = Some(value.to_vec()),
                b"GdFl" | b"PtFl" | b"SoLd" | b"SoLE" | b"PlLd" | b"plLd" => effects = true,
                b"lrFX" | b"lfx2" | b"lmfx" => effects = true,
                // Adjustment/fill layers do not have standalone raster pixels.
                b"levl" | b"curv" | b"brit" | b"blnc" | b"hue2" | b"selc" | b"grdm" | b"nvrt"
                | b"thrs" | b"post" | b"mixr" | b"blwh" | b"expA" | b"vibA" | b"clrL" => {
                    effects = true
                }
                _ => {}
            }
        }
        records.push(Record {
            bounds,
            channels,
            name,
            opacity,
            visible: flags & 2 == 0,
            mode: blend,
            clipping,
            group,
            mask,
            effects,
            fill_opacity,
            vector,
            fill,
            style,
        });
    }
    let mut result: Vec<RasterLayer> = Vec::new();
    let mut groups = Vec::new();
    let mut clipping_base = None;
    for mut record in records {
        let (w, h) = record.bounds.size()?;
        let mut planes = BTreeMap::new();
        for (id, length) in &record.channels {
            let bytes = r.take(*length)?;
            let base_channels = if mode == 4 {
                4
            } else if mode == 3 || mode == 9 {
                3
            } else {
                1
            };
            if *id >= base_channels || *id < -3 {
                continue;
            }
            let (cw, ch) = if *id == -2 {
                record
                    .mask
                    .as_ref()
                    .ok_or("PSD layer mask channel has no bounds")?
                    .bounds
                    .size()?
            } else if *id == -3 {
                return Err(format!(
                    "Layer '{}' uses a combined vector/user mask that cannot yet be imported.",
                    record.name
                ));
            } else {
                (w, h)
            };
            if cw == 0 || ch == 0 {
                continue;
            }
            let plane = super::decode_channels(bytes, cw, ch, depth, 1)?;
            if planes.insert(*id, plane).is_some() {
                return Err("Duplicate PSD layer channel.".into());
            }
        }
        if record.group == 3 {
            groups.push(result.len());
            clipping_base = None;
            continue;
        }
        if record.group == 1 || record.group == 2 {
            let start = groups.pop().ok_or("Unbalanced PSD groups")?;
            if record.mode != *b"pass"
                || record.opacity != 255
                || record.mask.is_some()
                || record.vector.is_some()
                || record.effects
            {
                return Err(format!("Group '{}' uses isolated blending, opacity, masks or effects. Import currently preserves pass-through groups at full opacity.",record.name));
            }
            for layer in &mut result[start..] {
                layer.visible &= record.visible;
                layer.name = format!("{} / {}", record.name, layer.name);
            }
            clipping_base = None;
            continue;
        }
        if record.effects {
            return Err(format!("Layer '{}' uses Photoshop effects, adjustments, a patterned/gradient fill or a smart object that cannot be preserved yet.",record.name));
        }
        let blend = match &record.mode {
            b"norm" => BlendMode::Normal,
            b"mul " => BlendMode::Multiply,
            b"dark" => BlendMode::Darken,
            b"scrn" => BlendMode::Screen,
            b"lite" => BlendMode::Lighten,
            b"sat " => BlendMode::Saturation,
            _ => {
                return Err(format!(
                    "Layer '{}' uses unsupported blend mode '{}'.",
                    record.name,
                    String::from_utf8_lossy(&record.mode)
                ))
            }
        };
        if record.vector.is_some() || record.fill.is_some() {
            if record.mask.is_some() || record.clipping || record.effects {
                return Err(format!("Vector layer '{}' has bitmap masks, clipping, effects or a smart object that cannot be preserved yet.",record.name));
            }
            let mut shape = super::vectors::shape(
                record.vector.as_deref(),
                record.fill.as_deref(),
                record.style.as_deref(),
                canvas_width,
                canvas_height,
                dpi,
            )
            .map_err(|e| format!("Vector layer '{}': {e}", record.name))?;
            if let Some(fill) = &mut shape.fill {
                fill[3] *= record.fill_opacity as f32 / 255.;
            }
            if let Some(stroke) = &mut shape.stroke {
                stroke.color[3] *= record.fill_opacity as f32 / 255.;
            }
            clipping_base = Some(result.len());
            let mut layer = RasterLayer {
                name: record.name,
                width: 0,
                height: 0,
                left: 0,
                top: 0,
                pixels: vec![].into(),
                shape: None,
                visible: record.visible,
                opacity: record.opacity,
                blend,
            };
            layer.set_shape(shape, canvas_width, canvas_height)?;
            result.push(layer);
            continue;
        }
        let base = match mode {
            0 | 1 | 2 | 8 => 1,
            3 | 9 => 3,
            4 => 4,
            _ => return Err("Unsupported PSD layer color mode.".into()),
        };
        if record.clipping {
            let base: &RasterLayer = result
                .get(clipping_base.ok_or("PSD clipping layer has no base")?)
                .ok_or("Invalid clipping base")?;
            if base.opacity != 255 || base.pixels.iter().any(|p| p[3] > 0. && p[3] < 1.) {
                return Err(format!("Clipping layer '{}' has a translucent base. Its clipping group cannot yet be preserved without flattening.",record.name));
            }
            record.visible &= base.visible;
        }
        if w * h > 0 && (0..base).any(|c| !planes.contains_key(&(c as i16))) {
            return Err(format!("Layer '{}' has no complete raster pixels; it must be rasterized in its source app before import.",record.name));
        }
        let mut pixels = Vec::with_capacity(w * h);
        for i in 0..w * h {
            let sample = |c| {
                sample(
                    planes.get(&c).map(Vec::as_slice).unwrap_or(&[]),
                    w,
                    depth,
                    i,
                )
            };
            let color = match mode {
                0 | 1 | 8 => [sample(0)?; 3],
                2 => {
                    let index = (sample(0)? * 255.).round() as usize;
                    [
                        palette[index] as f32 / 255.,
                        palette[256 + index] as f32 / 255.,
                        palette[512 + index] as f32 / 255.,
                    ]
                }
                3 => [sample(0)?, sample(1)?, sample(2)?],
                4 => [
                    sample(0)? * sample(3)?,
                    sample(1)? * sample(3)?,
                    sample(2)? * sample(3)?,
                ],
                9 => super::lab_to_rgb(
                    sample(0)? * 100.,
                    sample(1)? * 255. - 128.,
                    sample(2)? * 255. - 128.,
                ),
                _ => unreachable!(),
            };
            let mut alpha = if planes.contains_key(&-1) {
                sample(-1)?
            } else {
                1.
            };
            alpha *= record.fill_opacity as f32 / 255.;
            if let Some(mask) = &record.mask {
                if mask.flags & 2 == 0 {
                    let (mw, mh) = mask.bounds.size()?;
                    let x = record.bounds.left as i64 + (i % w) as i64 - mask.bounds.left as i64;
                    let y = record.bounds.top as i64 + (i / w) as i64 - mask.bounds.top as i64;
                    let mut coverage = if x < 0 || y < 0 || x >= mw as i64 || y >= mh as i64 {
                        mask.background as f32 / 255.
                    } else {
                        self::sample(
                            planes.get(&-2).ok_or("PSD mask pixels are missing")?,
                            mw,
                            depth,
                            y as usize * mw + x as usize,
                        )?
                    };
                    if mask.flags & 4 != 0 {
                        coverage = 1. - coverage;
                    }
                    alpha *= 1. - mask.density * (1. - coverage);
                }
            }
            if record.clipping {
                let base: &RasterLayer = result
                    .get(clipping_base.ok_or("PSD clipping layer has no base")?)
                    .ok_or("Invalid clipping base")?;
                let x = record.bounds.left as i64 + (i % w) as i64 - base.left as i64;
                let y = record.bounds.top as i64 + (i / w) as i64 - base.top as i64;
                alpha *= if x < 0 || y < 0 || x >= base.width as i64 || y >= base.height as i64 {
                    0.
                } else {
                    base.pixels.get(y as usize * base.width + x as usize)[3]
                };
            }
            pixels.push([color[0], color[1], color[2], alpha]);
        }
        if !record.clipping {
            clipping_base = Some(result.len());
        }
        result.push(RasterLayer {
            name: record.name,
            width: w,
            height: h,
            left: record.bounds.left,
            top: record.bounds.top,
            shape: None,
            pixels: pixels.into(),
            visible: record.visible,
            opacity: record.opacity,
            blend,
        });
    }
    if !groups.is_empty() {
        return Err("Unbalanced PSD groups.".into());
    }
    if result.is_empty() {
        result.push(RasterLayer {
            name: "Empty layer".into(),
            width: 0,
            height: 0,
            left: 0,
            top: 0,
            shape: None,
            pixels: vec![].into(),
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
        });
    }
    Ok(result)
}

fn sample(data: &[u8], width: usize, depth: u16, i: usize) -> Result<f32, String> {
    let stride = (width * depth as usize).div_ceil(8);
    let offset = i / width * stride;
    let x = i % width;
    let value = match depth {
        1 => {
            if data[offset + x / 8] & (0x80 >> (x % 8)) == 0 {
                1.
            } else {
                0.
            }
        }
        8 => data[offset + x] as f32 / 255.,
        16 => {
            u16::from_be_bytes(data[offset + x * 2..offset + x * 2 + 2].try_into().unwrap()) as f32
                / 65535.
        }
        32 => f32::from_be_bytes(data[offset + x * 4..offset + x * 4 + 4].try_into().unwrap()),
        _ => return Err("Unsupported PSD sample depth.".into()),
    };
    if !value.is_finite() {
        return Err("PSD contains non-finite samples.".into());
    }
    Ok(value.clamp(0., 1.))
}
