use super::{
    contact::{evaluate_mesoscopic_contact, PencilTipContact, PreparedPaperContact},
    document::{DirtyRect, Document},
    history::EditTransaction,
    pencil::{EraserKind, PencilFormulation, PencilTipState, ToolKind, ToolSettings},
    smudge::{apply_smudge_dab, SmudgeReservoir},
};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_deg: f32,
    pub azimuth_deg: f32,
    pub rotation_deg: Option<f32>,
}

impl StrokePoint {
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            pressure: self.pressure + (other.pressure - self.pressure) * t,
            tilt_deg: self.tilt_deg + (other.tilt_deg - self.tilt_deg) * t,
            azimuth_deg: lerp_angle_degrees(self.azimuth_deg, other.azimuth_deg, t),
            rotation_deg: match (self.rotation_deg, other.rotation_deg) {
                (Some(a), Some(b)) => Some(lerp_angle_degrees(a, b, t)),
                (a, b) => {
                    if t < 0.5 {
                        a
                    } else {
                        b
                    }
                }
            },
        }
    }

    pub fn quadratic(start: Self, control: Self, end: Self, t: f32) -> Self {
        // De Casteljau interpolation keeps pressure/tilt/azimuth transitions synchronized with
        // the smoothed spatial path instead of smoothing position while stepping material state.
        start.lerp(control, t).lerp(control.lerp(end, t), t)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Default, Clone)]
pub struct StrokeEngine {
    smudge_reservoir: SmudgeReservoir,
    pencil_stroke_counter: u32,
    pencil_contact_seed: u32,
}

impl StrokeEngine {
    /// Begin a new pencil-contact realization. The seed changes once per stroke, not once per dab,
    /// so virtual asperities on the graphite face follow the motion and form coherent streaks.
    pub fn begin_pencil_stroke(&mut self, start: StrokePoint) {
        self.pencil_stroke_counter = self.pencil_stroke_counter.wrapping_add(1).max(1);
        self.pencil_contact_seed = stroke_seed(start, self.pencil_stroke_counter);
    }

    pub fn dab_spacing_px(
        document: &Document,
        settings: &ToolSettings,
        tip: &PencilTipState,
    ) -> f32 {
        let diameter_px = match settings.tool {
            ToolKind::Pencil => {
                tip.effective_core_diameter_px(document.spec.dpi)
                    * settings.pencil_geometry_scale
                    * settings.pressure_width_scale(0.)
            }
            ToolKind::Smudge => settings.smudge_size_px,
            ToolKind::Eraser => settings.eraser_diameter_mm * document.spec.dpi / 25.4,
            ToolKind::Brush | ToolKind::Shapes => settings.brush_size_px,
            ToolKind::Tissue => settings.tissue_size_px,
            ToolKind::Lasso | ToolKind::VectorSelect => 1.,
        };
        // The pencil has a fixed physical core rather than an arbitrary brush diameter. Keep
        // sampling dense enough for the narrow sharpened point and for long tilted facets.
        if matches!(
            settings.tool,
            ToolKind::Brush | ToolKind::Tissue | ToolKind::Shapes
        ) {
            (diameter_px * 0.08).clamp(0.25, 12.)
        } else {
            (diameter_px * 0.085).clamp(0.18, 1.35)
        }
    }

    pub fn apply_segment(
        &mut self,
        document: &mut Document,
        settings: &ToolSettings,
        tip: &mut PencilTipState,
        from: StrokePoint,
        to: StrokePoint,
        tx: &mut EditTransaction,
    ) -> Option<DirtyRect> {
        let distance = ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt();
        if distance <= 1.0e-6 {
            return None;
        }
        let spacing = Self::dab_spacing_px(document, settings, tip);
        let steps = (distance / spacing).ceil().max(1.0) as usize;
        let mut dirty: Option<DirtyRect> = None;

        let motion = {
            let dx = to.x - from.x;
            let dy = to.y - from.y;
            let len = (dx * dx + dy * dy).sqrt();
            (len > 1.0e-5).then_some((dx / len, dy / len))
        };

        let sliding_distance_px = distance / steps as f32;
        if super::stroke_gpu::should_batch(document, settings, tip) {
            let inputs: Vec<_> = (1..=steps)
                .map(|step| {
                    (
                        from.lerp(to, (step as f32 - 0.5) / steps as f32),
                        sliding_distance_px,
                        motion,
                    )
                })
                .collect();
            return self.apply_batched(document, settings, tip, &inputs, tx);
        }
        for step in 1..=steps {
            let t = (step as f32 - 0.5) / steps as f32;
            let point = from.lerp(to, t);
            if let Some(rect) = self.apply_dab_with_motion(
                document,
                settings,
                tip,
                point,
                tx,
                motion,
                sliding_distance_px,
            ) {
                dirty = Some(match dirty {
                    Some(existing) => existing.union(rect),
                    None => rect,
                });
            }
        }

        if let Some(rect) = dirty {
            document.mark_dirty(rect);
        }
        dirty
    }

    /// Rasterize a quadratic Bezier stroke segment. The UI feeds midpoint-smoothed pointer
    /// samples here, producing C1-continuous curves without changing the physical dab model.
    pub fn apply_quadratic_segment(
        &mut self,
        document: &mut Document,
        settings: &ToolSettings,
        tip: &mut PencilTipState,
        start: StrokePoint,
        control: StrokePoint,
        end: StrokePoint,
        tx: &mut EditTransaction,
    ) -> Option<DirtyRect> {
        let chord = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
        let control_polygon = ((control.x - start.x).powi(2) + (control.y - start.y).powi(2))
            .sqrt()
            + ((end.x - control.x).powi(2) + (end.y - control.y).powi(2)).sqrt();
        let approximate_length = (0.5 * chord + 0.5 * control_polygon).max(chord);
        if approximate_length <= 1.0e-5 {
            return None;
        }

        let spacing = Self::dab_spacing_px(document, settings, tip);
        let steps = (approximate_length / spacing).ceil().max(1.0) as usize;
        let mut dirty: Option<DirtyRect> = None;

        let batch = super::stroke_gpu::should_batch(document, settings, tip);
        let mut inputs = Vec::new();
        for step in 1..=steps {
            let t = (step as f32 - 0.5) / steps as f32;
            let point = StrokePoint::quadratic(start, control, end, t);
            let a = StrokePoint::quadratic(start, control, end, (step - 1) as f32 / steps as f32);
            let b = StrokePoint::quadratic(start, control, end, step as f32 / steps as f32);
            let sliding_distance_px = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();

            // Quadratic derivative supplies local drag direction to the smudger. Pencil azimuth is
            // still independently user/device controlled, so curve smoothing does not rotate the
            // simulated pencil behind the user's back.
            let omt = 1.0 - t;
            let dx = 2.0 * (omt * (control.x - start.x) + t * (end.x - control.x));
            let dy = 2.0 * (omt * (control.y - start.y) + t * (end.y - control.y));
            let len = (dx * dx + dy * dy).sqrt();
            let motion = (len > 1.0e-5).then_some((dx / len, dy / len));

            if batch {
                inputs.push((point, sliding_distance_px, motion));
                continue;
            }

            if let Some(rect) = self.apply_dab_with_motion(
                document,
                settings,
                tip,
                point,
                tx,
                motion,
                sliding_distance_px,
            ) {
                dirty = Some(match dirty {
                    Some(existing) => existing.union(rect),
                    None => rect,
                });
            }
        }

        if batch {
            return self.apply_batched(document, settings, tip, &inputs, tx);
        }
        if let Some(rect) = dirty {
            document.mark_dirty(rect);
        }
        dirty
    }

