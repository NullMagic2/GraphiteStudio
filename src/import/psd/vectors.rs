use super::Cursor;
use crate::core::vector::shape::{Knot, Shape, ShapeStroke, Subpath};
use eframe::egui::Vec2;
use std::collections::BTreeMap;

pub(super) fn paths(data: &[u8], w: usize, h: usize) -> Result<Shape, String> {
    let mut r = Cursor::new(data);
    let mut paths = Vec::new();
    let mut initial_fill = false;
    let mut knots_total = 0;
    while r.pos + 26 <= data.len() {
        let kind = r.u16()?;
        let mut body = Cursor::new(r.take(24)?);
        match kind {
            0 | 3 => {
                let count = body.u16()? as usize;
                let operation = body.u16()?;
                if operation > 3 {
                    return Err("Unsupported Photoshop path operation.".into());
                }
                knots_total += count;
                if paths.len() >= 4096 || knots_total > 100_000 {
                    return Err("Photoshop path is too complex.".into());
                }
                let mut knots = Vec::with_capacity(count);
                for _ in 0..count {
                    let k = r.u16()?;
                    if (kind == 0 && !matches!(k, 1 | 2)) || (kind == 3 && !matches!(k, 4 | 5)) {
                        return Err("Invalid Photoshop Bézier knot.".into());
                    }
                    let mut coordinate = || -> Result<Vec2, String> {
                        let y = r.u32()? as i32 as f32 / 16_777_216. * h as f32;
                        let x = r.u32()? as i32 as f32 / 16_777_216. * w as f32;
                        Ok(Vec2::new(x, y))
                    };
                    knots.push(Knot {
                        before: coordinate()?,
                        anchor: coordinate()?,
                        after: coordinate()?,
                    });
                }
                paths.push(Subpath {
                    knots,
                    closed: kind == 0,
                    operation,
                });
            }
            6 | 7 => {}
            8 => {
                let rule = body.u16()?;
                if rule > 1 {
                    return Err("Invalid Photoshop path fill rule.".into());
                }
                initial_fill = rule == 1;
            }
            _ => return Err("Invalid Photoshop path record.".into()),
        }
    }
    if data.len() - r.pos > 3 || data[r.pos..].iter().any(|&v| v != 0) {
        return Err("Truncated Photoshop path.".into());
    }
    Ok(Shape {
        paths,
        initial_fill,
        inverted: false,
        fill: None,
        stroke: None,
    })
}

