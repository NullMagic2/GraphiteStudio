//! Retained cubic geometry for imported Photoshop paths and shape layers.
use super::*;
use tiny_skia::{FillRule, Mask, Path, PathBuilder, Transform};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Knot {
    #[serde(with = "point_serde")]
    pub before: Vec2,
    #[serde(with = "point_serde")]
    pub anchor: Vec2,
    #[serde(with = "point_serde")]
    pub after: Vec2,
}
mod point_serde {
    use super::Vec2;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(p: &Vec2, s: S) -> Result<S::Ok, S::Error> {
        [p.x, p.y].serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec2, D::Error> {
        let p = <[f32; 2]>::deserialize(d)?;
        Ok(Vec2::new(p[0], p[1]))
    }
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Subpath {
    pub knots: Vec<Knot>,
    pub closed: bool,
    pub operation: u16,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Shape {
    pub paths: Vec<Subpath>,
    pub fill: Option<[f32; 4]>,
    pub stroke: Option<ShapeStroke>,
    pub initial_fill: bool,
    pub inverted: bool,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct ShapeStroke {
    pub color: [f32; 4],
    pub width: f32,
    pub cap: u8,
    pub join: u8,
    pub miter: f32,
}

impl Subpath {
    pub fn path(&self) -> Option<Path> {
        let first = self.knots.first()?;
        let mut b = PathBuilder::new();
        b.move_to(first.anchor.x, first.anchor.y);
        for (a, z) in self.knots.iter().zip(
            self.knots
                .iter()
                .skip(1)
                .chain(self.closed.then_some(first)),
        ) {
            b.cubic_to(
                a.after.x, a.after.y, z.before.x, z.before.y, z.anchor.x, z.anchor.y,
            );
        }
        if self.closed {
            b.close();
        }
        b.finish()
    }
}
impl Shape {
    pub fn valid(&self) -> bool {
        let color = |c: &[f32; 4]| c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v));
        self.paths.len() <= 4096
            && self.paths.iter().map(|p| p.knots.len()).sum::<usize>() <= 100_000
            && self.paths.iter().all(|p| {
                p.operation <= 3
                    && p.knots.iter().all(|k| {
                        [k.before, k.anchor, k.after]
                            .iter()
                            .all(|v| v.is_finite() && v.x.abs() < 1e7 && v.y.abs() < 1e7)
                    })
            })
            && self.fill.as_ref().is_none_or(color)
            && self.stroke.as_ref().is_none_or(|s| {
                color(&s.color)
                    && s.width.is_finite()
                    && (0.0..=1e5).contains(&s.width)
                    && s.miter.is_finite()
                    && (1.0..=1e4).contains(&s.miter)
                    && s.cap <= 2
                    && s.join <= 2
            })
    }
    pub fn map(&mut self, f: impl Fn(Vec2) -> Vec2, width_scale: f32) {
        for path in &mut self.paths {
            for k in &mut path.knots {
                k.before = f(k.before);
                k.anchor = f(k.anchor);
                k.after = f(k.after);
            }
        }
        if let Some(stroke) = &mut self.stroke {
            stroke.width *= width_scale;
        }
    }
    pub fn bounds(&self) -> Rect {
        let mut bounds = Rect::NOTHING;
        for k in self.paths.iter().flat_map(|p| &p.knots) {
            for v in [k.before, k.anchor, k.after] {
                bounds.extend_with(v.to_pos2());
            }
        }
        bounds.expand(
            self.stroke
                .as_ref()
                .map_or(1., |s| s.width * s.miter.max(1.) / 2. + 1.),
        )
    }
    fn coverage(&self, w: u32, h: u32, transform: Transform, outline_tolerance: f32) -> Vec<u8> {
        let mut out = vec![if self.initial_fill { 255u8 } else { 0 }; w as usize * h as usize];
        for subpath in &self.paths {
            let Some(path) = subpath.path() else {
                continue;
            };
            let mut mask = Mask::new(w, h).expect("validated image dimensions");
            mask.fill_path(&path, FillRule::EvenOdd, true, transform);
            for (a, &b) in out.iter_mut().zip(mask.data()) {
                let x = *a as u32;
                let y = b as u32;
                *a = match subpath.operation {
                    1 => x + y - x * y / 255,
                    2 => x * (255 - y) / 255,
                    3 => x * y / 255,
                    _ => x * (255 - y) / 255 + y * (255 - x) / 255,
                }
                .min(255) as u8;
            }
        }
        if self.inverted {
            for a in &mut out {
                *a = 255 - *a;
            }
        }
        if outline_tolerance > 0. {
            for subpath in &self.paths {
                if let Some(path) = subpath.path().and_then(|p| {
                    p.stroke(
                        &tiny_skia::Stroke {
                            width: outline_tolerance * 2.,
                            ..Default::default()
                        },
                        1.,
                    )
                }) {
                    let mut mask = Mask::new(w, h).unwrap();
                    mask.fill_path(&path, FillRule::Winding, true, transform);
                    for (a, &b) in out.iter_mut().zip(mask.data()) {
                        *a = (*a).max(b);
                    }
                }
            }
        }
        out
    }
    pub fn rgba(&self, w: u32, h: u32, transform: Transform) -> Vec<[f32; 4]> {
        let mut pixels = vec![[0.; 4]; w as usize * h as usize];
        if let Some(fill) = self.fill {
            let coverage = self.coverage(w, h, transform, 0.);
            for (pixel, a) in pixels.iter_mut().zip(coverage) {
                *pixel = [fill[0], fill[1], fill[2], fill[3] * a as f32 / 255.];
            }
        }
        if let Some(stroke) = &self.stroke {
            if stroke.width > 0. {
                let style = tiny_skia::Stroke {
                    width: stroke.width,
                    miter_limit: stroke.miter,
                    line_cap: match stroke.cap {
                        1 => tiny_skia::LineCap::Round,
                        2 => tiny_skia::LineCap::Square,
                        _ => tiny_skia::LineCap::Butt,
                    },
                    line_join: match stroke.join {
                        1 => tiny_skia::LineJoin::Round,
                        2 => tiny_skia::LineJoin::Bevel,
                        _ => tiny_skia::LineJoin::Miter,
                    },
                    ..Default::default()
                };
                let mut coverage = vec![0u8; pixels.len()];
                for subpath in &self.paths {
                    if let Some(path) = subpath.path().and_then(|p| p.stroke(&style, 1.)) {
                        let mut mask = Mask::new(w, h).unwrap();
                        mask.fill_path(&path, FillRule::Winding, true, transform);
                        for (a, &b) in coverage.iter_mut().zip(mask.data()) {
                            *a = (*a).max(b);
                        }
                    }
                }
                for (p, a) in pixels.iter_mut().zip(coverage) {
                    let alpha = stroke.color[3] * a as f32 / 255.;
                    let combined = alpha + p[3] * (1. - alpha);
                    if combined > 0. {
                        for c in 0..3 {
                            p[c] =
                                (stroke.color[c] * alpha + p[c] * p[3] * (1. - alpha)) / combined;
                        }
                    }
                    p[3] = combined;
                }
            }
        }
        pixels
    }
    pub fn hit_test(&self, point: Vec2, tolerance: f32) -> bool {
        if !self.bounds().expand(tolerance).contains(point.to_pos2()) {
            return false;
        }
        let transform = Transform::from_translate(0.5 - point.x, 0.5 - point.y);
        if self.rgba(1, 1, transform)[0][3] > 0.01 {
            return true;
        }
        // Edges remain easy to select at low zoom, including unfilled paths.
        let mut edge = self.clone();
        edge.fill = None;
        edge.stroke = Some(ShapeStroke {
            color: [1.; 4],
            width: tolerance * 2.,
            cap: 1,
            join: 1,
            miter: 1.,
        });
        edge.rgba(1, 1, transform)[0][3] > 0.01
    }
    pub fn apply(&self, doc: &mut Document, tx: &mut EditTransaction) {
        let w = doc.spec.width_px;
        let h = doc.spec.height_px;
        let bounds = self.bounds().intersect(Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            Vec2::new(w as f32, h as f32),
        ));
        let bounds = if self.inverted || self.initial_fill {
            Rect::from_min_size(eframe::egui::Pos2::ZERO, Vec2::new(w as f32, h as f32))
        } else {
            bounds
        };
        if !bounds.is_positive() {
            return;
        }
        let x0 = bounds.min.x.floor().max(0.) as usize;
        let y0 = bounds.min.y.floor().max(0.) as usize;
        let x1 = (bounds.max.x.ceil() as usize).min(w);
        let y1 = (bounds.max.y.ceil() as usize).min(h);
        let rw = x1 - x0;
        let rh = y1 - y0;
        if rw == 0 || rh == 0 {
            return;
        }
        let pixels = self.rgba(
            rw as u32,
            rh as u32,
            Transform::from_translate(-(x0 as f32), -(y0 as f32)),
        );
        for (offset, mut pixel) in pixels.into_iter().enumerate() {
            if pixel[3] <= 0. {
                continue;
            }
            let index = (y0 + offset / rw) * w + x0 + offset % rw;
            let old = crate::render::layer_pixel_rgba(doc, Some(doc.active_layer_index()), index);
            let a = pixel[3] + old[3] * (1. - pixel[3]);
            for c in 0..3 {
                pixel[c] = (pixel[c] * pixel[3] + old[c] * old[3] * (1. - pixel[3])) / a;
            }
            pixel[3] = a;
            tx.remember(index, doc);
            doc.surface.set_deposit_pixel(
                index,
                crate::core::material::PixelDepositState::from_rgba(pixel),
            );
        }
    }
}
