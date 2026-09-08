use std::collections::HashMap;

pub(crate) const LAYER_TILE_SIDE: usize = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PixelSurfaceState {
    pub current_height: f32,
    pub abrasion: f32,
    pub graphite_mass: f32,
    pub clay_mass: f32,
    pub wax_mass: f32,
    pub loose_mass: f32,
    pub compacted_mass: f32,
    /// Axial orientation accumulator encoded as cos(2θ).
    pub orientation_x: f32,
    /// Axial orientation accumulator encoded as sin(2θ).
    pub orientation_y: f32,
    /// Mass-weighted pencil body color. These channels move with deposited material.
    pub color_r_mass: f32,
    pub color_g_mass: f32,
    pub color_b_mass: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Default, PartialEq)]
pub struct PixelDepositState {
    pub graphite_mass: f32,
    pub clay_mass: f32,
    pub wax_mass: f32,
    pub loose_mass: f32,
    pub compacted_mass: f32,
    pub orientation_x: f32,
    pub orientation_y: f32,
    pub color_r_mass: f32,
    pub color_g_mass: f32,
    pub color_b_mass: f32,
}

impl PixelDepositState {
    pub fn optical_depth(self) -> f32 {
        let mechanical = self.loose_mass + self.compacted_mass;
        let packed = if mechanical <= 1e-8 {
            0.
        } else {
            (self.compacted_mass / mechanical).clamp(0., 1.)
        };
        (self.graphite_mass * 4.85 + self.clay_mass * 0.62 + self.wax_mass * 0.32)
            * (0.86 + 0.34 * packed)
    }

    pub fn total_deposit(self) -> f32 {
        self.graphite_mass + self.clay_mass + self.wax_mass
    }

    pub fn is_empty(self) -> bool {
        self.total_deposit() <= 1.0e-10
    }
}

impl PixelSurfaceState {
    pub fn deposit(self) -> PixelDepositState {
        PixelDepositState {
            graphite_mass: self.graphite_mass,
            clay_mass: self.clay_mass,
            wax_mass: self.wax_mass,
            loose_mass: self.loose_mass,
            compacted_mass: self.compacted_mass,
            orientation_x: self.orientation_x,
            orientation_y: self.orientation_y,
            color_r_mass: self.color_r_mass,
            color_g_mass: self.color_g_mass,
            color_b_mass: self.color_b_mass,
        }
    }

    /// Fade a complete material edit in premultiplied pigment/coverage space. Reconstruct
    /// optical depth afterwards so 50% stays halfway even for dense, overlapping marks.
    /// This retains workable material for subsequent smudging/erasing, without whitening RGB.
    pub fn with_edit_opacity(self, after: Self, opacity: f32) -> Self {
        let t = opacity.clamp(0., 1.);
        if t == 0. {
            return self;
        }
        if t == 1. {
            return after;
        }
        let a = self.deposit();
        let b = after.deposit();
        let alpha_a = -(-a.optical_depth()).exp_m1();
        let alpha_b = -(-b.optical_depth()).exp_m1();
        let alpha = alpha_a * (1. - t) + alpha_b * t;
        let mix = |a: f32, b: f32| a * (1. - t) + b * t;
        let mut out = Self {
            current_height: mix(self.current_height, after.current_height),
            abrasion: mix(self.abrasion, after.abrasion),
            graphite_mass: mix(a.graphite_mass, b.graphite_mass),
            clay_mass: mix(a.clay_mass, b.clay_mass),
            wax_mass: mix(a.wax_mass, b.wax_mass),
            loose_mass: mix(a.loose_mass, b.loose_mass),
            compacted_mass: mix(a.compacted_mass, b.compacted_mass),
            orientation_x: mix(a.orientation_x, b.orientation_x),
            orientation_y: mix(a.orientation_y, b.orientation_y),
            ..Default::default()
        };
        let depth = out.deposit().optical_depth();
        if depth > 1e-10 && alpha > 1e-10 {
            let scale = -(-alpha.min(1. - f32::EPSILON)).ln_1p() / depth;
            out.graphite_mass *= scale;
            out.clay_mass *= scale;
            out.wax_mass *= scale;
            out.loose_mass *= scale;
            out.compacted_mass *= scale;
            out.orientation_x *= scale;
            out.orientation_y *= scale;
            let total = out.deposit().total_deposit();
            let color = |ca: f32, cb: f32| {
                ((ca / a.total_deposit().max(1e-8)).clamp(0., 1.) * alpha_a * (1. - t)
                    + (cb / b.total_deposit().max(1e-8)).clamp(0., 1.) * alpha_b * t)
                    / alpha
                    * total
            };
            out.color_r_mass = color(a.color_r_mass, b.color_r_mass);
            out.color_g_mass = color(a.color_g_mass, b.color_g_mass);
            out.color_b_mass = color(a.color_b_mass, b.color_b_mass);
        }
        out
    }
}