    pub fn apply_dab(
        &mut self,
        document: &mut Document,
        settings: &ToolSettings,
        tip: &mut PencilTipState,
        point: StrokePoint,
        tx: &mut EditTransaction,
    ) -> Option<DirtyRect> {
        if super::stroke_gpu::should_batch(document, settings, tip) {
            return self.apply_batched(
                document,
                settings,
                tip,
                &[(point, 0.18 * document.spec.dpi / 25.4, None)],
                tx,
            );
        }
        self.apply_dab_with_motion(
            document,
            settings,
            tip,
            point,
            tx,
            None,
            0.18 * document.spec.dpi / 25.4,
        )
    }

    fn apply_batched(
        &mut self,
        document: &mut Document,
        settings: &ToolSettings,
        tip: &mut PencilTipState,
        inputs: &[super::stroke_gpu::InputDab],
        tx: &mut EditTransaction,
    ) -> Option<DirtyRect> {
        let mut dirty: Option<DirtyRect> = None;
        for chunk in inputs.chunks(64) {
            if settings.tool == ToolKind::Pencil && self.pencil_contact_seed == 0 {
                if let Some(&(point, _, _)) = chunk.iter().find(|(p, _, _)| p.pressure > 1e-4) {
                    self.begin_pencil_stroke(point);
                }
            }
            if let Some(result) = super::stroke_gpu::try_apply(document, settings, tip, chunk, tx) {
                if let Some(rect) = result {
                    dirty = Some(dirty.map_or(rect, |r| r.union(rect)));
                }
            } else {
                for &(point, travel, motion) in chunk {
                    if let Some(rect) = self
                        .apply_dab_with_motion(document, settings, tip, point, tx, motion, travel)
                    {
                        dirty = Some(dirty.map_or(rect, |r| r.union(rect)));
                    }
                }
            }
        }
        if let Some(rect) = dirty {
            document.mark_dirty(rect);
        }
        dirty
    }

    fn apply_dab_with_motion(
        &mut self,
        document: &mut Document,
        settings: &ToolSettings,
        tip: &mut PencilTipState,
        point: StrokePoint,
        tx: &mut EditTransaction,
        motion: Option<(f32, f32)>,
        sliding_distance_px: f32,
    ) -> Option<DirtyRect> {
        let manual_rotation = tip.profile.orientation_deg;
        if settings.tool == ToolKind::Pencil {
            if let Some(rotation) = point.rotation_deg.filter(|a| a.is_finite()) {
                tip.profile.orientation_deg = rotation.rem_euclid(360.0);
            }
        }
        let result = self.apply_oriented_dab(
            document,
            settings,
            tip,
            point,
            tx,
            motion,
            sliding_distance_px,
        );
        tip.profile.orientation_deg = manual_rotation;
        result
    }

