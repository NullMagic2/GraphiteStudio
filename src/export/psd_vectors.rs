//! Photoshop-native Bezier paths and vector fill masks, per Adobe's PSD spec.
use crate::core::{document::Document, vector::VectorStroke};
use eframe::egui::Vec2;
use std::io::{self, Write};
#[derive(Clone, Copy)]
struct Knot {
    before: Vec2,
    point: Vec2,
    after: Vec2,
}
fn v(p: crate::core::stroke::StrokePoint) -> Vec2 {
    Vec2::new(p.x, p.y)
}
fn centerline(s: &VectorStroke) -> (Vec<Knot>, bool) {
    let Some(&first) = s.points.first() else {
        return (vec![], false);
    };
    if s.polyline {
        let closed =
            s.points.len() > 2 && (v(first) - v(*s.points.last().unwrap())).length() < 0.001;
        let count = s.points.len() - usize::from(closed);
        return (
            s.points[..count]
                .iter()
                .map(|&p| Knot {
                    before: v(p),
                    point: v(p),
                    after: v(p),
                })
                .collect(),
            closed,
        );
    }
    let mut knots = vec![Knot {
        before: v(first),
        point: v(first),
        after: v(first),
    }];
    let (mut raw, mut cursor) = (v(first), v(first));
    for point in s
        .points
        .iter()
        .skip(1)
        .map(|&p| v(p))
        .chain(std::iter::once(v(*s.points.last().unwrap())))
    {
        let end = (raw + point) * 0.5;
        let c1 = cursor + (raw - cursor) * (2. / 3.);
        let c2 = end + (raw - end) * (2. / 3.);
        knots.last_mut().unwrap().after = c1;
        knots.push(Knot {
            before: c2,
            point: end,
            after: end,
        });
        cursor = end;
        raw = point;
    }
    (knots, false)
}
fn record(out: &mut Vec<u8>, kind: u16, body: &[u8]) {
    out.extend(kind.to_be_bytes());
    out.extend(body);
    out.resize(out.len() + 24 - body.len(), 0);
}
fn append(out: &mut Vec<u8>, knots: &[Knot], closed: bool, w: f32, h: f32) {
    // The on-disk knot count is u16; long open paths continue as adjacent subpaths.
    for chunk in knots.chunks(65535) {
        if chunk.is_empty() {
            continue;
        }
        record(
            out,
            if closed { 0 } else { 3 },
            &(chunk.len() as u16).to_be_bytes(),
        );
        for knot in chunk {
            let mut data = Vec::with_capacity(24);
            for p in [knot.before, knot.point, knot.after] {
                for coordinate in [p.y / h, p.x / w] {
                    data.extend(
                        ((coordinate.clamp(-16., 15.999999) * 16_777_216.).round() as i32)
                            .to_be_bytes(),
                    );
                }
            }
            record(out, if closed { 2 } else { 5 }, &data);
        }
    }
}
fn blank() -> Vec<u8> {
    let mut b = Vec::new();
    record(&mut b, 6, &[]);
    b
}
pub fn resources(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    let total = doc
        .layers
        .iter()
        .map(|l| l.vectors.strokes.len())
        .sum::<usize>();
    let mut id = 2000;
    for layer in &doc.layers {
        let mut combined = blank();
        for (i, stroke) in layer.vectors.strokes.iter().enumerate() {
            let (mut data, closed) = {
                let (k, c) = centerline(stroke);
                (k, c)
            };
            if data.is_empty() {
                continue;
            }
            if total <= 998 {
                let mut path = blank();
                append(
                    &mut path,
                    &data,
                    closed,
                    doc.spec.width_px as f32,
                    doc.spec.height_px as f32,
                );
                resource(
                    &mut out,
                    id,
                    &format!("Layer {} - {} {}", layer.id, i + 1, stroke.label),
                    &path,
                );
                id += 1;
            } else {
                append(
                    &mut combined,
                    &data,
                    closed,
                    doc.spec.width_px as f32,
                    doc.spec.height_px as f32,
                );
            }
            data.clear();
        }
        if total > 998 && combined.len() > 26 && id < 2998 {
            resource(
                &mut out,
                id,
                &format!("Layer {} - paths", layer.id),
                &combined,
            );
            id += 1;
        }
    }
    out
}
pub fn resource(out: &mut Vec<u8>, id: u16, name: &str, data: &[u8]) {
    out.extend(b"8BIM");
    out.extend(id.to_be_bytes());
    let name: Vec<u8> = name
        .chars()
        .take(255)
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect();
    out.push(name.len() as u8);
    out.extend(&name);
    if (name.len() + 1) % 2 != 0 {
        out.push(0);
    }
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
    if data.len() % 2 != 0 {
        out.push(0);
    }
}
// A filled outline of a geometric stroke. Round segment caps/joins are encoded
// as cubic Bezier circles, combined using Photoshop's vector-mask union operation.
pub fn shape_mask(s: &VectorStroke, doc: &Document) -> Vec<u8> {
    if !s.polyline {
        return stroke_outline(s, doc);
    }
    let mut out = blank();
    record(&mut out, 8, &0u16.to_be_bytes());
    let radius = if s.settings.tool == crate::core::pencil::ToolKind::Pencil {
        s.settings.shape_width_px * 0.5
    } else {
        s.settings.brush_size_px * 0.5
    };
    let points: Vec<Vec2> = s.points.iter().map(|&p| v(p)).collect();
    let mut add = |knots: &[Knot]| {
        let at = out.len();
        append(
            &mut out,
            knots,
            true,
            doc.spec.width_px as f32,
            doc.spec.height_px as f32,
        );
        // Modern subpath operation 1 (union) and layout marker 1.
        out[at + 4..at + 6].copy_from_slice(&1u16.to_be_bytes());
        out[at + 6..at + 8].copy_from_slice(&1u16.to_be_bytes());
    };
    for pair in points.windows(2) {
        let direction = (pair[1] - pair[0]).normalized();
        let normal = Vec2::new(-direction.y, direction.x) * radius;
        let p = [
            pair[0] + normal,
            pair[1] + normal,
            pair[1] - normal,
            pair[0] - normal,
        ];
        let k = p.map(|p| Knot {
            before: p,
            point: p,
            after: p,
        });
        add(&k);
    }
    let k = 0.5522848 * radius;
    for &p in points.iter().take(points.len().saturating_sub(usize::from(
        points.len() > 2 && (points[0] - points[points.len() - 1]).length() < 0.001,
    ))) {
        let offsets = [
            Vec2::new(radius, 0.),
            Vec2::new(0., radius),
            Vec2::new(-radius, 0.),
            Vec2::new(0., -radius),
        ];
        let tangents = [
            Vec2::new(0., k),
            Vec2::new(-k, 0.),
            Vec2::new(0., -k),
            Vec2::new(k, 0.),
        ];
        let knots = std::array::from_fn::<_, 4, _>(|i| Knot {
            before: p + offsets[i] - tangents[i],
            point: p + offsets[i],
            after: p + offsets[i] + tangents[i],
        });
        add(&knots);
    }
    let mut mask = Vec::new();
    mask.extend(3u32.to_be_bytes());
    mask.extend(0u32.to_be_bytes());
    mask.extend(out);
    mask
}
fn id(b: &mut Vec<u8>, key: &[u8; 4]) {
    b.extend(0u32.to_be_bytes());
    b.extend(key);
}
fn descriptor(b: &mut Vec<u8>, class: &[u8; 4], count: u32) {
    b.extend(0u32.to_be_bytes());
    id(b, class);
    b.extend(count.to_be_bytes());
}
pub fn solid_color(rgb: [u8; 3]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(16u32.to_be_bytes());
    descriptor(&mut b, b"null", 1);
    id(&mut b, b"Clr ");
    b.extend(b"Objc");
    descriptor(&mut b, b"RGBC", 3);
    for (key, value) in [(*b"Rd  ", rgb[0]), (*b"Grn ", rgb[1]), (*b"Bl  ", rgb[2])] {
        id(&mut b, &key);
        b.extend(b"doub");
        b.extend((value as f64).to_be_bytes());
    }
    b
}
pub fn tagged<W: Write>(w: &mut W, key: &[u8; 4], data: &[u8]) -> io::Result<()> {
    w.write_all(b"8BIM")?;
    w.write_all(key)?;
    w.write_all(&(data.len() as u32).to_be_bytes())?;
    w.write_all(data)?;
    if data.len() % 2 != 0 {
        w.write_all(&[0])?;
    }
    Ok(())
}

