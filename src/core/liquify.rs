//! Brush-driven inverse deformation. Resample the session's original material, never
//! repeatedly resample already-warped pixels. Paper and other layers are untouched.
use super::{
    document::{DirtyRect, Document},
    history::EditTransaction,
    material::{PixelDepositState, SparseDepositState},
    orientation::QuarterTurn,
};
use eframe::egui::Vec2;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Push,
    TwirlLeft,
    TwirlRight,
    Pinch,
    Expand,
    Crystals,
    Edge,
    Reconstruct,
}
impl Mode {
    pub const ALL: [Self; 8] = [
        Self::Push,
        Self::TwirlLeft,
        Self::TwirlRight,
        Self::Pinch,
        Self::Expand,
        Self::Crystals,
        Self::Edge,
        Self::Reconstruct,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Push => "Push",
            Self::TwirlLeft => "Twirl Left",
            Self::TwirlRight => "Twirl Right",
            Self::Pinch => "Pinch",
            Self::Expand => "Expand",
            Self::Crystals => "Crystals",
            Self::Edge => "Edge",
            Self::Reconstruct => "Reconstruct",
        }
    }
}
#[derive(Clone)]
pub struct Settings {
    pub mode: Mode,
    pub size: f32,
    pub pressure: f32,
    pub distortion: f32,
    pub momentum: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Push,
            size: 160.,
            pressure: 0.65,
            distortion: 0.,
            momentum: 0.,
        }
    }
}
pub struct Warp {
    pub original: SparseDepositState,
    pub offsets: HashMap<usize, Vec2, ahash::RandomState>,
    pub transaction: EditTransaction,
    pub amount: f32,
    pub layer_id: u64,
}
impl Warp {
    pub fn new(doc: &Document) -> Self {
        let mut original = SparseDepositState::new(doc.spec.width_px, doc.spec.height_px);
        original.capture_from_surface(&doc.surface);
        Self {
            original,
            offsets: HashMap::default(),
            transaction: Default::default(),
            amount: 1.,
            layer_id: doc.active_layer_id(),
        }
    }
    pub fn expand(&mut self, map: &mut QuarterTurn) {
        self.original = map.sparse(&self.original);
        self.offsets = std::mem::take(&mut self.offsets)
            .into_iter()
            .map(|(i, v)| (map.index(i), v))
            .collect();
        self.transaction.expand(map);
    }
    fn offset_at(&self, p: Vec2) -> Vec2 {
        let mut out = Vec2::ZERO;
        for (i, weight) in samples(p, self.original.width, self.original.height) {
            if let Some(v) = self.offsets.get(&i) {
                out += *v * weight;
            }
        }
        out
    }
    pub fn material_at(&self, p: Vec2) -> PixelDepositState {
        let mut out = PixelDepositState::default();
        for (i, weight) in samples(p, self.original.width, self.original.height) {
            let v = self.original.get(i);
            macro_rules! mix {($($f:ident),*)=>{$(out.$f+=v.$f*weight;)*};}
            mix!(
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
        out
    }
    // Integrate only the extra source footprint caused by compression. Principal
    // axes prevent a strong squeeze in one direction from softening the other.
    // The bilinear reconstruction already covers one source texel; subtract that
    // footprint before adding taps. Every tap reads the immutable original.
    fn filtered_at(&self, q: Vec2, offset: Vec2) -> PixelDepositState {
        let w = self.original.width;
        let x = q.x as usize;
        let y = q.y as usize;
        let i = y * w + x;
        let get = |i| self.offsets.get(&i).copied().unwrap_or_default();
        let left = if x > 0 { get(i - 1) } else { Vec2::ZERO };
        let right = if x + 1 < w { get(i + 1) } else { Vec2::ZERO };
        let up = if y > 0 { get(i - w) } else { Vec2::ZERO };
        let down = if y + 1 < self.original.height {
            get(i + w)
        } else {
            Vec2::ZERO
        };
        let dx = Vec2::X + (right - left) * (0.5 * self.amount);
        let dy = Vec2::Y + (down - up) * (0.5 * self.amount);
        let a = dx.x * dx.x + dy.x * dy.x;
        let b = dx.x * dx.y + dy.x * dy.y;
        let c = dx.y * dx.y + dy.y * dy.y;
        let disc = ((a - c).powi(2) + 4. * b * b).sqrt();
        let major = ((a + c + disc) * 0.5 - 1.).max(0.).sqrt();
        let minor = ((a + c - disc) * 0.5 - 1.).max(0.).sqrt();
        let p = q + offset;
        if major < 0.25 {
            return self.material_at(p);
        }
        let angle = 0.5 * (2. * b).atan2(a - c);
        let u = Vec2::new(angle.cos(), angle.sin());
        let v = Vec2::new(-u.y, u.x);
        // Bound work per output pixel for interactive response, not image size.
        let nx = (major * 2.).ceil().clamp(2., 16.) as usize;
        let ny = if minor < 0.25 {
            1
        } else {
            (minor * 2.).ceil().clamp(2., 16.) as usize
        };
        let mut out = PixelDepositState::default();
        let weight = 1. / (nx * ny) as f32;
        for y in 0..ny {
            for x in 0..nx {
                let sample = self.material_at(
                    p + u * (((x as f32 + 0.5) / nx as f32 - 0.5) * major)
                        + v * (((y as f32 + 0.5) / ny as f32 - 0.5) * minor),
                );
                macro_rules! mix {($($f:ident),*)=>{$(out.$f += sample.$f * weight;)*};}
                mix!(
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
        }
        out
    }
    pub fn set_amount(&mut self, doc: &mut Document, amount: f32) {
        let patches = self.compute_amount(amount);
        self.apply_updates(doc, patches);
    }
    pub fn compute_amount(&mut self, amount: f32) -> Vec<(usize, PixelDepositState)> {
        self.amount = amount.clamp(0., 1.);
        let indices: Vec<_> = self.offsets.keys().copied().collect();
        self.resample(&indices)
    }
    pub fn resample(&self, indices: &[usize]) -> Vec<(usize, PixelDepositState)> {
        let w = self.original.width;
        parallel(indices, |&i| {
            let p = Vec2::new((i % w) as f32, (i / w) as f32);
            let offset = self.offsets[&i] * self.amount;
            (
                i,
                if offset.length_sq() < 1e-12 {
                    self.original.get(i)
                } else {
                    self.filtered_at(p, offset)
                },
            )
        })
    }
    pub fn reset(&mut self, doc: &mut Document) {
        std::mem::take(&mut self.transaction).rollback(doc);
        self.offsets.clear();
        self.amount = 1.;
        doc.mark_all_dirty();
    }
    pub fn apply_updates(&mut self, doc: &mut Document, updates: Vec<(usize, PixelDepositState)>) {
        let mut dirty: Option<DirtyRect> = None;
        for (i, value) in updates {
            if value != doc.surface.deposit_pixel(i) {
                self.transaction.remember(i, doc);
                doc.surface.set_deposit_pixel(i, value);
                let x = i % doc.spec.width_px;
                let y = i / doc.spec.width_px;
                let r = DirtyRect::new(x, y, x + 1, y + 1);
                dirty = Some(dirty.map_or(r, |d| d.union(r)));
            }
        }
        if let Some(r) = dirty {
            doc.mark_dirty(r);
        }
    }
    pub fn compute(
        &mut self,
        selection: &super::selection::Selection,
        center: Vec2,
        delta: Vec2,
        direction: Vec2,
        force: f32,
        dose: f32,
        s: &Settings,
    ) -> Vec<(usize, PixelDepositState)> {
        let indices = self.deform(selection, center, delta, direction, force, dose, s);
        self.resample(&indices)
    }
    pub fn deform(
        &mut self,
        selection: &super::selection::Selection,
        center: Vec2,
        delta: Vec2,
        direction: Vec2,
        force: f32,
        dose: f32,
        s: &Settings,
    ) -> Vec<usize> {
        let strength = s.pressure.clamp(0., 1.) * force.clamp(0., 1.);
        if strength <= 0.
            || !center.is_finite()
            || !delta.is_finite()
            || (s.mode == Mode::Push && delta.length_sq() < 1e-12)
            || (s.mode != Mode::Push && dose <= 0.)
        {
            return Vec::new();
        }
        let radius = s.size.max(1.) * 0.5;
        let (w, h) = (self.original.width, self.original.height);
        let x0 = (center.x - radius - 1.).floor().clamp(0., w as f32) as usize;
        let y0 = (center.y - radius - 1.).floor().clamp(0., h as f32) as usize;
        let x1 = (center.x + radius + 1.).ceil().clamp(0., w as f32) as usize;
        let y1 = (center.y + radius + 1.).ceil().clamp(0., h as f32) as usize;
        let direction = if direction.length_sq() > 1e-8 {
            direction.normalized()
        } else {
            Vec2::X
        };
        let mut indices = Vec::new();
        let mut points = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * w + x;
                let q = Vec2::new(x as f32, y as f32);
                if selection.allows(i) && (q - center).length() < radius {
                    let old = self.offsets.get(&i).copied().unwrap_or_default();
                    indices.push(i);
                    points.push([q.x, q.y, old.x, old.y]);
                }
            }
        }
        let params = [
            center.x,
            center.y,
            delta.x,
            delta.y,
            direction.x,
            direction.y,
            radius,
            strength,
            dose.clamp(0., 4.),
            s.distortion.clamp(0., 1.),
            s.mode as u32 as f32,
            points.len() as f32,
            0.,
            0.,
            0.,
            0.,
        ];
        let compose = |p: [f32; 2]| {
            let q = Vec2::new(p[0], p[1]);
            if s.mode == Mode::Reconstruct {
                q
            } else {
                q + self.offset_at(q)
            }
        };
        let offsets = if let Some(sources) = super::liquify_gpu::transform(&points, params) {
            parallel(&sources, |&p| compose(p))
        } else {
            // Fuse geometry and field sampling on the fallback; avoid another
            // thread launch, intermediate allocation and full point-array pass.
            parallel(&points, |&p| compose(source(p, params)))
        };
        for ((&i, point), source) in indices.iter().zip(&points).zip(offsets) {
            self.offsets
                .insert(i, source - Vec2::new(point[0], point[1]));
        }
        // Filtering depends on adjacent deformation derivatives. Refresh the
        // one-pixel halo too, so Amount=1 reproduces the displayed result exactly.
        let mut resample_indices = Vec::new();
        for y in y0.saturating_sub(1)..(y1 + 1).min(h) {
            for x in x0.saturating_sub(1)..(x1 + 1).min(w) {
                let i = y * w + x;
                if self.offsets.contains_key(&i) && selection.allows(i) {
                    resample_indices.push(i);
                }
            }
        }
        resample_indices
    }
    pub fn dab(
        &mut self,
        doc: &mut Document,
        center: Vec2,
        delta: Vec2,
        direction: Vec2,
        force: f32,
        dose: f32,
        s: &Settings,
    ) {
        let result = self.compute(&doc.selection, center, delta, direction, force, dose, s);
        self.apply_updates(doc, result);
    }
}
pub(crate) fn parallel<T: Sync, R: Send, F: Fn(&T) -> R + Sync>(data: &[T], f: F) -> Vec<R> {
    if data.len() < 4096 {
        return data.iter().map(f).collect();
    }
    let workers = std::thread::available_parallelism()
        .map_or(2, |v| v.get())
        .min(8);
    std::thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = data
            .chunks(data.len().div_ceil(workers))
            .map(|chunk| scope.spawn(move || chunk.iter().map(f).collect::<Vec<_>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("Liquify worker panicked"))
            .collect()
    })
}
pub(crate) fn source(p: [f32; 4], a: [f32; 16]) -> [f32; 2] {
    let center = Vec2::new(a[0], a[1]);
    let delta = Vec2::new(a[2], a[3]);
    let direction = Vec2::new(a[4], a[5]);
    let radius = a[6];
    let strength = a[7];
    let dose = a[8];
    let chaos = a[9];
    let mode = a[10] as u32;
    let q = Vec2::new(p[0], p[1]);
    let d = q - center;
    let r = d.length() / radius;
    let f = (1. - r * r).max(0.).powi(2);
    let k = strength * f * dose;
    if mode == 7 {
        let next = Vec2::new(p[2], p[3]) * (1. - (k * 0.5).clamp(0., 1.));
        let v = q + if next.length() < 0.01 {
            Vec2::ZERO
        } else {
            next
        };
        return [v.x, v.y];
    }
    let normal = Vec2::new(-direction.y, direction.x);
    let wave = (d.x / radius * 19. + (d.y / radius * 13.).sin() * 2.).sin();
    let mut src = match mode {
        0 => q - delta * strength * f,
        1 | 2 => {
            let angle = k * 0.24 * (1. + chaos * wave * 1.6) * if mode == 2 { -1. } else { 1. };
            let (sn, cs) = angle.sin_cos();
            center + Vec2::new(d.x * cs - d.y * sn, d.x * sn + d.y * cs)
        }
        3 => center + d * (k * 0.22).exp(),
        4 => center + d * (-k * 0.22).exp(),
        6 => q + normal * d.dot(normal) * ((k * 0.4).exp() - 1.),
        5 => {
            let facets = (d.angle() * 9. + r * 8.).sin().abs();
            center + d * (-k * (0.07 + 0.5 * facets) * (1. + chaos * 2.)).exp()
        }
        _ => q,
    };
    if chaos > 0. && !matches!(mode, 1 | 2 | 5) {
        let dose = if mode == 0 {
            delta.length() / radius
        } else {
            dose
        };
        src += Vec2::new(-d.y, d.x) * wave * chaos * strength * f * dose * 0.22;
    }
    [src.x, src.y]
}
fn samples(p: Vec2, w: usize, h: usize) -> impl Iterator<Item = (usize, f32)> {
    let x = p.x.floor() as i64;
    let y = p.y.floor() as i64;
    let fx = p.x - x as f32;
    let fy = p.y - y as f32;
    [
        (x, y, (1. - fx) * (1. - fy)),
        (x + 1, y, fx * (1. - fy)),
        (x, y + 1, (1. - fx) * fy),
        (x + 1, y + 1, fx * fy),
    ]
    .into_iter()
    .filter_map(move |(x, y, t)| {
        (x >= 0 && y >= 0 && x < w as i64 && y < h as i64 && t > 0.)
            .then_some((y.max(0) as usize * w + x.max(0) as usize, t))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{document::CanvasSpec, paper::PaperPreset};
    fn document() -> Document {
        let spec = CanvasSpec::from_physical("Liquify", 25.4, 25.4, 128.);
        let mut doc = Document::new(
            spec,
            PaperPreset::DrawingMedium,
            "White",
            vec![[1.; 3]; 128 * 128],
        );
        for y in 0..128 {
            for x in 0..128 {
                let mass = if (x / 8 + y / 8) % 2 == 0 { 0.8 } else { 0.1 };
                doc.surface.set_deposit_pixel(
                    y * 128 + x,
                    PixelDepositState {
                        graphite_mass: mass,
                        color_r_mass: mass * x as f32 / 128.,
                        color_g_mass: mass * y as f32 / 128.,
                        color_b_mass: mass * 0.3,
                        ..Default::default()
                    },
                );
            }
        }
        doc
    }
    #[test]
    fn batched_resampling_matches_sequential_material_exactly() {
        for mode in Mode::ALL {
            let doc = document();
            let mut sequential = Warp::new(&doc);
            let mut batched = Warp::new(&doc);
            let settings = Settings {
                mode,
                size: 80.,
                ..Default::default()
            };
            let mut output = HashMap::new();
            let mut changed = Vec::new();
            for j in 0..8 {
                let c = Vec2::new(48. + j as f32 * 3., 64.);
                output.extend(sequential.compute(
                    &doc.selection,
                    c,
                    Vec2::new(3., 0.),
                    Vec2::X,
                    1.,
                    0.4,
                    &settings,
                ));
                changed.extend(batched.deform(
                    &doc.selection,
                    c,
                    Vec2::new(3., 0.),
                    Vec2::X,
                    1.,
                    0.4,
                    &settings,
                ));
            }
            changed.sort_unstable();
            changed.dedup();
            assert_eq!(sequential.offsets, batched.offsets);
            for (i, p) in batched.resample(&changed) {
                assert_eq!(p, output[&i], "{} pixel {}", mode.label(), i);
            }
        }
        let doc = document();
        let mut warp = Warp::new(&doc);
        assert!(warp
            .deform(
                &doc.selection,
                Vec2::splat(64.),
                Vec2::ZERO,
                Vec2::X,
                1.,
                1.,
                &Settings::default()
            )
            .is_empty());
        assert!(warp.offsets.is_empty());
    }
    #[test]
    #[ignore = "Manual Liquify latency benchmark"]
    fn latency_benchmark() {
        let n = 768;
        let spec = CanvasSpec::from_physical("Latency", 25.4, 25.4, n as f32);
        let mut doc = Document::new(
            spec,
            PaperPreset::DrawingMedium,
            "White",
            vec![[1.; 3]; n * n],
        );
        for i in 0..n * n {
            doc.surface.set_deposit_pixel(
                i,
                PixelDepositState {
                    graphite_mass: if (i % n / 8 + i / n / 8) % 2 == 0 {
                        0.8
                    } else {
                        0.1
                    },
                    ..Default::default()
                },
            );
        }
        for mode in [Mode::Push, Mode::TwirlRight, Mode::Crystals] {
            let settings = Settings {
                mode,
                size: 600.,
                ..Default::default()
            };
            let mut times = Vec::new();
            for _ in 0..3 {
                let mut warp = Warp::new(&doc);
                let start = std::time::Instant::now();
                let mut changed = Vec::new();
                for j in 0..8 {
                    changed.extend(warp.deform(
                        &doc.selection,
                        Vec2::new(320. + j as f32 * 6., 384.),
                        Vec2::new(6., 0.),
                        Vec2::X,
                        1.,
                        0.5,
                        &settings,
                    ));
                }
                changed.sort_unstable();
                changed.dedup();
                std::hint::black_box(warp.resample(&changed));
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{} 600px eight-step batch median {:.2}ms",
                mode.label(),
                times[1]
            );
        }
    }
    #[test]
    fn compression_antialiases_without_softening_perpendicular_detail() {
        let mut doc = document();
        for y in 0..128 {
            for x in 0..128 {
                doc.surface.set_deposit_pixel(
                    y * 128 + x,
                    PixelDepositState {
                        graphite_mass: 1. + (x % 2) as f32,
                        color_r_mass: (y % 2) as f32,
                        ..Default::default()
                    },
                );
            }
        }
        let mut warp = Warp::new(&doc);
        // 4:1 horizontal compression aliases a checker to solid black with a
        // single bilinear lookup. Vertical one-pixel stripes must stay exact.
        for y in 0..128 {
            for x in 0..128 {
                warp.offsets
                    .insert(y * 128 + x, Vec2::new(3. * (x as f32 - 64.), 0.));
            }
        }
        let mut error = 0.;
        for y in 60..68 {
            for x in 56..64 {
                let q = Vec2::new(x as f32, y as f32);
                let offset = warp.offsets[&(y * 128 + x)];
                let result = warp.filtered_at(q, offset);
                error += (result.graphite_mass - 1.5).abs();
                assert!((result.color_r_mass - (y % 2) as f32).abs() < 0.00001);
            }
        }
        assert!(error / 64. < 0.05, "alias error {}", error / 64.);
        warp.offsets.clear();
        for y in 60..68 {
            for x in 56..64 {
                let q = Vec2::new(x as f32, y as f32);
                assert_eq!(
                    warp.filtered_at(q, Vec2::ZERO),
                    warp.original.get(y * 128 + x)
                );
            }
        }
    }
    #[test]
    fn every_mode_amount_reset_selection_and_material_are_correct() {
        for mode in Mode::ALL {
            let mut doc = document();
            let original = doc.surface.clone();
            let mut warp = Warp::new(&doc);
            let settings = Settings {
                mode,
                size: 90.,
                pressure: 0.8,
                ..Default::default()
            };
            if mode == Mode::Reconstruct {
                warp.dab(
                    &mut doc,
                    Vec2::splat(64.),
                    Vec2::new(20., 6.),
                    Vec2::X,
                    1.,
                    1.,
                    &Settings {
                        mode: Mode::Push,
                        ..settings.clone()
                    },
                );
            }
            let previous = doc.surface.graphite_mass.clone();
            warp.dab(
                &mut doc,
                Vec2::splat(64.),
                Vec2::new(15., 3.),
                Vec2::X,
                1.,
                1.,
                &settings,
            );
            assert!(
                doc.surface.graphite_mass != previous,
                "{} did nothing",
                mode.label()
            );
            assert_eq!(doc.surface.current_height, original.current_height);
            assert_eq!(doc.surface.abrasion, original.abrasion);
            let effect = doc.surface.graphite_mass.clone();
            warp.set_amount(&mut doc, 0.);
            assert!(doc.surface.graphite_mass == original.graphite_mass);
            warp.set_amount(&mut doc, 1.);
            assert!(doc.surface.graphite_mass == effect);
            warp.reset(&mut doc);
            assert!(doc.surface.graphite_mass == original.graphite_mass);
            doc.selection.set(
                vec![
                    Vec2::ZERO,
                    Vec2::new(64., 0.),
                    Vec2::new(64., 128.),
                    Vec2::new(0., 128.),
                ],
                128,
                128,
            );
            warp.dab(
                &mut doc,
                Vec2::splat(64.),
                Vec2::new(15., 3.),
                Vec2::X,
                1.,
                1.,
                &settings,
            );
            for y in 0..128 {
                for x in 64..128 {
                    assert_eq!(
                        doc.surface.deposit_pixel(y * 128 + x),
                        original.deposit_pixel(y * 128 + x)
                    );
                }
            }
        }
    }
    #[test]
    fn deformation_directions_pressure_and_distortion_are_distinct() {
        let p = [80., 64., 0., 0.];
        let a = [
            64., 64., 12., 0., 1., 0., 48., 1., 1., 0., 0., 1., 0., 0., 0., 0.,
        ];
        let get = |mode, distortion| {
            let mut a = a;
            a[10] = mode as f32;
            a[9] = distortion;
            source(p, a)
        };
        assert!(get(0, 0.)[0] < p[0]);
        assert!(get(1, 0.)[1] > p[1]);
        assert!(get(2, 0.)[1] < p[1]);
        assert!(get(3, 0.)[0] > p[0]);
        assert!(get(4, 0.)[0] < p[0]);
        assert_ne!(get(5, 0.), get(4, 0.));
        assert_ne!(get(0, 0.), get(0, 1.));
        let mut zero = a;
        zero[7] = 0.;
        assert_eq!(source(p, zero), [p[0], p[1]]);
        let p = [70., 80., 0., 0.];
        let mut edge = a;
        edge[10] = 6.;
        let result = source(p, edge);
        assert_eq!(result[0], p[0]);
        assert!(result[1] > p[1]);
    }
    #[test]
    fn reconstruction_reduces_displacement_and_cpu_uses_multiple_threads() {
        let mut doc = document();
        let mut w = Warp::new(&doc);
        w.dab(
            &mut doc,
            Vec2::splat(64.),
            Vec2::new(24., 8.),
            Vec2::X,
            1.,
            1.,
            &Settings::default(),
        );
        let norm = |w: &Warp| w.offsets.values().map(|v| v.length()).sum::<f32>();
        let before = norm(&w);
        for _ in 0..12 {
            w.dab(
                &mut doc,
                Vec2::splat(64.),
                Vec2::ZERO,
                Vec2::X,
                1.,
                1.,
                &Settings {
                    mode: Mode::Reconstruct,
                    pressure: 1.,
                    ..Default::default()
                },
            );
        }
        assert!(norm(&w) < before * 0.5);
        let ids = std::sync::Mutex::new(std::collections::HashSet::new());
        let output = parallel(&vec![3; 10000], |v| {
            ids.lock().unwrap().insert(std::thread::current().id());
            v * 2
        });
        assert!(output.iter().all(|&v| v == 6));
        if std::thread::available_parallelism().is_ok_and(|n| n.get() > 1) {
            assert!(ids.lock().unwrap().len() > 1);
        }
    }
    #[test]
    #[ignore = "Run on a real GPU to validate every shader mode and readback"]
    fn real_gpu_all_modes_match_cpu() {
        use egui_wgpu::wgpu;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::DX12,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
        let mut checked = 0;
        let points: Vec<_> = (0..16384)
            .map(|i| [(i % 128) as f32, (i / 128) as f32, 4., -3.])
            .collect();
        for adapter in adapters {
            if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
                continue;
            }
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("Liquify validation"),
                    ..Default::default()
                }))
                .unwrap();
            let mut gpu =
                super::super::liquify_gpu::Compute::new(device.clone(), queue.clone()).unwrap();
            let clock = std::time::Instant::now();
            let mut worst = 0f32;
            for mode in 0..8 {
                for distortion in [0., 1.] {
                    let a = [
                        64.,
                        64.,
                        12.,
                        4.,
                        1.,
                        0.,
                        90.,
                        0.8,
                        0.7,
                        distortion,
                        mode as f32,
                        points.len() as f32,
                        0.,
                        0.,
                        0.,
                        0.,
                    ];
                    let actual = gpu.run(&points, a).unwrap();
                    for (p, actual) in points.iter().zip(actual) {
                        let expected = source(*p, a);
                        for i in 0..2 {
                            worst = worst.max((actual[i] - expected[i]).abs());
                        }
                    }
                }
            }
            assert!(worst < 0.01, "GPU deviation: {worst}");
            super::super::liquify_gpu::install(device, queue).unwrap();
            let count = super::super::liquify_gpu::completed_batches();
            let mut doc = document();
            let mut warp = Warp::new(&doc);
            warp.dab(
                &mut doc,
                Vec2::splat(64.),
                Vec2::new(10., 5.),
                Vec2::X,
                1.,
                1.,
                &Settings::default(),
            );
            assert!(super::super::liquify_gpu::completed_batches() > count);
            println!(
                "{} {:?}: all modes passed; max error {worst:.6}; {:?}",
                adapter.get_info().name,
                adapter.get_info().backend,
                clock.elapsed()
            );
            checked += 1;
        }
        assert!(checked > 0, "No hardware compute adapter tested");
    }
}