    fn apply_oriented_dab(
        &mut self,
        document: &mut Document,
        settings: &ToolSettings,
        tip: &mut PencilTipState,
        point: StrokePoint,
        tx: &mut EditTransaction,
        motion: Option<(f32, f32)>,
        sliding_distance_px: f32,
    ) -> Option<DirtyRect> {
        let pressure = point.pressure.clamp(0.0, 1.0);
        if pressure <= 1.0e-4
            || (settings.tool == ToolKind::Eraser && settings.eraser_strength <= 0.)
            || (settings.tool == ToolKind::Pencil && settings.flow <= 0.)
        {
            return None;
        }
        if matches!(
            settings.tool,
            ToolKind::Brush | ToolKind::Tissue | ToolKind::Shapes
        ) {
            return super::powder::apply(document, settings, point, sliding_distance_px, tx);
        }

        // Direct engine callers (including tests) may not use the UI's explicit begin hook.
        if settings.tool == ToolKind::Pencil && self.pencil_contact_seed == 0 {
            self.begin_pencil_stroke(point);
        }

        if settings.tool == ToolKind::Smudge {
            return apply_smudge_dab(
                document,
                tx,
                &mut self.smudge_reservoir,
                point.x,
                point.y,
                settings.smudge_size_px,
                pressure,
                settings.smudge_strength,
                motion,
            );
        }

        let tilt = if settings.tool == ToolKind::Pencil {
            (point.tilt_deg.clamp(0.0, 84.0) / 84.0).powf(1.20)
        } else {
            0.0
        };
        let formulation = settings.grade.formulation();
        let diameter_px = if settings.tool == ToolKind::Pencil {
            (tip.effective_core_diameter_px(document.spec.dpi)
                * settings.pencil_geometry_scale
                * settings.pressure_width_scale(pressure))
            .max(0.8)
        } else {
            (settings.eraser_diameter_mm * document.spec.dpi / 25.4).max(0.7)
        };

        let tip_contact = (settings.tool == ToolKind::Pencil).then(|| {
            PencilTipContact::from_state_with_sharpness(
                diameter_px,
                pressure,
                point.tilt_deg,
                point.azimuth_deg,
                formulation.core_hardness,
                settings.tip_sharpness,
            )
        });
        let eraser_radius = diameter_px * 0.5 * settings.eraser_kind.pressure_scale(pressure);
        let imported_contact = if settings.tool == ToolKind::Pencil && settings.pencil_tip_shape {
            settings.pencil_texture.as_ref().map(|texture| {
                super::brush::ImportedContact::new(
                    texture,
                    diameter_px,
                    pressure,
                    point.tilt_deg,
                    point.azimuth_deg + tip.profile.orientation_deg,
                    settings.tip_sharpness,
                )
            })
        } else {
            None
        };
        let reach = imported_contact
            .as_ref()
            .map(|c| c.reach())
            .or_else(|| tip_contact.map(PencilTipContact::reach_px))
            .unwrap_or(eraser_radius)
            .ceil() as i32
            + 2;
        let min_x = ((point.x.floor() as i32) - reach).max(0) as usize;
        let min_y = ((point.y.floor() as i32) - reach).max(0) as usize;
        let max_x = ((point.x.ceil() as i32) + reach + 1)
            .min(document.spec.width_px as i32)
            .max(0) as usize;
        let max_y = ((point.y.ceil() as i32) + reach + 1)
            .min(document.spec.height_px as i32)
            .max(0) as usize;

        if min_x >= max_x || min_y >= max_y {
            return None;
        }

        let mut changed_bounds: Option<DirtyRect> = None;
        let compliance = document.paper.compliance();
        let document_dpi = document.spec.dpi;
        let sliding_mm = sliding_distance_px * 25.4 / document_dpi;
        let eraser = PreparedEraser::new(
            pressure,
            settings.eraser_strength,
            sliding_mm,
            settings.eraser_kind,
        );
        if settings.tool == ToolKind::Pencil && sliding_mm <= 1.0e-8 {
            return None;
        }
        let prepared_contact = tip_contact.map(|geometry| geometry.prepare(&tip.profile, pressure));
        let texture_rotation = (-tip.profile.orientation_deg.to_radians()).sin_cos();
        let paper_contact =
            PreparedPaperContact::new(pressure, formulation.core_hardness, compliance);
        let transfer = PreparedGraphiteTransfer::new(
            pressure,
            tilt,
            motion
                .map(|(dx, dy)| dy.atan2(dx))
                .unwrap_or_else(|| tip_contact.map_or(0., |c| c.angle_rad)),
            settings.flow,
            sliding_mm,
            settings.pencil_color_rgb,
            formulation,
            document.paper,
        );
        // Reject empty 8x8 blocks before doing costly per-pixel tip sampling.
        // Keep the original row/pixel order for identical deposition and wear.
        let mut spans = Vec::with_capacity((max_x - min_x).div_ceil(8));

        for y in min_y..max_y {
            if (y - min_y) % 8 == 0 {
                spans.clear();
                if let Some(shape) = &imported_contact {
                    let bottom = (y + 8).min(max_y) - 1;
                    for left in (min_x..max_x).step_by(8) {
                        let right = (left + 8).min(max_x);
                        if shape.may_cover_box(
                            left as f32 + 0.5 - point.x,
                            y as f32 + 0.5 - point.y,
                            (right - 1) as f32 + 0.5 - point.x,
                            bottom as f32 + 0.5 - point.y,
                        ) {
                            spans.push((left, right));
                        }
                    }
                } else if let Some(geometry) = &prepared_contact {
                    let bottom = (y + 8).min(max_y) - 1;
                    for left in (min_x..max_x).step_by(8) {
                        let right = (left + 8).min(max_x);
                        if geometry.may_cover_box(
                            left as f32 + 0.5 - point.x,
                            y as f32 + 0.5 - point.y,
                            (right - 1) as f32 + 0.5 - point.x,
                            bottom as f32 + 0.5 - point.y,
                        ) {
                            spans.push((left, right));
                        }
                    }
                } else {
                    spans.push((min_x, max_x));
                }
            }
            for &(left, right) in &spans {
                for x in left..right {
                    let dx = (x as f32 + 0.5) - point.x;
                    let dy = (y as f32 + 0.5) - point.y;
                    let (
                        coverage,
                        footprint_weight,
                        tip_cross_coordinate,
                        tip_axial_coordinate,
                        profile_u,
                        profile_v,
                        profile_contact,
                    ) = if let Some(shape) = &imported_contact {
                        let sample = shape.sample_prepared(dx, dy, &tip.profile, texture_rotation);
                        if sample.coverage <= 1e-5 {
                            continue;
                        }
                        (
                            sample.coverage,
                            sample.pressure_weight,
                            sample.cross_coordinate_px,
                            sample.axial_coordinate_px,
                            sample.profile_u,
                            sample.profile_v,
                            sample.profile_contact,
                        )
                    } else if let Some(geometry) = &prepared_contact {
                        let tip_sample = geometry.sample_for_deposition(dx, dy, &tip.profile);
                        if tip_sample.coverage <= 1.0e-4 {
                            continue;
                        }
                        let index = document.index(x, y);
                        // Paper tooth perturbs only the last sub-pixel of the physical boundary. It is
                        // continuous, so the edge feathers into porous graphite instead of breaking
                        // into square white holes.
                        // The last paper-grain width is porous, not an unbroken digital
                        // contour. Grain changes contact reach as well as interior mass.
                        let edge_amplitude =
                            (0.10 * document_dpi / 25.4) * (1.15 - 0.35 * pressure);
                        let edge_jitter = (document.surface.edge_grain[index] as f32 / 255.0 - 0.5)
                            * edge_amplitude
                            * 3.0;
                        let edge_width = (0.045 * document_dpi / 25.4).max(0.65);
                        let porous_edge = smoothstep01(
                            0.5 - (tip_sample.signed_distance + edge_jitter) / edge_width,
                        );
                        let relief = settings.pencil_texture.as_ref().map_or(1., |texture| {
                            texture.contact_relief(
                                tip_sample.profile_u,
                                tip_sample.profile_v,
                                texture_rotation,
                            )
                        });
                        let footprint_weight = tip_sample.pressure_weight * relief;
                        if porous_edge * tip_sample.coverage <= 1.0e-5 {
                            continue;
                        }
                        (
                            porous_edge * tip_sample.coverage,
                            footprint_weight,
                            tip_sample.cross_coordinate_px,
                            tip_sample.axial_coordinate_px,
                            tip_sample.profile_u,
                            tip_sample.profile_v,
                            tip_sample.profile_contact * relief,
                        )
                    } else {
                        let distance = (dx * dx + dy * dy).sqrt();
                        let signed_distance = eraser_radius - distance;
                        let softness = match settings.eraser_kind {
                            EraserKind::Vinyl => 0.22,
                            EraserKind::Kneaded => 0.45,
                        };
                        let coverage = smoothstep01(
                            (signed_distance + 0.5) / (eraser_radius * softness).max(1.0),
                        );
                        if coverage <= 1.0e-4 {
                            continue;
                        }
                        (
                            coverage,
                            coverage
                                * (0.82
                                    + 0.18
                                        * (1.0 - distance / eraser_radius.max(0.1))
                                            .clamp(0.0, 1.0)),
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            1.0,
                        )
                    };
                    let _ = coverage;
                    let index = document.index(x, y);
                    if !document.selection.allows(index) {
                        continue;
                    }

                    let changed = match settings.tool {
                        ToolKind::Pencil => {
                            let macro_contact = paper_contact
                                .sample(document.surface.current_height[index], footprint_weight);
                            if macro_contact.normal_load <= 1.0e-6 {
                                false
                            } else {
                                let meso = evaluate_mesoscopic_contact(
                                    x,
                                    y,
                                    tip_cross_coordinate,
                                    tip_axial_coordinate,
                                    tilt,
                                    profile_contact,
                                    pressure,
                                    document.surface.contact_support[index] as f32 / 255.0,
                                    macro_contact,
                                    document_dpi,
                                    document.color_grain[index],
                                    settings.particle_variation,
                                );
                                if meso.contact_fraction <= 0.002 || meso.capture <= 0.002 {
                                    false
                                } else {
                                    tx.remember(index, document);
                                    let resolved_load = meso.normal_load;
                                    // Paper contact/capture becomes deposited material mass. The
                                    // footprint itself is kept mechanically flat enough that paper tooth,
                                    // not a radial opacity gradient, creates the visible graphite body.
                                    let capture = (meso.capture * coverage).clamp(0.0, 1.0);
                                    let changed = transfer.apply(
                                        document,
                                        index,
                                        capture,
                                        resolved_load,
                                        meso.particle_tone,
                                    );
                                    if changed {
                                        // Normalize both travel and sampled pixel area to 120 dpi.
                                        let dpi_scale = 120.0 / document.spec.dpi.max(1.0);
                                        let physical_slide =
                                            sliding_distance_px * dpi_scale.powi(3);
                                        tip.profile.register_contact_wear_prepared(
                                            profile_u,
                                            profile_v,
                                            resolved_load,
                                            physical_slide,
                                            formulation,
                                            texture_rotation,
                                        );
                                    }
                                    changed
                                }
                            }
                        }
                        ToolKind::Smudge => {
                            unreachable!("smudge is handled before footprint evaluation")
                        }
                        ToolKind::Brush
                        | ToolKind::Tissue
                        | ToolKind::Shapes
                        | ToolKind::Lasso
                        | ToolKind::VectorSelect => {
                            unreachable!()
                        }
                        ToolKind::Eraser => {
                            if document.surface.total_deposit(index) <= 0. {
                                false
                            } else {
                                tx.remember(index, document);
                                eraser.apply(document, index, footprint_weight)
                            }
                        }
                    };

                    if changed {
                        let pixel = DirtyRect::new(x, y, x + 1, y + 1);
                        changed_bounds = Some(changed_bounds.map_or(pixel, |r| r.union(pixel)));
                    }
                }
            }
        }

        changed_bounds
    }