/// Clean native vector alternative: the brush centerline and pressure-dependent
/// width become a filled outline. Paper grain / sampled-tip pixels are retained
/// on the original texture layer, not falsely represented as vector brush data.
fn stroke_outline(s: &VectorStroke, doc: &Document) -> Vec<u8> {
    use crate::core::{contact::PencilTipContact, pencil::ToolKind};
    let (knots, _) = centerline(s);
    if knots.is_empty() {
        return blank();
    }
    let radius = |index: usize| {
        let p = s.points[index.min(s.points.len() - 1)];
        let pressure = p.pressure.clamp(0., 1.);
        let r = match s.settings.tool {
            ToolKind::Pencil => {
                PencilTipContact::from_state_with_sharpness(
                    s.tip.effective_core_diameter_px(doc.spec.dpi)
                        * s.settings.pencil_geometry_scale,
                    pressure,
                    p.tilt_deg,
                    p.azimuth_deg,
                    s.settings.grade.formulation().core_hardness,
                    s.settings.tip_sharpness,
                )
                .cross_radius_px
            }
            ToolKind::Tissue => s.settings.tissue_size_px * 0.5 * (0.65 + 0.35 * pressure.sqrt()),
            _ => s.settings.brush_size_px * 0.5 * (0.65 + 0.35 * pressure.sqrt()),
        };
        r.max(0.1)
    };
    let mut samples = Vec::new();
    for (i, pair) in knots.windows(2).enumerate() {
        let length = (pair[1].point - pair[0].point).length();
        let steps = (length / 4.).ceil().clamp(2., 32.) as usize;
        for j in 0..steps {
            let t = j as f32 / steps as f32;
            let u = 1. - t;
            let point = pair[0].point * u * u * u
                + pair[0].after * 3. * u * u * t
                + pair[1].before * 3. * u * t * t
                + pair[1].point * t * t * t;
            samples.push((point, radius(i) * (1. - t) + radius(i + 1) * t));
        }
    }
    samples.push((knots.last().unwrap().point, radius(s.points.len() - 1)));
    let mut out = blank();
    record(&mut out, 8, &0u16.to_be_bytes());
    let mut add = |knots: &[Knot]| {
        if knots.is_empty() {
            return;
        }
        let at = out.len();
        append(
            &mut out,
            knots,
            true,
            doc.spec.width_px as f32,
            doc.spec.height_px as f32,
        );
        out[at + 4..at + 6].copy_from_slice(&1u16.to_be_bytes());
        out[at + 6..at + 8].copy_from_slice(&1u16.to_be_bytes());
    };
    if samples.len() > 1 {
        let mut left = Vec::new();
        let mut right = Vec::new();
        for (i, &(point, r)) in samples.iter().enumerate() {
            let tangent = (samples[(i + 1).min(samples.len() - 1)].0
                - samples[i.saturating_sub(1)].0)
                .normalized();
            let normal = if tangent.length_sq() > 0. {
                Vec2::new(-tangent.y, tangent.x)
            } else {
                Vec2::Y
            };
            left.push(point + normal * r);
            right.push(point - normal * r);
        }
        left.extend(right.into_iter().rev());
        add(&left
            .into_iter()
            .map(|p| Knot {
                before: p,
                point: p,
                after: p,
            })
            .collect::<Vec<_>>());
    }
    for &(p, r) in [samples.first().unwrap(), samples.last().unwrap()] {
        let k = 0.5522848 * r;
        let offsets = [
            Vec2::new(r, 0.),
            Vec2::new(0., r),
            Vec2::new(-r, 0.),
            Vec2::new(0., -r),
        ];
        let tangents = [
            Vec2::new(0., k),
            Vec2::new(-k, 0.),
            Vec2::new(0., -k),
            Vec2::new(k, 0.),
        ];
        add(&std::array::from_fn::<_, 4, _>(|i| Knot {
            before: p + offsets[i] - tangents[i],
            point: p + offsets[i],
            after: p + offsets[i] + tangents[i],
        }));
    }
    let mut mask = Vec::new();
    mask.extend(3u32.to_be_bytes());
    mask.extend(0u32.to_be_bytes());
    mask.extend(out);
    mask
}