pub(super) fn shape(
    mask: Option<&[u8]>,
    fill: Option<&[u8]>,
    style: Option<&[u8]>,
    w: usize,
    h: usize,
    dpi: f32,
) -> Result<Shape, String> {
    let mut shape = if let Some(mask) = mask {
        let mut r = Cursor::new(mask);
        if r.u32()? != 3 {
            return Err("Unsupported Photoshop vector mask version.".into());
        }
        let flags = r.u32()?;
        if flags & 4 != 0 {
            return Err("Disabled Photoshop vector masks are not supported yet.".into());
        }
        let mut shape = paths(&mask[r.pos..], w, h)?;
        shape.inverted = flags & 1 != 0;
        shape
    } else {
        let knots = [
            Vec2::ZERO,
            Vec2::new(w as f32, 0.),
            Vec2::new(w as f32, h as f32),
            Vec2::new(0., h as f32),
        ]
        .map(|p| Knot {
            before: p,
            anchor: p,
            after: p,
        })
        .to_vec();
        Shape {
            paths: vec![Subpath {
                knots,
                closed: true,
                operation: 1,
            }],
            fill: None,
            stroke: None,
            initial_fill: false,
            inverted: false,
        }
    };
    if let Some(fill) = fill {
        shape.fill = Some(color(&descriptor_block(fill)?)?);
    }
    if let Some(style) = style {
        let d = descriptor_block(style)?;
        if !boolean(&d, "fillEnabled", true) {
            shape.fill = None;
        }
        if boolean(&d, "strokeEnabled", false) {
            let width = unit(&d, "strokeStyleLineWidth", dpi, 1.)?;
            let alignment =
                enumeration(&d, "strokeStyleLineAlignment").unwrap_or(b"strokeStyleAlignCenter");
            if alignment != b"strokeStyleAlignCenter" {
                return Err("Inside/outside Photoshop strokes are not supported yet; the vector is not rasterized.".into());
            }
            if matches!(d.get(b"strokeStyleLineDashSet".as_slice()),Some(Value::List(v)) if !v.is_empty())
            {
                return Err("Dashed Photoshop vector strokes are not supported yet.".into());
            }
            if enumeration(&d, "strokeStyleBlendMode")
                .is_some_and(|v| v != b"Nrml" && v != b"normal")
            {
                return Err("Photoshop stroke blend mode is not supported.".into());
            }
            let content =
                object(&d, b"strokeStyleContent").ok_or("Photoshop stroke color is missing")?;
            let mut c = color(content)?;
            c[3] *= number(&d, "strokeStyleOpacity", 100.) / 100.;
            let cap = match enumeration(&d, "strokeStyleLineCapType") {
                Some(b"strokeStyleRoundCap") => 1,
                Some(b"strokeStyleSquareCap") => 2,
                _ => 0,
            };
            let join = match enumeration(&d, "strokeStyleLineJoinType") {
                Some(b"strokeStyleRoundJoin") => 1,
                Some(b"strokeStyleBevelJoin") => 2,
                _ => 0,
            };
            shape.stroke = Some(ShapeStroke {
                color: c,
                width,
                cap,
                join,
                miter: number(&d, "strokeStyleMiterLimit", 4.),
            });
        }
    }
    if shape.fill.is_none() && shape.stroke.is_none() {
        return Err("This Photoshop vector has no supported solid fill or stroke.".into());
    }
    if !shape.valid() {
        return Err("Invalid Photoshop vector geometry or style.".into());
    }
    Ok(shape)
}

