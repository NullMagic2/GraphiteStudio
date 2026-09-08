#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Pencil,
    Smudge,
    Eraser,
    Brush,
    Tissue,
    Shapes,
    Lasso,
    VectorSelect,
}

impl ToolKind {
    pub const PALETTE: [Self; 7] = [
        Self::Pencil,
        Self::Tissue,
        Self::Smudge,
        Self::Eraser,
        Self::Shapes,
        Self::VectorSelect,
        Self::Lasso,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Pencil => "Pencil",
            Self::Smudge => "Smudge",
            Self::Eraser => "Eraser",
            Self::Brush => "Custom pencil",
            Self::Tissue => "Tissue",
            Self::Shapes => "Shapes",
            Self::Lasso => "Free selection",
            Self::VectorSelect => "Vector selection",
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PencilGrade {
    H4,
    H2,
    H,
    HB,
    B,
    B2,
    B4,
    B6,
    B8,
}

impl PencilGrade {
    pub const ALL: [Self; 9] = [
        Self::H4,
        Self::H2,
        Self::H,
        Self::HB,
        Self::B,
        Self::B2,
        Self::B4,
        Self::B6,
        Self::B8,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::H4 => "4H",
            Self::H2 => "2H",
            Self::H => "H",
            Self::HB => "HB",
            Self::B => "B",
            Self::B2 => "2B",
            Self::B4 => "4B",
            Self::B6 => "6B",
            Self::B8 => "8B",
        }
    }

    pub fn formulation(self) -> PencilFormulation {
        // Generic, research-informed formulation family. Graphite/clay/wax fractions are
        // deliberately treated as approximations rather than a universal grade standard:
        // commercial grade recipes differ between manufacturers.
        match self {
            Self::H4 => PencilFormulation::new(0.55, 0.40, 0.05, 0.96, 0.42, 0.52, 0.90),
            Self::H2 => PencilFormulation::new(0.60, 0.35, 0.05, 0.84, 0.52, 0.57, 0.82),
            Self::H => PencilFormulation::new(0.64, 0.31, 0.05, 0.72, 0.61, 0.61, 0.74),
            Self::HB => PencilFormulation::new(0.68, 0.27, 0.05, 0.58, 0.73, 0.66, 0.64),
            Self::B => PencilFormulation::new(0.71, 0.24, 0.05, 0.47, 0.83, 0.70, 0.56),
            Self::B2 => PencilFormulation::new(0.74, 0.21, 0.05, 0.36, 0.95, 0.74, 0.48),
            Self::B4 => PencilFormulation::new(0.79, 0.16, 0.05, 0.23, 1.12, 0.80, 0.37),
            Self::B6 => PencilFormulation::new(0.84, 0.11, 0.05, 0.13, 1.28, 0.85, 0.29),
            Self::B8 => PencilFormulation::new(0.90, 0.05, 0.05, 0.07, 1.43, 0.90, 0.23),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PencilFormulation {
    pub graphite_fraction: f32,
    pub clay_fraction: f32,
    pub wax_fraction: f32,
    /// 1 = hard, 0 = very soft.
    pub core_hardness: f32,
    /// Relative core-wear/material-release coefficient.
    pub wear_coefficient: f32,
    /// Relative lubricity; currently influences side-contact transfer and compaction.
    pub lubricity: f32,
    /// Relative resistance to point flattening.
    pub point_retention: f32,
}

impl PencilFormulation {
    const fn new(
        graphite_fraction: f32,
        clay_fraction: f32,
        wax_fraction: f32,
        core_hardness: f32,
        wear_coefficient: f32,
        lubricity: f32,
        point_retention: f32,
    ) -> Self {
        Self {
            graphite_fraction,
            clay_fraction,
            wax_fraction,
            core_hardness,
            wear_coefficient,
            lubricity,
            point_retention,
        }
    }

    pub fn total_fraction(self) -> f32 {
        self.graphite_fraction + self.clay_fraction + self.wax_fraction
    }
}

/// Persistent normalized graphite-tip surface. `height` is the protrusion of the core toward the
/// paper: 1.0 is locally highest/most exposed and lower values are recessed by wear. The map is
/// stored in the pencil's own frame, so changing `orientation_deg` exposes different edges/facets
/// without erasing the wear history.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TipProfile {
    pub resolution: u8,
    pub height: Vec<f32>,
    pub hardness_bias: f32,
    pub orientation_deg: f32,
    pub(crate) pending_wear: Vec<f32>,
    peak_height: f32,
}

impl TipProfile {
    pub const DEFAULT_RESOLUTION: u8 = 32;

    pub fn fresh_default(hardness_bias: f32) -> Self {
        Self::fresh(Self::DEFAULT_RESOLUTION, hardness_bias)
    }

    pub fn fresh(resolution: u8, hardness_bias: f32) -> Self {
        let resolution = resolution.clamp(12, 64);
        let n = resolution as usize;
        let mut height = vec![0.0; n * n];

        // A sharpened point is a rounded cone with a subtle, deterministic faceting term. The
        // irregularity is intentionally small: persistent wear, not random brush noise, should be
        // responsible for the larger streak/facet structure that develops during drawing.
        for y in 0..n {
            for x in 0..n {
                let u = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                height[y * n + x] = Self::fresh_height_unrotated(u, v);
            }
        }

        let peak_height = height.iter().copied().fold(0.0, f32::max).max(1.0e-4);
        Self {
            resolution,
            pending_wear: vec![0.0; n * n],
            height,
            hardness_bias: hardness_bias.clamp(0.0, 1.0),
            orientation_deg: 0.0,
            peak_height,
        }
    }

    pub fn rotate_by(&mut self, delta_deg: f32) {
        self.orientation_deg = (self.orientation_deg + delta_deg).rem_euclid(360.0);
    }

    pub fn set_orientation(&mut self, orientation_deg: f32) {
        self.orientation_deg = orientation_deg.rem_euclid(360.0);
    }

    pub fn peak_height(&self) -> f32 {
        self.peak_height
    }

    /// Small persistent silhouette irregularity of the exposed graphite face. Fresh sharpening
    /// contributes restrained faceting; local wear recesses the corresponding edge. The returned
    /// scale intentionally stays close to 1 so the point remains believable rather than becoming
    /// a decorative noisy brush outline.
    pub fn edge_radius_scale(&self, paper_angle_rad: f32) -> f32 {
        let local_angle = paper_angle_rad - self.orientation_deg.to_radians();
        let fresh_facet = 0.024 * (5.0 * local_angle + 0.35).cos()
            + 0.015 * (8.0 * local_angle - 0.90).cos()
            + 0.009 * (11.0 * local_angle + 1.70).cos();
        let sample_radius = 0.86;
        let u = paper_angle_rad.cos() * sample_radius;
        let v = paper_angle_rad.sin() * sample_radius;
        let edge_wear = self.wear_recession(u, v);
        (1.0 + fresh_facet - 0.085 * edge_wear).clamp(0.90, 1.06)
    }

    fn fresh_height_unrotated(u: f32, v: f32) -> f32 {
        let r = (u * u + v * v).sqrt();
        if r > 1.0 {
            return 0.0;
        }
        let angle = v.atan2(u);
        let facet = 0.018 * (6.0 * angle).cos() + 0.010 * (11.0 * angle + 0.7).cos();
        let cone = (1.0 - r.powf(1.34)).clamp(0.0, 1.0);
        (cone * (0.985 + facet)).clamp(0.0, 1.0)
    }

    /// Height of the untouched sharpened profile at this paper-facing coordinate. This is kept
    /// separate from the current height so contact can react to *wear recession* without applying
    /// the cone shape twice: the analytic paper-space envelope already represents the fresh cone.
    pub fn reference_height(&self, u: f32, v: f32) -> f32 {
        let (u, v) = self.rotate_coordinates(u, v);
        Self::fresh_height_unrotated(u, v)
    }

    /// Local recession from the sharpened reference surface. A fresh tip reports zero everywhere,
    /// including the anti-aliased fringe outside the normalized core disk. This prevents the
    /// persistent profile from deleting otherwise valid sub-pixel sharpened-apex contacts.
    pub fn wear_recession(&self, u: f32, v: f32) -> f32 {
        let (u, v) = self.rotate_coordinates(u, v);
        if u * u + v * v > 1.0 {
            return 0.0;
        }
        let fresh = Self::fresh_height_unrotated(u, v);
        let current = self.sample_unrotated(u, v);
        (fresh - current).max(0.0).clamp(0.0, 1.0)
    }

    /// Bilinear profile sample in normalized pencil-local coordinates. Outside the exposed core
    /// disk there is no graphite surface to contact.
    pub fn sample_height(&self, u: f32, v: f32) -> f32 {
        self.sample_height_prepared(u, v, (-self.orientation_deg.to_radians()).sin_cos())
    }
    pub(crate) fn sample_height_prepared(&self, u: f32, v: f32, rotation: (f32, f32)) -> f32 {
        if u * u + v * v > 1.04 {
            return 0.0;
        }
        let (sin, cos) = rotation;
        let (u, v) = (u * cos - v * sin, u * sin + v * cos);
        self.sample_unrotated(u, v)
    }

    fn rotate_coordinates(&self, u: f32, v: f32) -> (f32, f32) {
        // Rotating the physical pencil by +theta means a paper-space query sees the profile at
        // -theta in the stored pencil frame.
        let angle = -self.orientation_deg.to_radians();
        let (sin_a, cos_a) = angle.sin_cos();
        (u * cos_a - v * sin_a, u * sin_a + v * cos_a)
    }

    fn sample_unrotated(&self, u: f32, v: f32) -> f32 {
        let n = self.resolution as usize;
        let fx = ((u * 0.5 + 0.5) * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
        let fy = ((v * 0.5 + 0.5) * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(n - 1);
        let y1 = (y0 + 1).min(n - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let a = self.height[y0 * n + x0];
        let b = self.height[y0 * n + x1];
        let c = self.height[y1 * n + x0];
        let d = self.height[y1 * n + x1];
        let top = a + (b - a) * tx;
        let bottom = c + (d - c) * tx;
        top + (bottom - top) * ty
    }

    /// Accumulate abrasion into the exact profile region that made paper contact. Wear is deferred
    /// until stroke end so one continuous line keeps stable geometry, while separate strokes still
    /// evolve the physical point throughout a drawing session.
    pub fn register_contact_wear(
        &mut self,
        paper_u: f32,
        paper_v: f32,
        local_contact_load: f32,
        sliding_distance_px: f32,
        formulation: PencilFormulation,
    ) -> f32 {
        self.register_contact_wear_prepared(
            paper_u,
            paper_v,
            local_contact_load,
            sliding_distance_px,
            formulation,
            (-self.orientation_deg.to_radians()).sin_cos(),
        )
    }
    pub(crate) fn register_contact_wear_prepared(
        &mut self,
        paper_u: f32,
        paper_v: f32,
        local_contact_load: f32,
        sliding_distance_px: f32,
        formulation: PencilFormulation,
        rotation: (f32, f32),
    ) -> f32 {
        if local_contact_load <= 0.0 || sliding_distance_px <= 0.0 {
            return 0.0;
        }

        let (sin, cos) = rotation;
        let (u, v) = (paper_u * cos - paper_v * sin, paper_u * sin + paper_v * cos);
        if u * u + v * v > 1.08 {
            return 0.0;
        }

        let softness = 1.0 - formulation.core_hardness;
        let retention = formulation.point_retention.clamp(0.0, 1.0);
        let wear_delta = local_contact_load.powf(1.08)
            * sliding_distance_px.max(0.0)
            * formulation.wear_coefficient
            * (0.42 + 0.78 * softness)
            * (1.12 - 0.64 * retention)
            * 0.00000035;
        if wear_delta <= 1.0e-10 {
            return 0.0;
        }

        let n = self.resolution as usize;
        let fx = ((u * 0.5 + 0.5) * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
        let fy = ((v * 0.5 + 0.5) * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(n - 1);
        let y1 = (y0 + 1).min(n - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let weights = [
            (y0 * n + x0, (1.0 - tx) * (1.0 - ty)),
            (y0 * n + x1, tx * (1.0 - ty)),
            (y1 * n + x0, (1.0 - tx) * ty),
            (y1 * n + x1, tx * ty),
        ];
        for (index, weight) in weights {
            self.pending_wear[index] += wear_delta * weight;
        }
        wear_delta
    }

    pub fn commit_pending_wear(&mut self) -> f32 {
        let pending_total: f32 = self.pending_wear.iter().sum();
        if pending_total <= 1.0e-10 {
            self.clear_pending_wear();
            return 0.0;
        }

        let n = self.resolution as usize;
        // Smooth only abrasion, conserving its amount. Fixed height smoothing after each pen
        // lift used to change the tip even under tiny loads and could regrow recessed graphite.
        let mut abrasion = vec![0.0; self.height.len()];
        for y in 0..n {
            for x in 0..n {
                let index = y * n + x;
                let amount = self.pending_wear[index];
                abrasion[index] += amount * 0.8;
                for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx >= 0 && ny >= 0 && nx < n as isize && ny < n as isize {
                        abrasion[ny as usize * n + nx as usize] += amount * 0.05;
                    } else {
                        abrasion[index] += amount * 0.05;
                    }
                }
            }
        }
        let mut removed = 0.;
        for (height, wear) in self.height.iter_mut().zip(abrasion) {
            let loss = wear.min(*height);
            *height -= loss;
            removed += loss;
        }
        self.peak_height = self.height.iter().copied().fold(0.0, f32::max).max(1.0e-5);
        self.clear_pending_wear();
        removed
    }

    pub fn clear_pending_wear(&mut self) {
        self.pending_wear.fill(0.0);
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct PencilTipState {
    pub core_diameter_mm: f32,
    /// Cumulative committed dimensionless profile wear work. This is diagnostic/calibration state;
    /// geometry comes from `profile.height`, not from a scalar flatness value.
    pub wear_work: f32,
    pub profile: TipProfile,
}

impl PencilTipState {
    pub const MAX_CORE_DIAMETER_MM: f32 = 200. * 25.4 / 36.;
    pub fn fresh(core_diameter_mm: f32) -> Self {
        Self {
            core_diameter_mm: core_diameter_mm.clamp(1.0, Self::MAX_CORE_DIAMETER_MM),
            wear_work: 0.0,
            profile: TipProfile::fresh_default(0.5),
        }
    }

    pub fn fresh_for_formulation(core_diameter_mm: f32, formulation: PencilFormulation) -> Self {
        Self {
            core_diameter_mm: core_diameter_mm.clamp(1.0, Self::MAX_CORE_DIAMETER_MM),
            wear_work: 0.0,
            profile: TipProfile::fresh_default(formulation.core_hardness),
        }
    }

    pub fn sharpen(&mut self, core_diameter_mm: f32, formulation: PencilFormulation) {
        let orientation = self.profile.orientation_deg;
        *self = Self::fresh_for_formulation(core_diameter_mm, formulation);
        self.profile.set_orientation(orientation);
    }

    pub fn sync_core_diameter(&mut self, diameter_mm: f32) {
        self.core_diameter_mm = diameter_mm.clamp(1.0, Self::MAX_CORE_DIAMETER_MM);
    }

    /// Wear fraction is deliberately diagnostic/spacing-only. The contact shape itself is sampled
    /// from the persistent 2-D tip profile, so rotating the point can reveal sharp unworn facets.
    pub fn wear_fraction(&self) -> f32 {
        (1.0 - (-self.wear_work * 2.0).exp()).clamp(0.0, 0.98)
    }

    pub fn effective_core_diameter_px(&self, dpi: f32) -> f32 {
        self.core_diameter_mm.clamp(1.0, Self::MAX_CORE_DIAMETER_MM) * dpi.max(36.0) / 25.4
    }

    pub fn register_profile_wear(
        &mut self,
        profile_u: f32,
        profile_v: f32,
        local_contact_load: f32,
        sliding_distance_px: f32,
        formulation: PencilFormulation,
    ) {
        let _ = self.profile.register_contact_wear(
            profile_u,
            profile_v,
            local_contact_load,
            sliding_distance_px,
            formulation,
        );
    }

    pub fn commit_pending_wear(&mut self) -> bool {
        let committed = self.profile.commit_pending_wear();
        if committed <= 0.0 {
            return false;
        }
        self.wear_work += committed;
        true
    }

    pub fn clear_pending_wear(&mut self) {
        self.profile.clear_pending_wear();
    }
}

impl Default for PencilTipState {
    fn default() -> Self {
        Self::fresh(2.0)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ToolSettings {
    /// Gallery identity only; it has no effect on stroke rendering.
    #[serde(default)]
    pub pencil_id: String,
    /// 0 = blunt, 0.5 = standard, 1 = fine. Older files retain the standard geometry.
    #[serde(default = "default_tip_sharpness")]
    pub tip_sharpness: f32,
    /// Additional width dynamics. Missing values preserve legacy vector replay.
    #[serde(default)]
    pub pressure_width: f32,
    pub use_wintab: bool,
    pub eraser_kind: EraserKind,
    pub tool: ToolKind,
    pub grade: PencilGrade,
    /// Physical graphite-core diameter in millimeters. This is not brush size: an upright sharpened
    /// point touches only a small fraction of the core, while tilt progressively exposes the side.
    pub pencil_core_diameter_mm: f32,
    pub pencil_geometry_scale: f32,
    /// Smudger / blending-stump contact diameter in document pixels.
    pub smudge_size_px: f32,
    /// Material transport strength for the smudger.
    pub smudge_strength: f32,
    /// Eraser contact diameter in millimeters.
    pub eraser_diameter_mm: f32,
    /// 0 = upright, 80 = almost laying the pencil on its side. Used as the manual fallback when
    /// a live stylus backend does not provide tilt.
    pub tilt_deg: f32,
    /// Direction of the side-of-pencil footprint. Used as the manual fallback when auto-azimuth
    /// is disabled or there is not yet enough stroke motion to infer a direction.
    pub azimuth_deg: f32,
    /// When true, pencil side-footprint azimuth follows stroke direction for stylus/mouse input
    /// whenever no live device orientation packet is available.
    pub auto_azimuth: bool,
    /// Mouse fallback pressure. A tablet backend can replace this per-sample.
    pub mouse_pressure: f32,
    /// Device-pressure response only: below one is lighter touch, above one is firmer.
    pub pen_pressure_gamma: f32,
    /// Material-release multiplier, intentionally separate from pressure.
    pub flow: f32,
    /// Optical pencil body color in sRGB. Grade mechanics remain independent from color.
    pub pencil_color_rgb: [u8; 3],
    /// Restrained grayscale/value variation between mesoscopic graphite particles.
    /// This never creates white particles; it only breaks up digitally uniform pencil tone.
    pub particle_variation: f32,
    pub eraser_strength: f32,
    /// Optional pencil position stabilization, from off (0) to maximum (1).
    pub line_smoothing: f32,
    #[serde(default = "hold_to_straighten_default")]
    pub hold_to_straighten: bool,
    pub brush_tip: Option<std::sync::Arc<super::brush::BrushTip>>,
    /// Imported relief for the physical pencil solver. Kept separate from the
    /// legacy stamp mask so previously saved Pencil paths retain their appearance.
    #[serde(default)]
    pub pencil_texture: Option<std::sync::Arc<super::brush::BrushTip>>,
    /// Older editable strokes keep their relief-only contact when replayed.
    #[serde(default)]
    pub pencil_tip_shape: bool,
    pub brush_size_px: f32,
    pub brush_angle_deg: f32,
    pub tissue_size_px: f32,
    pub tissue_load: f32,
    /// Spatial graphite-density variation; zero preserves legacy tissue strokes.
    #[serde(default)]
    pub tissue_random_graphite: f32,
    pub shape: super::shapes::ShapeKind,
    #[serde(default)]
    pub shape_line_snap: bool,
    #[serde(default)]
    pub transform_keep_aspect: bool,
    pub shape_width_px: f32,
    /// Opacity of a completed geometric outline, independent of material flow and layer opacity.
    #[serde(default = "full_shape_opacity")]
    pub shape_opacity: f32,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            pencil_id: String::new(),
            tip_sharpness: default_tip_sharpness(),
            pressure_width: 0.,
            tool: ToolKind::Pencil,
            use_wintab: false,
            eraser_kind: EraserKind::Vinyl,
            grade: PencilGrade::HB,
            pencil_core_diameter_mm: 2.0,
            pencil_geometry_scale: 1.0,
            smudge_size_px: 18.0,
            smudge_strength: 0.82,
            eraser_diameter_mm: 5.0,
            tilt_deg: 8.0,
            azimuth_deg: 20.0,
            auto_azimuth: true,
            mouse_pressure: 0.48,
            pen_pressure_gamma: 1.0,
            flow: 1.0,
            pencil_color_rgb: [104, 104, 104],
            particle_variation: 0.24,
            eraser_strength: 0.78,
            line_smoothing: 0.,
            hold_to_straighten: true,
            brush_tip: None,
            pencil_texture: None,
            pencil_tip_shape: true,
            brush_size_px: 40.,
            brush_angle_deg: 0.,
            tissue_size_px: 100.,
            tissue_load: 0.35,
            tissue_random_graphite: 0.,
            shape: super::shapes::ShapeKind::Line,
            shape_line_snap: false,
            transform_keep_aspect: false,
            shape_width_px: 3.,
            shape_opacity: 1.,
        }
    }
}

fn default_tip_sharpness() -> f32 {
    0.5
}

fn hold_to_straighten_default() -> bool {
    true
}

impl ToolSettings {
    pub fn snap_shape(&self, shift: bool) -> bool {
        shift || (self.shape == super::shapes::ShapeKind::Line && self.shape_line_snap)
    }

    /// Modulates contact size without remapping material force or pigment color.
    /// At full force the original footprint is available for broad shading.
    pub fn pressure_width_scale(&self, pressure: f32) -> f32 {
        let fine = 0.22 + 0.78 * pressure.clamp(0., 1.).powf(1.15);
        1. - self.pressure_width.clamp(0., 1.) * (1. - fine)
    }
}

fn full_shape_opacity() -> f32 {
    1.
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraserKind {
    Vinyl,
    Kneaded,
}
impl EraserKind {
    pub fn pressure_scale(self, pressure: f32) -> f32 {
        let p = pressure.clamp(0.0, 1.0).sqrt();
        match self {
            Self::Vinyl => 0.82 + 0.18 * p,
            Self::Kneaded => 0.60 + 0.40 * p,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wear_is_proportional_to_work_not_pen_lifts_and_never_regrows() {
        let base = TipProfile::fresh_default(0.5);
        let mut together = base.clone();
        let mut split = base.clone();
        for _ in 0..20 {
            together.register_contact_wear(0.2, 0.1, 0.8, 200., PencilGrade::HB.formulation());
            split.register_contact_wear(0.2, 0.1, 0.8, 200., PencilGrade::HB.formulation());
            split.commit_pending_wear();
        }
        together.commit_pending_wear();
        for ((&a, &b), &original) in together.height.iter().zip(&split.height).zip(&base.height) {
            assert!((a - b).abs() < 2e-6);
            assert!(a <= original && b <= original);
        }
        let mut light = base.clone();
        light.register_contact_wear(0.2, 0.1, 0.8, 0.01, PencilGrade::HB.formulation());
        light.commit_pending_wear();
        assert!(light
            .height
            .iter()
            .zip(base.height)
            .all(|(&a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn writing_width_is_dynamic_saved_and_legacy_paths_keep_original_geometry() {
        let mut settings = ToolSettings::default();
        assert_eq!(settings.pressure_width_scale(0.2), 1.);
        settings.pressure_width = 0.85;
        settings.tip_sharpness = 0.85;
        assert!(settings.pressure_width_scale(0.15) < 0.45);
        assert!(settings.pressure_width_scale(0.7) > settings.pressure_width_scale(0.15) * 1.7);
        assert_eq!(settings.pressure_width_scale(1.), 1.);
        let mut saved = serde_json::to_value(&settings).unwrap();
        let loaded: ToolSettings = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(loaded.pressure_width, settings.pressure_width);
        assert_eq!(loaded.tip_sharpness, settings.tip_sharpness);
        saved.as_object_mut().unwrap().remove("pressure_width");
        let old: ToolSettings = serde_json::from_value(saved).unwrap();
        assert_eq!(old.pressure_width_scale(0.2), 1.);
    }

    #[test]
    fn sharpness_is_saved_and_old_settings_keep_standard_tip() {
        let mut settings = ToolSettings::default();
        settings.tip_sharpness = 0.87;
        let mut json = serde_json::to_value(settings).unwrap();
        assert_eq!(
            serde_json::from_value::<ToolSettings>(json.clone())
                .unwrap()
                .tip_sharpness,
            0.87
        );
        json.as_object_mut().unwrap().remove("tip_sharpness");
        assert_eq!(
            serde_json::from_value::<ToolSettings>(json)
                .unwrap()
                .tip_sharpness,
            0.5
        );
    }

    #[test]
    fn old_shape_settings_default_to_full_opacity() {
        let mut settings = ToolSettings::default();
        settings.shape_opacity = 0.37;
        let mut json = serde_json::to_value(settings).unwrap();
        assert_eq!(
            serde_json::from_value::<ToolSettings>(json.clone())
                .unwrap()
                .shape_opacity,
            0.37
        );
        json.as_object_mut().unwrap().remove("shape_opacity");
        assert_eq!(
            serde_json::from_value::<ToolSettings>(json)
                .unwrap()
                .shape_opacity,
            1.
        );
    }

    #[test]
    fn old_settings_keep_uniform_tissue_and_unconstrained_shapes_and_transforms() {
        let mut json = serde_json::to_value(ToolSettings::default()).unwrap();
        for field in ["tissue_random_graphite", "shape_line_snap", "transform_keep_aspect"] {
            json.as_object_mut().unwrap().remove(field);
        }
        let old: ToolSettings = serde_json::from_value(json).unwrap();
        assert_eq!(old.tissue_random_graphite, 0.);
        assert!(!old.shape_line_snap);
        assert!(!old.transform_keep_aspect);
    }

    #[test]
    fn formulations_are_normalized() {
        for grade in PencilGrade::ALL {
            let f = grade.formulation();
            assert!((f.total_fraction() - 1.0).abs() < 1.0e-6);
        }
    }

    #[test]
    fn soft_core_releases_more_material() {
        assert!(
            PencilGrade::B6.formulation().wear_coefficient
                > PencilGrade::H4.formulation().wear_coefficient
        );
    }

    #[test]
    fn fresh_tip_profile_has_a_central_peak() {
        let profile = TipProfile::fresh_default(0.5);
        let center = profile.sample_height(0.0, 0.0);
        let shoulder = profile.sample_height(0.72, 0.0);
        assert!(center > shoulder);
        assert!(profile.peak_height() > 0.90);
    }

    #[test]
    fn rotating_profile_changes_anisotropic_wear_sampling() {
        let mut profile = TipProfile::fresh_default(0.4);
        let f = PencilGrade::B4.formulation();
        for _ in 0..200 {
            profile.register_contact_wear(0.55, 0.05, 0.9, 1.0, f);
        }
        assert!(profile.commit_pending_wear() > 0.0);
        let worn = profile.sample_height(0.55, 0.05);
        profile.rotate_by(90.0);
        let rotated = profile.sample_height(0.55, 0.05);
        assert!((worn - rotated).abs() > 1.0e-4);
    }
}
