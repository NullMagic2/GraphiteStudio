use super::{document::Document, history::EditTransaction, material::PixelDepositState};
use eframe::egui::{Pos2, Rect, Vec2};
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct Selection {
    #[serde(with = "polygon_serde")]
    pub polygon: Vec<Vec2>,
    #[serde(skip)]
    mask: Option<std::sync::Arc<Vec<u8>>>,
}
impl Selection {
    pub fn quarter_turn(&mut self, turn: &super::orientation::QuarterTurn) {
        for p in &mut self.polygon {
            *p = turn.point(*p);
        }
        if turn.expanded() && self.mask.is_some() {
            let (w,h)=turn.size();
            self.set(self.polygon.clone(),w,h);
            return;
        }
        if let Some(mask) = &mut self.mask {
            turn.grid(std::sync::Arc::make_mut(mask));
        }
    }
    pub fn allows(&self, index: usize) -> bool {
        self.mask.as_ref().is_none_or(|m| m[index] != 0)
    }
    pub fn set(&mut self, polygon: Vec<Vec2>, width: usize, height: usize) {
        if polygon.len() < 3 {
            *self = Self::default();
            return;
        }
        let mut mask = vec![0; width * height];
        for y in 0..height {
            let scan = y as f32 + 0.5;
            let mut crossings = Vec::new();
            for (a, b) in polygon
                .iter()
                .zip(polygon.iter().cycle().skip(1))
                .take(polygon.len())
            {
                if (a.y > scan) != (b.y > scan) {
                    crossings.push(a.x + (scan - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
            crossings.sort_by(f32::total_cmp);
            for pair in crossings.chunks_exact(2) {
                let a = (pair[0] - 0.5).ceil().clamp(0., width as f32) as usize;
                let b = (pair[1] - 0.5).ceil().clamp(0., width as f32) as usize;
                mask[y * width + a..y * width + b].fill(1);
            }
        }
        self.polygon = polygon;
        self.mask = Some(std::sync::Arc::new(mask));
    }
}
pub struct Cutout {
    pub bounds: Rect,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<PixelDepositState>,
}
fn add(a: &mut PixelDepositState, b: PixelDepositState, k: f32) {
    macro_rules! channels {($($f:ident),*)=>{$(a.$f+=b.$f*k;)*}}
    channels!(
        graphite_mass,
        clay_mass,
        wax_mass,
        loose_mass,
        compacted_mass,
        orientation_x,
        orientation_y,
        color_r_mass,
        color_g_mass,
        color_b_mass
    );
}
impl Cutout {
    pub fn capture(doc: &Document) -> Result<Option<Self>, String> {
        let (mut x0, mut y0, mut x1, mut y1) = (doc.spec.width_px, doc.spec.height_px, 0, 0);
        for y in 0..doc.spec.height_px {
            for x in 0..doc.spec.width_px {
                let i = doc.index(x, y);
                if doc.selection.allows(i) && doc.surface.total_deposit(i) > 1e-8 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x1 == 0 {
            return Ok(None);
        }
        let (width, height) = (x1 - x0, y1 - y0);
        let mut pixels = vec![PixelDepositState::default(); width * height];
        for y in y0..y1 {
            for x in x0..x1 {
                let i = doc.index(x, y);
                if doc.selection.allows(i) {
                    pixels[(y - y0) * width + x - x0] = doc.surface.deposit_pixel(i);
                }
            }
        }
        Ok(Some(Self {
            bounds: Rect::from_min_max(
                Pos2::new(x0 as f32, y0 as f32),
                Pos2::new(x1 as f32, y1 as f32),
            ),
            width,
            height,
            pixels,
        }))
    }
    pub fn lift(&self, doc: &mut Document, tx: &mut EditTransaction) {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.pixels[y * self.width + x].total_deposit() <= 0. {
                    continue;
                }
                let i = doc.index(
                    x + self.bounds.min.x as usize,
                    y + self.bounds.min.y as usize,
                );
                tx.remember(i, doc);
                doc.surface.set_deposit_pixel(i, Default::default());
            }
        }
        doc.mark_all_dirty();
    }
    fn sample(&self, x: f32, y: f32) -> PixelDepositState {
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let mut out = PixelDepositState::default();
        for (dx, dy, k) in [
            (0, 0, (1. - fx) * (1. - fy)),
            (1, 0, fx * (1. - fy)),
            (0, 1, (1. - fx) * fy),
            (1, 1, fx * fy),
        ] {
            let (a, b) = (ix + dx, iy + dy);
            if a >= 0 && b >= 0 && (a as usize) < self.width && (b as usize) < self.height {
                add(
                    &mut out,
                    self.pixels[b as usize * self.width + a as usize],
                    k,
                );
            }
        }
        out
    }
    pub fn place(&self, doc: &mut Document, target: Rect, tx: &mut EditTransaction) {
        self.place_rotated(doc, target, 0., tx);
    }
    pub fn place_rotated(
        &self,
        doc: &mut Document,
        target: Rect,
        angle: f32,
        tx: &mut EditTransaction,
    ) {
        self.place_transformed(doc,target,angle,[false,false],tx);
    }
    pub fn place_transformed(&self,doc:&mut Document,target:Rect,angle:f32,flip:[bool;2],tx:&mut EditTransaction) {
        let mut bounds = Rect::NOTHING;
        for p in [
            target.left_top(),
            target.right_top(),
            target.right_bottom(),
            target.left_bottom(),
        ] {
            bounds.extend_with(super::vector::rotate(p, target.center(), angle));
        }
        let min = bounds.min.max(Pos2::ZERO);
        let max = bounds.max.min(Pos2::new(
            doc.spec.width_px as f32,
            doc.spec.height_px as f32,
        ));
        let (s, c) = (2. * angle).sin_cos();
        for y in min.y.floor().max(0.) as usize..max.y.ceil().max(0.) as usize {
            for x in min.x.floor().max(0.) as usize..max.x.ceil().max(0.) as usize {
                let local = super::vector::rotate(
                    Pos2::new(x as f32 + 0.5, y as f32 + 0.5),
                    target.center(),
                    -angle,
                );
                let mut uv=(local-target.min)/target.size();
                if flip[0]{uv.x=1.-uv.x;}if flip[1]{uv.y=1.-uv.y;}
                let mut p = self.sample(uv.x*self.width as f32-0.5,uv.y*self.height as f32-0.5);
                if flip[0]!=flip[1]{p.orientation_y=-p.orientation_y;}
                if p.total_deposit() <= 1e-10 {
                    continue;
                }
                (p.orientation_x, p.orientation_y) = (
                    p.orientation_x * c - p.orientation_y * s,
                    p.orientation_x * s + p.orientation_y * c,
                );
                let i = doc.index(x, y);
                tx.remember(i, doc);
                let mut value = doc.surface.deposit_pixel(i);
                add(&mut value, p, 1.);
                doc.surface.set_deposit_pixel(i, value);
            }
        }
        doc.mark_all_dirty();
    }
}

mod polygon_serde {
    use super::Vec2;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(v: &[Vec2], s: S) -> Result<S::Ok, S::Error> {
        v.iter()
            .map(|p| [p.x, p.y])
            .collect::<Vec<_>>()
            .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec2>, D::Error> {
        Ok(Vec::<[f32; 2]>::deserialize(d)?
            .into_iter()
            .map(|p| Vec2::new(p[0], p[1]))
            .collect())
    }
}