#[derive(Debug)]
enum Value {
    Object(BTreeMap<Vec<u8>, Value>),
    Number(f64),
    Unit(Vec<u8>, f64),
    Bool(bool),
    Enum(Vec<u8>),
    List(Vec<Value>),
    Other,
}
type Descriptor = BTreeMap<Vec<u8>, Value>;
fn identifier(r: &mut Cursor<'_>) -> Result<Vec<u8>, String> {
    let length = r.u32()? as usize;
    r.take(if length == 0 { 4 } else { length })
        .map(|b| b.to_vec())
}
fn unicode(r: &mut Cursor<'_>) -> Result<(), String> {
    let n = r.u32()? as usize;
    if n > 1_000_000 {
        return Err("PSD descriptor text is too large.".into());
    }
    r.take(n * 2)?;
    Ok(())
}
fn descriptor_block(data: &[u8]) -> Result<Descriptor, String> {
    let mut r = Cursor::new(data);
    if r.u32()? != 16 {
        return Err("Unsupported Photoshop descriptor version.".into());
    }
    descriptor(&mut r, 0, &mut 0)
}
fn descriptor(r: &mut Cursor<'_>, depth: usize, budget: &mut usize) -> Result<Descriptor, String> {
    if depth > 24 {
        return Err("PSD descriptor nesting is too deep.".into());
    }
    unicode(r)?;
    identifier(r)?;
    let count = r.u32()? as usize;
    if count > 10000 {
        return Err("PSD descriptor is too large.".into());
    }
    let mut out = BTreeMap::new();
    for _ in 0..count {
        let key = identifier(r)?;
        let value = value(r, depth + 1, budget)?;
        out.insert(key, value);
    }
    Ok(out)
}
fn value(r: &mut Cursor<'_>, depth: usize, budget: &mut usize) -> Result<Value, String> {
    *budget += 1;
    if *budget > 20000 || depth > 24 {
        return Err("PSD descriptor is too complex.".into());
    }
    Ok(match r.take(4)? {
        b"Objc" | b"GlbO" => Value::Object(descriptor(r, depth, budget)?),
        b"doub" => Value::Number(f64::from_be_bytes(r.take(8)?.try_into().unwrap())),
        b"UntF" => {
            let unit = r.take(4)?.to_vec();
            Value::Unit(unit, f64::from_be_bytes(r.take(8)?.try_into().unwrap()))
        }
        b"long" => Value::Number(r.u32()? as i32 as f64),
        b"bool" => Value::Bool(r.take(1)?[0] != 0),
        b"enum" => {
            identifier(r)?;
            Value::Enum(identifier(r)?)
        }
        b"VlLs" => {
            let count = r.u32()? as usize;
            if count > 10000 {
                return Err("PSD descriptor list is too large.".into());
            }
            Value::List(
                (0..count)
                    .map(|_| value(r, depth + 1, budget))
                    .collect::<Result<_, _>>()?,
            )
        }
        b"TEXT" => {
            unicode(r)?;
            Value::Other
        }
        b"tdta" | b"alis" => {
            r.section()?;
            Value::Other
        }
        other => {
            return Err(format!(
                "Unsupported Photoshop shape descriptor '{}'.",
                String::from_utf8_lossy(other)
            ))
        }
    })
}
fn object<'a>(d: &'a Descriptor, key: &[u8]) -> Option<&'a Descriptor> {
    if let Some(Value::Object(o)) = d.get(key) {
        Some(o)
    } else {
        None
    }
}
fn number(d: &Descriptor, key: &str, default: f32) -> f32 {
    match d.get(key.as_bytes()) {
        Some(Value::Number(v) | Value::Unit(_, v)) => *v as f32,
        _ => default,
    }
}
fn boolean(d: &Descriptor, key: &str, default: bool) -> bool {
    match d.get(key.as_bytes()) {
        Some(Value::Bool(v)) => *v,
        _ => default,
    }
}
fn enumeration<'a>(d: &'a Descriptor, key: &str) -> Option<&'a [u8]> {
    match d.get(key.as_bytes()) {
        Some(Value::Enum(v)) => Some(v),
        _ => None,
    }
}
fn unit(d: &Descriptor, key: &str, dpi: f32, default: f32) -> Result<f32, String> {
    Ok(match d.get(key.as_bytes()) {
        Some(Value::Unit(u, v)) => {
            *v as f32
                * match u.as_slice() {
                    b"#Pxl" => 1.,
                    b"#Pnt" => dpi / 72.,
                    b"#In " => dpi,
                    b"#Mlm" => dpi / 25.4,
                    b"#Cmt" => dpi / 2.54,
                    _ => return Err("Unsupported Photoshop vector measurement unit.".into()),
                }
        }
        _ => number(d, key, default),
    })
}
fn color(d: &Descriptor) -> Result<[f32; 4], String> {
    let c = object(d, b"Clr ")
        .ok_or("Only solid-color Photoshop vector fills are currently supported")?;
    let rgb = if c.contains_key(b"Rd  ".as_slice()) {
        [
            number(c, "Rd  ", 0.) / 255.,
            number(c, "Grn ", 0.) / 255.,
            number(c, "Bl  ", 0.) / 255.,
        ]
    } else if c.contains_key(b"Gry ".as_slice()) {
        [number(c, "Gry ", 0.) / 100.; 3]
    } else if c.contains_key(b"Cyn ".as_slice()) {
        let k = 1. - number(c, "Blck", 0.) / 100.;
        [
            (1. - number(c, "Cyn ", 0.) / 100.) * k,
            (1. - number(c, "Mgnt", 0.) / 100.) * k,
            (1. - number(c, "Ylw ", 0.) / 100.) * k,
        ]
    } else {
        return Err("Unsupported Photoshop vector color model.".into());
    };
    if rgb.iter().any(|v| !v.is_finite()) {
        return Err("Invalid Photoshop vector color.".into());
    }
    Ok([
        rgb[0].clamp(0., 1.),
        rgb[1].clamp(0., 1.),
        rgb[2].clamp(0., 1.),
        1.,
    ])
}