    pub fn clear_smudger(&mut self) {
        self.smudge_reservoir.clear();
    }

    pub fn smudger_load_fraction(&self, diameter_px: f32) -> f32 {
        self.smudge_reservoir.load_fraction(diameter_px)
    }

    pub fn smudger_reservoir_mass(&self) -> f32 {
        self.smudge_reservoir.total_mass()
    }
}

struct PreparedGraphiteTransfer {
    formulation: PencilFormulation,
    sliding_mm: f32,
    wear_flow: f32,
    release_efficiency: f32,
    side_transfer: f32,
    capture_bias: f32,
    resistance: f32,
    compact_fraction: f32,
    direction: [f32; 2],
    base_color: [f32; 3],
}
impl PreparedGraphiteTransfer {
    fn new(
        pressure: f32,
        tilt: f32,
        orientation_rad: f32,
        flow: f32,
        sliding_mm: f32,
        color: [u8; 3],
        formulation: PencilFormulation,
        paper: super::paper::PaperPreset,
    ) -> Self {
        Self {
            formulation,
            sliding_mm,
            wear_flow: formulation.wear_coefficient * flow.clamp(0., 2.),
            release_efficiency: 0.72
                + 0.28 * super::contact::pencil_effective_pressure(pressure).powf(0.72),
            side_transfer: 1. + 0.36 * tilt * formulation.lubricity,
            capture_bias: paper.capture_bias(),
            resistance: paper.abrasion_resistance(),
            compact_fraction: 0.055 + 0.34 * pressure.powf(1.55),
            direction: [(orientation_rad * 2.).cos(), (orientation_rad * 2.).sin()],
            base_color: color.map(|c| c as f32 / 255.),
        }
    }
    fn apply(
        &self,
        document: &mut Document,
        index: usize,
        contact_fraction: f32,
        normal_load: f32,
        particle_tone: f32,
    ) -> bool {
        let formulation = self.formulation;
        let sliding_mm = self.sliding_mm;
        let fiber = document.surface.fiber[index];
        let existing = document.surface.total_deposit(index);

        // Local storage capacity grows in valleys. Low-pressure strokes still preferentially mark
        // peaks because contact mechanics decides which cells touch the core in the first place.
        let capacity = document.surface.local_capacity(index);
        let remaining = (capacity - existing).max(0.0);
        let fiber_capture = (0.70 + 0.38 * fiber) * self.capture_bias;

        // Pressure has already changed both cone contact geometry and local force. Material release
        // therefore needs only a mild efficiency term: ordinary force must already look like graphite,
        // while hard pressure gets denser because more real material is transferred over a larger
        // tapered contact -- not because the renderer switches to a darker paint color.
        let abrasion_work = self.wear_flow
            * normal_load.powf(0.82)
            * self.release_efficiency
            * contact_fraction
            * fiber_capture
            * self.side_transfer
            * sliding_mm
            * 0.72;
        // Integrate finite-capacity deposition exponentially rather than allowing a
        // nonzero saturation floor to accumulate unbounded dark paint.
        // exp_m1 retains tiny deposits near saturation instead of subtracting
        // two nearly equal floats and quantizing the contact threshold.
        let amount = remaining * -(-abrasion_work / capacity).exp_m1();

        if amount <= 2.0e-6 {
            return false;
        }

        document.surface.graphite_mass[index] += amount * formulation.graphite_fraction;
        document.surface.clay_mass[index] += amount * formulation.clay_fraction;
        document.surface.wax_mass[index] += amount * formulation.wax_fraction;
        let base_color = self.base_color;
        // Shade variation modulates value without injecting neutral gray into the pigment.
        // At zero variation (tone=1), the stored RGB is exactly the selected sRGB color.
        let tone = particle_tone.clamp(0.55, 1.30);
        let color = base_color.map(|c| (c * tone).clamp(0.0, 1.0));
        document.surface.color_r_mass[index] += amount * color[0];
        document.surface.color_g_mass[index] += amount * color[1];
        document.surface.color_b_mass[index] += amount * color[2];

        // Freshly transferred material is mostly mobile. Contact pressure packs a fraction at once,
        // and also drives some pre-existing loose material into a compacted film.
        let initially_compacted = amount * self.compact_fraction;
        document.surface.loose_mass[index] += amount - initially_compacted;
        document.surface.compacted_mass[index] += initially_compacted;

        let packing_rate = normal_load * sliding_mm / 0.17
            * (0.0030 + 0.0065 * formulation.core_hardness + 0.0025 * formulation.lubricity);
        let transferred_to_compact =
            document.surface.loose_mass[index] * packing_rate.clamp(0.0, 0.11);
        document.surface.loose_mass[index] -= transferred_to_compact;
        document.surface.compacted_mass[index] += transferred_to_compact;

        // Preserve an axial (not directional) history of material laydown. Parallel passes reinforce
        // coherence; cross-hatching reduces it through vector cancellation.
        document.surface.orientation_x[index] += amount * self.direction[0];
        document.surface.orientation_y[index] += amount * self.direction[1];

        // Raised paper grains progressively flatten under repeated loaded contact. We do not raise
        // valleys here: their apparent filling comes from deposited material, not paper restoration.
        let peak_height = (document.surface.current_height[index] - 0.5).max(0.0);
        if peak_height > 0.0 {
            let resistance = self.resistance;
            let smoothing = normal_load.powf(1.65) * sliding_mm / 0.17
                * (0.0007 + 0.0032 * formulation.core_hardness)
                * (1.25 - 0.55 * resistance);
            document.surface.current_height[index] =
                (document.surface.current_height[index] - peak_height * smoothing).clamp(0.0, 1.0);
        }

        let abrasion_gain = normal_load.powf(1.35) * sliding_mm / 0.17
            * (0.00018 + 0.00062 * formulation.core_hardness)
            * (1.20 - 0.65 * self.resistance);
        document.surface.abrasion[index] =
            (document.surface.abrasion[index] + abrasion_gain).clamp(0.0, 1.0);

        true
    }
}