/// Sparse tile backing for inactive drawing layers.
///
/// Only the active layer uses the dense hot-path arrays in `SurfaceState`. Inactive layers are
/// stored in 32×32 tiles allocated on demand, so adding layers does not immediately multiply the
/// document's full-resolution physical-state memory footprint.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SparseDepositState {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) tiles: HashMap<usize, Box<[PixelDepositState]>>,
}

impl SparseDepositState {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: HashMap::new(),
        }
    }

    fn tile_columns(&self) -> usize {
        self.width.div_ceil(LAYER_TILE_SIDE)
    }

    fn location(&self, index: usize) -> (usize, usize) {
        let x = index % self.width;
        let y = index / self.width;
        let tile_x = x / LAYER_TILE_SIDE;
        let tile_y = y / LAYER_TILE_SIDE;
        let local_x = x % LAYER_TILE_SIDE;
        let local_y = y % LAYER_TILE_SIDE;
        let tile_id = tile_y * self.tile_columns() + tile_x;
        let offset = local_y * LAYER_TILE_SIDE + local_x;
        (tile_id, offset)
    }

    pub fn get(&self, index: usize) -> PixelDepositState {
        debug_assert!(index < self.width.saturating_mul(self.height));
        let (tile_id, offset) = self.location(index);
        self.tiles
            .get(&tile_id)
            .map(|tile| tile[offset])
            .unwrap_or_default()
    }

    /// Borrow one horizontal span inside a tile: renderers can resolve the sparse
    /// tile once for up to 32 pixels instead of hashing every pixel separately.
    pub(crate) fn row_span(&self, x: usize, y: usize, len: usize) -> Option<&[PixelDepositState]> {
        debug_assert!(x + len <= self.width && y < self.height);
        debug_assert!(x % LAYER_TILE_SIDE + len <= LAYER_TILE_SIDE);
        let (tile_id, offset) = self.location(y * self.width + x);
        self.tiles.get(&tile_id).map(|tile| &tile[offset..offset + len])
    }

    pub fn set(&mut self, index: usize, state: PixelDepositState) {
        debug_assert!(index < self.width.saturating_mul(self.height));
        let (tile_id, offset) = self.location(index);
        if state.is_empty() && !self.tiles.contains_key(&tile_id) {
            return;
        }
        let tile = self.tiles.entry(tile_id).or_insert_with(|| {
            vec![PixelDepositState::default(); LAYER_TILE_SIDE * LAYER_TILE_SIDE].into_boxed_slice()
        });
        tile[offset] = state;
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    pub fn capture_from_surface(&mut self, surface: &SurfaceState) {
        self.tiles.clear();
        let n = self.width.saturating_mul(self.height);
        for index in 0..n {
            let state = surface.deposit_pixel(index);
            if !state.is_empty() {
                self.set(index, state);
            }
        }
    }

    pub fn load_into_surface(&self, surface: &mut SurfaceState) {
        surface.clear_deposit();
        let tile_cols = self.tile_columns();
        for (&tile_id, tile) in &self.tiles {
            let tile_x = tile_id % tile_cols;
            let tile_y = tile_id / tile_cols;
            for local_y in 0..LAYER_TILE_SIDE {
                let y = tile_y * LAYER_TILE_SIDE + local_y;
                if y >= self.height {
                    break;
                }
                for local_x in 0..LAYER_TILE_SIDE {
                    let x = tile_x * LAYER_TILE_SIDE + local_x;
                    if x >= self.width {
                        break;
                    }
                    let offset = local_y * LAYER_TILE_SIDE + local_x;
                    let state = tile[offset];
                    if state.is_empty() {
                        continue;
                    }
                    let index = y * self.width + x;
                    surface.set_deposit_pixel(index, state);
                }
            }
        }
    }

    pub fn allocated_tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn occupied_pixel_count(&self) -> usize {
        self.tiles
            .values()
            .map(|tile| tile.iter().filter(|state| !state.is_empty()).count())
            .sum()
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SurfaceState {
    /// Original paper height field. This never changes and is useful for diagnostics,
    /// undo-independent comparisons, and future material-restoration operations.
    pub rest_height: Vec<f32>,
    /// Weak directional fiber field in the range 0..1.
    pub fiber: Vec<f32>,
    /// Continuous statistical fraction of unresolved fiber tops available to contact in each
    /// document pixel. This is deliberately not a binary pore mask.
    pub contact_support: Vec<u8>,
    /// Mesoscopic paper grain used only to perturb the physical contact boundary by sub-pixel
    /// amounts, producing porous pencil edges without white square holes.
    pub edge_grain: Vec<u8>,
    /// Evolving paper surface height. Drawing can flatten raised asperities.
    pub current_height: Vec<f32>,
    /// Accumulated surface disturbance/abrasion in the range 0..1.
    pub abrasion: Vec<f32>,

    /// Deposit channels for the currently active drawing layer.
    pub graphite_mass: Vec<f32>,
    pub clay_mass: Vec<f32>,
    pub wax_mass: Vec<f32>,
    pub loose_mass: Vec<f32>,
    pub compacted_mass: Vec<f32>,
    pub orientation_x: Vec<f32>,
    pub orientation_y: Vec<f32>,
    pub color_r_mass: Vec<f32>,
    pub color_g_mass: Vec<f32>,
    pub color_b_mass: Vec<f32>,
}

impl SurfaceState {
    pub fn from_paper(
        rest_height: Vec<f32>,
        fiber: Vec<f32>,
        contact_support: Vec<u8>,
        edge_grain: Vec<u8>,
    ) -> Self {
        assert_eq!(rest_height.len(), fiber.len());
        assert_eq!(rest_height.len(), contact_support.len());
        assert_eq!(rest_height.len(), edge_grain.len());
        let n = rest_height.len();
        Self {
            current_height: rest_height.clone(),
            rest_height,
            fiber,
            contact_support,
            edge_grain,
            abrasion: vec![0.0; n],
            graphite_mass: vec![0.0; n],
            clay_mass: vec![0.0; n],
            wax_mass: vec![0.0; n],
            loose_mass: vec![0.0; n],
            compacted_mass: vec![0.0; n],
            orientation_x: vec![0.0; n],
            orientation_y: vec![0.0; n],
            color_r_mass: vec![0.0; n],
            color_g_mass: vec![0.0; n],
            color_b_mass: vec![0.0; n],
        }
    }

    #[inline]
    pub fn pixel(&self, index: usize) -> PixelSurfaceState {
        PixelSurfaceState {
            current_height: self.current_height[index],
            abrasion: self.abrasion[index],
            graphite_mass: self.graphite_mass[index],
            clay_mass: self.clay_mass[index],
            wax_mass: self.wax_mass[index],
            loose_mass: self.loose_mass[index],
            compacted_mass: self.compacted_mass[index],
            orientation_x: self.orientation_x[index],
            orientation_y: self.orientation_y[index],
            color_r_mass: self.color_r_mass[index],
            color_g_mass: self.color_g_mass[index],
            color_b_mass: self.color_b_mass[index],
        }
    }

    #[inline]
    pub fn set_pixel(&mut self, index: usize, state: PixelSurfaceState) {
        self.current_height[index] = state.current_height;
        self.abrasion[index] = state.abrasion;
        self.graphite_mass[index] = state.graphite_mass;
        self.clay_mass[index] = state.clay_mass;
        self.wax_mass[index] = state.wax_mass;
        self.loose_mass[index] = state.loose_mass;
        self.compacted_mass[index] = state.compacted_mass;
        self.orientation_x[index] = state.orientation_x;
        self.orientation_y[index] = state.orientation_y;
        self.color_r_mass[index] = state.color_r_mass;
        self.color_g_mass[index] = state.color_g_mass;
        self.color_b_mass[index] = state.color_b_mass;
    }

    #[inline]
    pub fn deposit_pixel(&self, index: usize) -> PixelDepositState {
        PixelDepositState {
            graphite_mass: self.graphite_mass[index],
            clay_mass: self.clay_mass[index],
            wax_mass: self.wax_mass[index],
            loose_mass: self.loose_mass[index],
            compacted_mass: self.compacted_mass[index],
            orientation_x: self.orientation_x[index],
            orientation_y: self.orientation_y[index],
            color_r_mass: self.color_r_mass[index],
            color_g_mass: self.color_g_mass[index],
            color_b_mass: self.color_b_mass[index],
        }
    }

    pub fn set_deposit_pixel(&mut self, index: usize, state: PixelDepositState) {
        self.graphite_mass[index] = state.graphite_mass;
        self.clay_mass[index] = state.clay_mass;
        self.wax_mass[index] = state.wax_mass;
        self.loose_mass[index] = state.loose_mass;
        self.compacted_mass[index] = state.compacted_mass;
        self.orientation_x[index] = state.orientation_x;
        self.orientation_y[index] = state.orientation_y;
        self.color_r_mass[index] = state.color_r_mass;
        self.color_g_mass[index] = state.color_g_mass;
        self.color_b_mass[index] = state.color_b_mass;
    }

    pub fn total_deposit(&self, index: usize) -> f32 {
        self.graphite_mass[index] + self.clay_mass[index] + self.wax_mass[index]
    }

    pub fn local_capacity(&self, index: usize) -> f32 {
        let valley_depth = (1.0 - self.current_height[index]).clamp(0.0, 1.0);
        0.38 + 1.55 * valley_depth + 0.24 * self.fiber[index]
    }

    pub fn remaining_capacity(&self, index: usize) -> f32 {
        (self.local_capacity(index) - self.total_deposit(index)).max(0.0)
    }

    pub fn packed_fraction(&self, index: usize) -> f32 {
        let mechanical_total = self.loose_mass[index] + self.compacted_mass[index];
        if mechanical_total <= 1.0e-8 {
            0.0
        } else {
            (self.compacted_mass[index] / mechanical_total).clamp(0.0, 1.0)
        }
    }

    pub fn orientation_coherence(&self, index: usize) -> f32 {
        let total = self.total_deposit(index);
        if total <= 1.0e-8 {
            return 0.0;
        }
        let magnitude =
            (self.orientation_x[index].powi(2) + self.orientation_y[index].powi(2)).sqrt();
        (magnitude / total).clamp(0.0, 1.0)
    }

    pub fn clear_deposit(&mut self) {
        self.graphite_mass.fill(0.0);
        self.clay_mass.fill(0.0);
        self.wax_mass.fill(0.0);
        self.loose_mass.fill(0.0);
        self.compacted_mass.fill(0.0);
        self.orientation_x.fill(0.0);
        self.orientation_y.fill(0.0);
        self.color_r_mass.fill(0.0);
        self.color_g_mass.fill(0.0);
        self.color_b_mass.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_opacity_preserves_existing_pigment_and_interpolates_dense_coverage() {
        let before = PixelSurfaceState {
            graphite_mass: 0.4,
            loose_mass: 0.4,
            color_r_mass: 0.4,
            current_height: 0.65,
            ..Default::default()
        };
        let after = PixelSurfaceState {
            graphite_mass: 1.8,
            loose_mass: 1.2,
            compacted_mass: 0.6,
            color_r_mass: 0.4,
            color_b_mass: 1.4,
            current_height: 0.61,
            ..Default::default()
        };
        assert_eq!(before.with_edit_opacity(after, 0.), before);
        assert_eq!(before.with_edit_opacity(after, 1.), after);
        let half = before.with_edit_opacity(after, 0.5);
        let alpha = |p: PixelSurfaceState| -(-p.deposit().optical_depth()).exp_m1();
        assert!((alpha(half) - (alpha(before) + alpha(after)) * 0.5).abs() < 1e-6);
        let premult =
            |p: PixelSurfaceState| p.color_r_mass / p.deposit().total_deposit() * alpha(p);
        assert!((premult(half) - (premult(before) + premult(after)) * 0.5).abs() < 1e-6);
        assert_eq!(
            half.current_height,
            (before.current_height + after.current_height) * 0.5
        );
    }

    #[test]
    fn sparse_layer_allocates_only_touched_tiles() {
        let mut sparse = SparseDepositState::new(256, 256);
        let mut state = PixelDepositState::default();
        state.graphite_mass = 0.5;
        sparse.set(1, state);
        sparse.set(255 * 256 + 255, state);
        assert_eq!(sparse.allocated_tile_count(), 2);
        assert_eq!(sparse.get(1).graphite_mass, 0.5);
        assert_eq!(sparse.get(128 * 256 + 128).graphite_mass, 0.0);
    }
}