struct PreparedEraser {
    pressure: f32,
    strength: f32,
    sliding_mm: f32,
    pressure_lift: f32,
    pressure_work: f32,
    loose_rate: f32,
    compact_rate: f32,
    wear: f32,
}
impl PreparedEraser {
    fn new(pressure: f32, strength: f32, sliding_mm: f32, kind: EraserKind) -> Self {
        let (loose_rate, compact_rate, wear) = match kind {
            EraserKind::Vinyl => (1.70, 0.24 * (0.45 + 0.55 * pressure), 1.),
            EraserKind::Kneaded => (1.15, 0.045, 0.03),
        };
        Self {
            pressure,
            strength: strength.clamp(0., 1.),
            sliding_mm: sliding_mm.max(0.),
            pressure_lift: pressure.powf(0.90),
            pressure_work: pressure.powf(1.45),
            loose_rate,
            compact_rate,
            wear,
        }
    }
    fn apply(&self, document: &mut Document, index: usize, footprint_weight: f32) -> bool {
        let total_before = document.surface.total_deposit(index);
        let strength = self.strength;
        let pressure = self.pressure;
        let sliding_mm = self.sliding_mm;
        if total_before <= 0. || strength <= 0. || pressure <= 0. || footprint_weight <= 0. {
            return false;
        }
        if strength >= 1. {
            // Full lift removes even compacted pigment, without painting white or
            // leaving new abrasion marks. Paper and other layers remain independent.
            document
                .surface
                .set_deposit_pixel(index, Default::default());
            return true;
        }

        // Raised material is easier to reach; valleys and compacted films resist lifting. A vinyl
        // eraser is represented here as removal/abrasion, not white paint.
        let accessibility = 0.40 + 0.60 * document.surface.current_height[index];
        let grain_contact = 0.84 + 0.16 * document.surface.contact_support[index] as f32 / 255.0;
        let base_lift = footprint_weight
            * self.pressure_lift
            * strength
            * accessibility
            * grain_contact
            * sliding_mm.max(0.0);

        let loose_before = document.surface.loose_mass[index];
        let compact_before = document.surface.compacted_mass[index];
        // Integrate rubbing distance exponentially, rather than removing a fixed amount
        // for every input packet. Loose graphite lifts first; burnished traces resist.
        let (loose_rate, compact_rate) = (self.loose_rate, self.compact_rate);
        let loose_removed = loose_before * (-(-base_lift * loose_rate).exp_m1());
        let compact_removed = compact_before * (-(-base_lift * compact_rate).exp_m1());
        let removed = (loose_removed + compact_removed).min(total_before);
        if removed <= 1.0e-8 {
            return false;
        }

        document.surface.loose_mass[index] = (loose_before - loose_removed).max(0.0);
        document.surface.compacted_mass[index] = (compact_before - compact_removed).max(0.0);

        let remaining_ratio = ((total_before - removed) / total_before).clamp(0.0, 1.0);
        document.surface.graphite_mass[index] *= remaining_ratio;
        document.surface.clay_mass[index] *= remaining_ratio;
        document.surface.wax_mass[index] *= remaining_ratio;
        document.surface.orientation_x[index] *= remaining_ratio;
        document.surface.orientation_y[index] *= remaining_ratio;
        document.surface.color_r_mass[index] *= remaining_ratio;
        document.surface.color_g_mass[index] *= remaining_ratio;
        document.surface.color_b_mass[index] *= remaining_ratio;

        // Hard erasing disturbs the support. It may flatten high fibers and permanently roughen the
        // local surface, so undo must include the paper channels as well as deposited material.
        let wear = self.wear;
        let eraser_work =
            footprint_weight * self.pressure_work * strength * sliding_mm.max(0.0) * wear;
        let peak_height = (document.surface.current_height[index] - 0.5).max(0.0);
        document.surface.current_height[index] = (document.surface.current_height[index]
            - peak_height * eraser_work * 0.0014)
            .clamp(0.0, 1.0);
        document.surface.abrasion[index] =
            (document.surface.abrasion[index] + eraser_work * 0.00075).clamp(0.0, 1.0);

        true
    }
}

fn stroke_seed(point: StrokePoint, counter: u32) -> u32 {
    let mut h = 0xB529_7A4D_u32 ^ counter.wrapping_mul(0x68E3_1DA4);
    h ^= point.x.to_bits().rotate_left(7);
    h ^= point.y.to_bits().rotate_left(17);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h.max(1)
}

fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp_angle_degrees(a: f32, b: f32, t: f32) -> f32 {
    let mut delta = (b - a) % 360.0;
    if delta > 180.0 {
        delta -= 360.0;
    }
    if delta < -180.0 {
        delta += 360.0;
    }
    (a + delta * t).rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        document::CanvasSpec,
        paper::{generate_builtin_albedo, PaperPreset, PaperTexturePreset},
        pencil::{PencilGrade, ToolKind},
    };

    fn new_test_document(width_mm: f32, height_mm: f32, dpi: f32, paper: PaperPreset) -> Document {
        let spec = CanvasSpec::from_physical("test", width_mm, height_mm, dpi);
        let albedo = generate_builtin_albedo(
            spec.width_px,
            spec.height_px,
            spec.dpi,
            PaperTexturePreset::White,
        );
        Document::new(spec, paper, "White", albedo)
    }

    fn dab_stats(grade: PencilGrade, pressure: f32, tilt_deg: f32) -> (usize, f32) {
        let mut document = new_test_document(30.0, 30.0, 120.0, PaperPreset::DrawingMedium);
        let mut settings = ToolSettings::default();
        settings.grade = grade;
        settings.pencil_core_diameter_mm = 2.0;
        settings.tilt_deg = tilt_deg;
        settings.azimuth_deg = 0.0;
        settings.auto_azimuth = false;
        settings.particle_variation = 0.03;
        let mut tip = PencilTipState::fresh_for_formulation(
            settings.pencil_core_diameter_mm,
            settings.grade.formulation(),
        );
        let point = StrokePoint {
            x: document.spec.width_px as f32 * 0.5,
            y: document.spec.height_px as f32 * 0.5,
            pressure,
            tilt_deg,
            azimuth_deg: 0.0,
            rotation_deg: None,
        };
        let mut tx = EditTransaction::default();
        let mut engine = StrokeEngine::default();
        engine.begin_pencil_stroke(point);
        let _ = engine.apply_dab(&mut document, &settings, &mut tip, point, &mut tx);
        let occupied = document
            .surface
            .graphite_mass
            .iter()
            .filter(|&&m| m > 1.0e-8)
            .count();
        let total = document.surface.graphite_mass.iter().sum::<f32>();
        (occupied, total)
    }

    #[test]
    fn imported_relief_uses_continuous_pencil_contact_and_preserves_legacy_replay() {
        use std::sync::Arc;
        let texture = Arc::new(super::super::brush::BrushTip {
            name: "Striated test".into(),
            width: 8,
            height: 8,
            mask: (0..64).map(|i| if i % 3 == 0 { 0 } else { 220 }).collect(),
        });
        let render = |tilt: f32, pressure: f32, custom: bool, legacy_mask: bool| {
            let mut doc = new_test_document(50., 35., 120., PaperPreset::DrawingMedium);
            let mut settings = ToolSettings::default();
            settings.pencil_tip_shape = false;
            settings.pencil_texture = custom.then(|| texture.clone());
            settings.brush_tip = legacy_mask.then(|| texture.clone());
            settings.pencil_color_rgb = [190, 40, 60];
            settings.particle_variation = 0.;
            let mut tip = PencilTipState::fresh(4.);
            let mut engine = StrokeEngine::default();
            let from = StrokePoint {
                x: 35.,
                y: 80.,
                pressure,
                tilt_deg: tilt,
                azimuth_deg: 90.,
                rotation_deg: Some(37.),
            };
            engine.apply_segment(
                &mut doc,
                &settings,
                &mut tip,
                from,
                StrokePoint { x: 180., ..from },
                &mut EditTransaction::default(),
            );
            doc
        };
        let standard = render(65., 0.6, false, false);
        let dormant = render(65., 0.6, false, true);
        assert_eq!(
            standard.surface.graphite_mass,
            dormant.surface.graphite_mass
        );
        let custom = render(65., 0.6, true, false);
        assert_ne!(custom.surface.graphite_mass, standard.surface.graphite_mass);
        for x in 45..170 {
            assert!(
                (0..custom.spec.height_px)
                    .any(|y| custom.surface.graphite_mass[custom.index(x, y)] > 1e-6),
                "stamp gap at {x}"
            );
        }
        let upright = render(0., 0.6, true, false);
        assert!(
            custom
                .surface
                .graphite_mass
                .iter()
                .filter(|&&m| m > 1e-6)
                .count()
                > upright
                    .surface
                    .graphite_mass
                    .iter()
                    .filter(|&&m| m > 1e-6)
                    .count()
                    * 2
        );
        let light = render(65., 0.1, true, false);
        assert!(
            custom.surface.graphite_mass.iter().sum::<f32>()
                > light.surface.graphite_mass.iter().sum::<f32>()
        );
        assert!(custom.surface.abrasion.iter().any(|&v| v > 0.));
        for i in 0..custom.spec.pixel_count() {
            let amount = custom.surface.total_deposit(i);
            if amount > 1e-6 {
                assert!((custom.surface.color_r_mass[i] / amount - 190. / 255.).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn tilted_stroke_dirty_bounds_cover_every_material_and_display_change() {
        use crate::render::{DocumentRenderer, RasterRenderer};
        for azimuth_deg in [0., 37., 113., 281.] {
            let mut doc = new_test_document(50., 50., 120., PaperPreset::DrawingMedium);
            let before: Vec<_> = (0..doc.spec.pixel_count())
                .map(|i| doc.surface.pixel(i))
                .collect();
            let mut renderer = RasterRenderer::default();
            let mut image = renderer.render_full(&doc);
            let mut engine = StrokeEngine::default();
            let mut tip = PencilTipState::fresh(5.);
            let settings = ToolSettings::default();
            let from = StrokePoint {
                x: 100.27,
                y: 90.63,
                pressure: 0.6,
                tilt_deg: 78.,
                azimuth_deg,
                rotation_deg: None,
            };
            let to = StrokePoint {
                x: 113.9,
                y: 99.2,
                pressure: 0.8,
                ..from
            };
            let rect = engine
                .apply_segment(
                    &mut doc,
                    &settings,
                    &mut tip,
                    from,
                    to,
                    &mut EditTransaction::default(),
                )
                .unwrap();
            for (i, &old) in before.iter().enumerate() {
                if doc.surface.pixel(i) != old {
                    let (x, y) = (i % doc.spec.width_px, i / doc.spec.width_px);
                    assert!(x >= rect.min_x && x < rect.max_x && y >= rect.min_y && y < rect.max_y);
                }
            }
            let patch = renderer.render_region(&doc, rect);
            for y in 0..rect.height() {
                for x in 0..rect.width() {
                    image[(rect.min_x + x, rect.min_y + y)] = patch[(x, y)];
                }
            }
            assert_eq!(image, RasterRenderer::default().render_full(&doc));
        }
    }

    #[test]
    fn soft_grade_deposits_more_graphite_than_hard_grade() {
        let (_, hard) = dab_stats(PencilGrade::H4, 0.55, 8.0);
        let (_, soft) = dab_stats(PencilGrade::B6, 0.55, 8.0);
        assert!(soft > hard);
    }

    #[test]
    fn ordinary_pressure_is_visible_and_hard_pressure_is_denser() {
        let (ordinary_n, ordinary_mass) = dab_stats(PencilGrade::HB, 0.26, 8.0);
        let (_, medium_mass) = dab_stats(PencilGrade::HB, 0.48, 8.0);
        let (_, hard_mass) = dab_stats(PencilGrade::HB, 0.90, 8.0);
        assert!(ordinary_n > 0 && ordinary_mass > 0.0);
        assert!(medium_mass > ordinary_mass * 1.25);
        assert!(hard_mass > medium_mass * 1.45);
    }

    #[test]
    fn tilt_not_core_size_is_the_primary_broad_mark_control() {
        let (upright_n, _) = dab_stats(PencilGrade::HB, 0.48, 8.0);
        let (side_n, _) = dab_stats(PencilGrade::HB, 0.48, 72.0);
        assert!(
            side_n > upright_n * 3,
            "laying the pencil over should expose a broad facet"
        );
    }

    #[test]
    fn pressure_does_not_turn_the_upright_point_into_a_marker() {
        let (light_n, _) = dab_stats(PencilGrade::HB, 0.20, 8.0);
        let (hard_n, _) = dab_stats(PencilGrade::HB, 0.90, 8.0);
        assert!(hard_n >= light_n);
        assert!(
            hard_n < light_n.max(1) * 6,
            "pressure width growth must stay bounded"
        );
    }

    #[test]
    fn repeated_side_passes_build_tone() {
        let mut document = new_test_document(32.0, 32.0, 120.0, PaperPreset::DrawingMedium);
        let mut settings = ToolSettings::default();
        settings.pencil_core_diameter_mm = 2.0;
        settings.tilt_deg = 65.0;
        settings.azimuth_deg = 0.0;
        settings.auto_azimuth = false;
        let mut tip = PencilTipState::fresh_for_formulation(
            settings.pencil_core_diameter_mm,
            settings.grade.formulation(),
        );
        let point = StrokePoint {
            x: document.spec.width_px as f32 * 0.5,
            y: document.spec.height_px as f32 * 0.5,
            pressure: 0.42,
            tilt_deg: settings.tilt_deg,
            azimuth_deg: 0.0,
            rotation_deg: None,
        };
        let mut engine = StrokeEngine::default();
        let mut tx = EditTransaction::default();
        engine.begin_pencil_stroke(point);
        let _ = engine.apply_dab(&mut document, &settings, &mut tip, point, &mut tx);
        let first = document.surface.graphite_mass.iter().sum::<f32>();
        for _ in 0..3 {
            let mut tx = EditTransaction::default();
            engine.begin_pencil_stroke(point);
            let _ = engine.apply_dab(&mut document, &settings, &mut tip, point, &mut tx);
        }
        let layered = document.surface.graphite_mass.iter().sum::<f32>();
        assert!(layered > first * 1.35);
    }

    #[test]
    fn pencil_color_is_stored_with_material_without_changing_grade_mechanics() {
        let mut document = new_test_document(20.0, 20.0, 120.0, PaperPreset::DrawingMedium);
        let mut settings = ToolSettings::default();
        settings.pencil_color_rgb = [180, 40, 30];
        let mut tip = PencilTipState::fresh(settings.pencil_core_diameter_mm);
        let point = StrokePoint {
            x: document.spec.width_px as f32 * 0.5,
            y: document.spec.height_px as f32 * 0.5,
            pressure: 0.7,
            tilt_deg: 8.0,
            azimuth_deg: 0.0,
            rotation_deg: None,
        };
        let mut tx = EditTransaction::default();
        let _ =
            StrokeEngine::default().apply_dab(&mut document, &settings, &mut tip, point, &mut tx);
        let r: f32 = document.surface.color_r_mass.iter().sum();
        let b: f32 = document.surface.color_b_mass.iter().sum();
        assert!(r > b * 3.0);
    }

    #[test]
    fn eraser_does_not_deposit_graphite() {
        let mut document = new_test_document(20.0, 20.0, 120.0, PaperPreset::DrawingMedium);
        let mut settings = ToolSettings::default();
        settings.tool = ToolKind::Eraser;
        let mut tip = PencilTipState::default();
        let point = StrokePoint {
            x: 40.0,
            y: 40.0,
            pressure: 0.8,
            tilt_deg: 0.0,
            azimuth_deg: 0.0,
            rotation_deg: None,
        };
        let mut tx = EditTransaction::default();
        let _ =
            StrokeEngine::default().apply_dab(&mut document, &settings, &mut tip, point, &mut tx);
        assert_eq!(document.surface.graphite_mass.iter().sum::<f32>(), 0.0);
    }

    #[test]
    fn eraser_zero_is_noop_and_full_lift_removes_all_pigment_with_undo() {
        use crate::core::history::History;
        for kind in [EraserKind::Vinyl, EraserKind::Kneaded] {
            let mut doc = new_test_document(10., 10., 120., PaperPreset::DrawingMedium);
            let i = doc.index(20, 20);
            doc.surface.graphite_mass[i] = 0.7;
            doc.surface.clay_mass[i] = 0.25;
            doc.surface.wax_mass[i] = 0.05;
            doc.surface.loose_mass[i] = 0.1;
            doc.surface.compacted_mass[i] = 0.9;
            doc.surface.color_r_mass[i] = 0.8;
            doc.surface.color_g_mass[i] = 0.1;
            doc.surface.color_b_mass[i] = 0.2;
            doc.surface.orientation_x[i] = 0.3;
            doc.surface.orientation_y[i] = 0.6;
            let original = doc.surface.pixel(i);
            let mut settings = ToolSettings {
                tool: ToolKind::Eraser,
                eraser_kind: kind,
                eraser_strength: 0.,
                ..Default::default()
            };
            let point = StrokePoint {
                x: 20.5,
                y: 20.5,
                pressure: 0.3,
                tilt_deg: 0.,
                azimuth_deg: 0.,
                rotation_deg: None,
            };
            let mut tx = EditTransaction::default();
            let mut engine = StrokeEngine::default();
            let mut tip = PencilTipState::default();
            assert!(engine
                .apply_dab(&mut doc, &settings, &mut tip, point, &mut tx)
                .is_none());
            assert_eq!(doc.surface.pixel(i), original);
            assert!(tx.is_empty());
            settings.eraser_strength = 1.;
            assert!(engine
                .apply_dab(&mut doc, &settings, &mut tip, point, &mut tx)
                .is_some());
            assert_eq!(doc.surface.deposit_pixel(i), Default::default());
            assert_eq!(doc.surface.current_height[i], original.current_height);
            assert_eq!(doc.surface.abrasion[i], original.abrasion);
            let mut history = History::default();
            history.push(tx, &doc);
            assert!(history.undo(&mut doc));
            assert_eq!(doc.surface.pixel(i), original);
            assert!(history.redo(&mut doc));
            assert_eq!(doc.surface.deposit_pixel(i), Default::default());
        }
    }

    #[test]
    fn eraser_lifts_loose_material_first_and_undo_restores_color_and_paper() {
        use crate::core::history::History;
        let mut doc = new_test_document(10., 10., 120., PaperPreset::DrawingMedium);
        let i = doc.index(20, 20);
        doc.surface.graphite_mass[i] = 1.0;
        doc.surface.loose_mass[i] = 0.5;
        doc.surface.compacted_mass[i] = 0.5;
        doc.surface.color_r_mass[i] = 0.6;
        doc.surface.color_g_mass[i] = 0.2;
        doc.surface.color_b_mass[i] = 0.1;
        let original = doc.surface.pixel(i);
        let mut tx = EditTransaction::default();
        tx.remember(i, &doc);
        assert!(PreparedEraser::new(0.7,0.8,1.,EraserKind::Kneaded).apply(&mut doc,i,1.));
        assert!(doc.surface.loose_mass[i] < doc.surface.compacted_mass[i]);
        assert!(doc.surface.graphite_mass[i] > 0.5);
        assert!((doc.surface.color_r_mass[i] / doc.surface.graphite_mass[i] - 0.6).abs() < 0.00001);
        let after = doc.surface.pixel(i);
        let mut history = History::default();
        history.push(tx, &doc);
        assert!(history.undo(&mut doc));
        assert_eq!(doc.surface.pixel(i), original);
        assert!(history.redo(&mut doc));
        assert_eq!(doc.surface.pixel(i), after);
    }

    #[test]
    fn eraser_rubbing_depends_on_distance_and_type_instead_of_packet_count() {
        let run = |steps: usize, kind: EraserKind| {
            let mut doc = new_test_document(10., 10., 120., PaperPreset::DrawingMedium);
            let i = doc.index(20, 20);
            doc.surface.graphite_mass[i] = 1.;
            doc.surface.loose_mass[i] = 0.5;
            doc.surface.compacted_mass[i] = 0.5;
            for _ in 0..steps {
                PreparedEraser::new(0.7,0.8,2./steps as f32,kind).apply(&mut doc,i,1.);
            }
            (doc.surface.graphite_mass[i], doc.surface.compacted_mass[i])
        };
        let coarse = run(1, EraserKind::Vinyl);
        let fine = run(120, EraserKind::Vinyl);
        assert!((coarse.0 - fine.0).abs() < 0.0003);
        assert!(fine.0 < run(120, EraserKind::Kneaded).0);
        assert!(fine.1 < run(120, EraserKind::Kneaded).1);
    }

    #[test]
    fn short_stroke_profile_wear_is_deferred_and_small() {
        let mut document = new_test_document(28.0, 20.0, 120.0, PaperPreset::RoughSketch);
        let settings = ToolSettings::default();
        let mut tip = PencilTipState::fresh_for_formulation(
            settings.pencil_core_diameter_mm,
            settings.grade.formulation(),
        );
        let initial_core = tip.core_diameter_mm;
        let initial_peak = tip.profile.peak_height();
        let y = document.spec.height_px as f32 * 0.5;
        let from = StrokePoint {
            x: 8.0,
            y,
            pressure: 0.9,
            tilt_deg: 30.0,
            azimuth_deg: 0.0,
            rotation_deg: None,
        };
        let to = StrokePoint {
            x: document.spec.width_px as f32 - 8.0,
            ..from
        };
        let mut tx = EditTransaction::default();
        let mut engine = StrokeEngine::default();
        engine.begin_pencil_stroke(from);
        let _ = engine.apply_segment(&mut document, &settings, &mut tip, from, to, &mut tx);
        assert_eq!(tip.wear_work, 0.0);
        assert!((tip.profile.peak_height() - initial_peak).abs() < 1.0e-6);
        assert!((tip.core_diameter_mm - initial_core).abs() < 1.0e-6);
        assert!(tip.commit_pending_wear());
        assert!(tip.wear_work > 0.0);
        assert_eq!(tip.core_diameter_mm, initial_core);
    }

    #[test]
    fn quadratic_interpolation_keeps_pressure_and_position_smooth() {
        let start = StrokePoint {
            x: 0.0,
            y: 0.0,
            pressure: 0.2,
            tilt_deg: 0.0,
            azimuth_deg: 0.0,
            rotation_deg: None,
        };
        let control = StrokePoint {
            x: 5.0,
            y: 8.0,
            pressure: 0.5,
            ..start
        };
        let end = StrokePoint {
            x: 10.0,
            y: 0.0,
            pressure: 0.8,
            ..start
        };
        let mid = StrokePoint::quadratic(start, control, end, 0.5);
        assert!((mid.x - 5.0).abs() < 1.0e-5);
        assert!(mid.y > 3.9);
        assert!(mid.pressure > 0.45 && mid.pressure < 0.55);
    }

    #[test]
    fn barrel_rotation_uses_short_arc_and_preserves_manual_tip_angle() {
        let point = StrokePoint {
            x: 40.,
            y: 40.,
            pressure: 0.6,
            tilt_deg: 65.,
            azimuth_deg: 45.,
            rotation_deg: Some(359.),
        };
        let mid = point.lerp(
            StrokePoint {
                rotation_deg: Some(1.),
                ..point
            },
            0.5,
        );
        assert!(mid.rotation_deg.unwrap().abs() < 0.0001);
        assert_eq!(mid.azimuth_deg, 45.);
        let draw = |rotation| {
            let mut doc = new_test_document(20., 20., 120., PaperPreset::DrawingMedium);
            let mut tip = PencilTipState::default();
            tip.profile.orientation_deg = 33.;
            let mut engine = StrokeEngine::default();
            engine.apply_dab(
                &mut doc,
                &ToolSettings::default(),
                &mut tip,
                StrokePoint {
                    rotation_deg: Some(rotation),
                    ..point
                },
                &mut EditTransaction::default(),
            );
            assert_eq!(tip.profile.orientation_deg, 33.);
            doc.surface.graphite_mass
        };
        assert_ne!(draw(0.), draw(90.));
    }

    #[test]
    fn zero_shade_variation_stores_selected_color_without_gray_mixing() {
        let mut doc = new_test_document(20., 20., 120., PaperPreset::DrawingMedium);
        let settings = ToolSettings {
            pencil_color_rgb: [240, 0, 75],
            particle_variation: 0.,
            ..Default::default()
        };
        let mut engine = StrokeEngine::default();
        let point = StrokePoint {
            x: 40.,
            y: 40.,
            pressure: 0.8,
            tilt_deg: 65.,
            azimuth_deg: 0.,
            rotation_deg: None,
        };
        engine.apply_dab(
            &mut doc,
            &settings,
            &mut PencilTipState::default(),
            point,
            &mut EditTransaction::default(),
        );
        let mut count = 0;
        for i in 0..doc.spec.pixel_count() {
            let mass = doc.surface.total_deposit(i);
            if mass > 0.00001 {
                count += 1;
                assert_eq!(doc.surface.color_g_mass[i], 0.);
                assert!((doc.surface.color_r_mass[i] / mass - 240. / 255.).abs() < 0.000001);
                assert!((doc.surface.color_b_mass[i] / mass - 75. / 255.).abs() < 0.000001);
            }
        }
        assert!(count > 0);
    }
}
