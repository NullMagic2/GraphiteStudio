use std::f32::consts::PI;

use super::pencil::TipProfile;

/// Paper-space contact produced by a physically bounded graphite core. An upright sharpened point
/// touches only a small fraction of the core diameter; broad marks appear when tilt exposes a long,
/// directional side facet. The persistent fine-scale wear surface lives in `TipProfile`.
#[derive(Debug, Clone, Copy)]
pub struct PencilTipContact {
    /// Transverse radius of the sharpened apex contact. Unlike a mechanical-pencil nib, this grows
    /// substantially as normal force pushes the tapered graphite farther into the paper tooth.
    pub point_radius_px: f32,
    /// Axial radius of the apex contact. Even before true side contact, tilt makes the tapered point
    /// slightly elliptical rather than an invariant circular nib.
    pub point_axial_radius_px: f32,
    /// Maximum half-width of the exposed graphite side facet.
    pub cross_radius_px: f32,
    /// Extra one-sided length extending behind the stylus tip along the pencil azimuth.
    pub rear_extension_px: f32,
    pub angle_rad: f32,
    pub side_fraction: f32,
    /// Approximate physical contact area in document pixels².
    pub area_px2: f32,
    /// Local normal-load scale after distributing one stylus force over the exposed contact.
    pub force_density_scale: f32,
    /// Pressure after the pencil's force/contact response curve. Kept here so footprint geometry,
    /// paper penetration and material release all respond to the same physical load signal.
    pub effective_pressure: f32,
    /// Physical scale of the irregular contacting grains, in document pixels.
    pub grain_radius_px: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TipContactSample {
    pub coverage: f32,
    pub pressure_weight: f32,
    pub signed_distance: f32,
    /// Coordinate transverse to pencil azimuth.
    pub cross_coordinate_px: f32,
    /// Coordinate along pencil azimuth.
    pub axial_coordinate_px: f32,
    /// Normalized coordinates on the persistent tip profile that produced this paper contact.
    pub profile_u: f32,
    pub profile_v: f32,
    /// Fraction of the local tip surface that is close enough to the current contact plane.
    pub profile_contact: f32,
}

/// Quantities shared by every paper pixel under a single dab. The profile's
/// committed heights stay fixed during the dab; abrasion accumulates separately.
pub(crate) struct PreparedTipContact {
    geometry: PencilTipContact,
    sin_a: f32,
    cos_a: f32,
    sin_r: f32,
    cos_r: f32,
    front_x: f32,
    rear_x: f32,
    center_x: f32,
    half_length: f32,
    axial_power: f32,
    cross_power: f32,
    grain_scale: f32,
    wear_allowance: f32,
}

/// Convert raw tablet force to the force actually used by the graphite solver.
///
/// A wooden pencil should already make a useful mark at ordinary hand pressure. The old v0.12
/// mapping preserved too much of the tablet's low numerical range and then deliberately capped
/// point-width growth, which made the tool feel like a uniform mechanical pencil. This curve is
/// monotonic, reaches zero exactly, and expands the low/mid range without sacrificing a hard-force
/// ceiling.
pub fn pencil_effective_pressure(pressure: f32) -> f32 {
    let p = pressure.clamp(0.0, 1.0);
    if p <= 0.0 {
        return 0.0;
    }
    let lifted = p.powf(0.58);
    let shoulder = smoothstep01((p - 0.18) / 0.82);
    (0.88 * lifted + 0.12 * shoulder).clamp(0.0, 1.0)
}

impl PencilTipContact {
    #[allow(clippy::too_many_arguments)]
    pub fn from_state(
        core_diameter_px: f32,
        pressure: f32,
        tilt_deg: f32,
        azimuth_deg: f32,
        core_hardness: f32,
    ) -> Self {
        Self::from_state_with_sharpness(
            core_diameter_px,
            pressure,
            tilt_deg,
            azimuth_deg,
            core_hardness,
            0.5,
        )
    }

    pub fn from_state_with_sharpness(
        core_diameter_px: f32,
        pressure: f32,
        tilt_deg: f32,
        azimuth_deg: f32,
        core_hardness: f32,
        sharpness: f32,
    ) -> Self {
        let p = pressure.clamp(0.0, 1.0);
        let load = pencil_effective_pressure(p);
        let hardness = core_hardness.clamp(0.0, 1.0);
        let softness = 1.0 - hardness;
        let core_radius = (core_diameter_px.max(0.8) * 0.5).max(0.40);

        // Pressure now advances the paper/contact plane down the sharpened cone. That means a
        // tapered wooden pencil gains *real contact width* with force instead of keeping an almost
        // invariant nib and changing only darkness. Even at full force the apex remains bounded by
        // the physical core radius, so it cannot become an arbitrary brush.
        // Apex preparation is independent of grade, core size and the broad side facet.
        let sharpness = sharpness.clamp(0., 1.);
        let apex_scale = if sharpness <= 0.5 {
            1.0 + (0.5 - sharpness) * 1.1
        } else {
            1.0 - (sharpness - 0.5) * 0.9
        };
        let point_fraction = (0.035 + 0.600 * load.powf(0.82)) * apex_scale;
        let point_radius_px = (core_radius * point_fraction * (1.0 + 0.10 * softness))
            .clamp(0.15, core_radius * 0.70);

        // An oblique cone intersects the paper as a slightly elongated contact before the pencil is
        // tilted far enough to expose a true side face. This removes the mechanical-pencil feeling
        // of a perfectly circular nib at every ordinary drawing angle.
        let tilt_rad = tilt_deg.clamp(0.0, 84.0).to_radians();
        let obliqueness = tilt_rad.sin().clamp(0.0, 1.0);
        let point_axial_radius_px = (point_radius_px * (1.0 + 0.46 * obliqueness.powf(1.35)))
            .min(core_radius * 0.82)
            .max(point_radius_px);

        // True side contact starts only after the tapered point has become clearly oblique. It is
        // still tilt-driven, but pressure can expose a little more of the physical graphite face.
        let side = smoothstep01((tilt_deg.clamp(0.0, 84.0) - 18.0) / 60.0).powf(1.08);
        let side_radius = core_radius * (0.68 + 0.27 * load.powf(0.52)) * (1.0 + 0.055 * softness);
        let cross_radius_px =
            lerp(point_radius_px, side_radius, side.powf(0.70)).min(core_radius * 1.04);

        // A side stroke is a long face trailing behind the tip. Pressure changes its exposed length
        // moderately because a loaded pencil settles deeper into both graphite and paper, but tilt
        // remains the dominant cause of broad shading.
        let rear_extension_px = core_diameter_px.max(0.8)
            * side
            * (0.62 + 2.25 * side)
            * (0.76 + 0.24 * load.powf(0.45))
            * (1.03 - 0.07 * hardness);

        let point_area = PI * point_radius_px * point_axial_radius_px;
        let facet_area = rear_extension_px * (point_radius_px + cross_radius_px) * 1.48;
        let area_px2 = (point_area + facet_area).max(point_area);

        // Equal hand force over a broad side face gives lower force density, but the floor prevents
        // the side from becoming a ghost. The important change is that upright force is no longer
        // used to suppress pressure-driven cone growth.
        let raw_density = (point_area / area_px2.max(point_area)).powf(0.42);
        let force_density_scale = lerp(1.0, raw_density.max(0.38), side).clamp(0.38, 1.0);

        Self {
            point_radius_px,
            point_axial_radius_px,
            cross_radius_px,
            rear_extension_px,
            angle_rad: azimuth_deg.rem_euclid(360.0) / 180.0 * PI,
            side_fraction: side,
            area_px2,
            force_density_scale,
            effective_pressure: load,
            grain_radius_px: core_diameter_px * 0.085,
        }
    }

    /// Sample a pressure-grown tapered apex transitioning into a one-sided side facet.
    ///
    /// The important distinction from a brush is that pressure moves the contact plane down a
    /// finite cone; tilt exposes a graphite face; neither operation scales a circular stamp.
    pub fn sample(self, dx: f32, dy: f32, profile: &TipProfile, pressure: f32) -> TipContactSample {
        self.prepare(profile, pressure)
            .sample(dx, dy, profile, false)
    }

    pub(crate) fn prepare(self, profile: &TipProfile, pressure: f32) -> PreparedTipContact {
        let (sin_a, cos_a) = self.angle_rad.sin_cos();
        let front_x = self.point_axial_radius_px;
        let rear_x = -self.point_axial_radius_px - self.rear_extension_px;
        let center_x = 0.5 * (front_x + rear_x);
        let half_length = (0.5 * (front_x - rear_x)).max(self.point_axial_radius_px);
        let point_power = 2.0 + 0.25 * self.effective_pressure;
        let (sin_r, cos_r) = profile.orientation_deg.to_radians().sin_cos();
        PreparedTipContact {
            geometry: self,
            sin_a,
            cos_a,
            sin_r,
            cos_r,
            front_x,
            rear_x,
            center_x,
            half_length,
            axial_power: lerp(point_power, 5.0, self.side_fraction),
            cross_power: lerp(point_power, 2.25, self.side_fraction),
            grain_scale: self.grain_radius_px.max(0.35),
            wear_allowance: 0.018
                + 0.145 * pencil_effective_pressure(pressure)
                + 0.060 * self.side_fraction,
        }
    }

    pub fn signed_distance(self, dx: f32, dy: f32, profile: &TipProfile, pressure: f32) -> f32 {
        self.sample(dx, dy, profile, pressure).signed_distance
    }

    pub fn reach_px(self) -> f32 {
        self.rear_extension_px
            + self
                .cross_radius_px
                .max(self.point_axial_radius_px)
                .max(self.point_radius_px)
            + 2.0
    }
}

impl PreparedTipContact {
    /// Conservative rejection of an entire block of pixel centers. Extremes
    /// bound the full rotated block, including the irregular edge and AA fringe.
    pub(crate) fn may_cover_box(&self, min_dx: f32, min_dy: f32, max_dx: f32, max_dy: f32) -> bool {
        let corners = [
            (min_dx, min_dy),
            (max_dx, min_dy),
            (min_dx, max_dy),
            (max_dx, max_dy),
        ];
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (dx, dy) in corners {
            let x = dx * self.cos_a + dy * self.sin_a;
            let y = -dx * self.sin_a + dy * self.cos_a;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        let abs_min = |lo: f32, hi: f32| {
            if lo <= 0. && hi >= 0. {
                0.
            } else {
                lo.abs().min(hi.abs())
            }
        };
        let half_width = |x: f32| {
            let axial01 =
                ((self.front_x - x) / (self.front_x - self.rear_x).max(0.1)).clamp(0., 1.);
            lerp(
                self.geometry.point_radius_px,
                self.geometry.cross_radius_px,
                self.geometry.side_fraction * smoothstep01(axial01 / 0.72),
            )
        };
        let width_a = half_width(min_x);
        let width_b = half_width(max_x);
        let min_width = (width_a.min(width_b) * 0.90).max(0.10);
        let max_width = (width_a.max(width_b) * 1.06).max(0.10);
        let nx_min = (abs_min(min_x - self.center_x, max_x - self.center_x)
            / self.half_length.max(0.12))
        .clamp(0., 3.);
        let ny_min = (abs_min(min_y, max_y) / max_width).clamp(0., 3.);
        let metric_min = nx_min.powf(self.axial_power) + ny_min.powf(self.cross_power);
        if metric_min <= 1. {
            return true;
        }
        let nx_max = ((min_x - self.center_x)
            .abs()
            .max((max_x - self.center_x).abs())
            / self.half_length.max(0.12))
        .clamp(0., 3.);
        let ny_max = (min_y.abs().max(max_y.abs()) / min_width).clamp(0., 3.);
        let gx_max =
            self.axial_power * nx_max.powf(self.axial_power - 1.) / self.half_length.max(0.12);
        let gy_max = self.cross_power * ny_max.powf(self.cross_power - 1.) / min_width;
        let gradient_max = (gx_max * gx_max + gy_max * gy_max).sqrt().max(0.01);
        metric_min - 1. <= (0.5 + self.grain_scale * 1.2) * gradient_max + 0.0001
    }

    pub(crate) fn sample_for_deposition(
        &self,
        dx: f32,
        dy: f32,
        profile: &TipProfile,
    ) -> TipContactSample {
        self.sample(dx, dy, profile, true)
    }

    fn sample(
        &self,
        dx: f32,
        dy: f32,
        profile: &TipProfile,
        reject_empty: bool,
    ) -> TipContactSample {
        let geometry = self.geometry;
        let local_x = dx * self.cos_a + dy * self.sin_a;
        let local_y = -dx * self.sin_a + dy * self.cos_a;
        let (front_x, rear_x, center_x, half_length) =
            (self.front_x, self.rear_x, self.center_x, self.half_length);
        let axial01 = ((front_x - local_x) / (front_x - rear_x).max(0.1)).clamp(0.0, 1.0);

        let side_growth = smoothstep01(axial01 / 0.72);
        let local_half_width = lerp(
            geometry.point_radius_px,
            geometry.cross_radius_px,
            geometry.side_fraction * side_growth,
        );
        let nx = ((local_x - center_x).abs() / half_length.max(0.12)).clamp(0.0, 3.0);
        let axial_power = self.axial_power;
        let cross_power = self.cross_power;
        let axial_metric = nx.powf(axial_power);
        let gx = axial_power * nx.powf(axial_power - 1.0) / half_length.max(0.12);
        let grain_scale = self.grain_scale;
        if reject_empty {
            // edge_radius_scale is bounded to [0.90, 1.06]. Use its widest
            // contour for a lower metric bound and narrowest for an upper
            // gradient bound. Reject only when even the most favorable grain
            // offset cannot reach the AA fringe. No texture samples are dropped.
            let ny_min = (local_y.abs() / (local_half_width * 1.06).max(0.10)).clamp(0., 3.);
            let metric_min = axial_metric + ny_min.powf(cross_power);
            if metric_min > 1. {
                let min_width = (local_half_width * 0.90).max(0.10);
                let ny_max = (local_y.abs() / min_width).clamp(0., 3.);
                let gy_max = cross_power * ny_max.powf(cross_power - 1.) / min_width;
                let gradient_max = (gx * gx + gy_max * gy_max).sqrt().max(0.01);
                let fringe = 0.5 + grain_scale * 1.2;
                if metric_min - 1. > fringe * gradient_max + 0.0001 {
                    return TipContactSample::default();
                }
            }
        }
        let boundary_angle = (local_y / local_half_width.max(0.10))
            .atan2((local_x - center_x) / half_length.max(0.10));
        let edge_scale = profile.edge_radius_scale(boundary_angle);
        let ny = (local_y.abs() / (local_half_width * edge_scale).max(0.10)).clamp(0.0, 3.0);

        // A loaded sharpened point develops a small flattened/faceted contact rather than remaining
        // a perfect circle. The exponent rises smoothly with load, while a true side facet becomes
        // more trapezoidal along the pencil axis.
        let metric = axial_metric + ny.powf(cross_power);
        // Convert the implicit contour to a distance using its local gradient.
        // Scaling by the short radius feathered the ends of long facets over many
        // pixels. The gradient gives approximately one pixel of edge AA in any direction.
        let gy =
            cross_power * ny.powf(cross_power - 1.0) / (local_half_width * edge_scale).max(0.10);
        let gradient = (gx * gx + gy * gy).sqrt().max(0.01);
        let smooth_distance = (metric - 1.0) / gradient;
        // An exposed graphite face is a collection of asperities. Their persistent
        // topography changes both the contact silhouette and local pressure. It is
        // not a circular mask with random opacity applied afterward.
        let gu = (local_x * self.cos_r - local_y * self.sin_r) / grain_scale;
        let gv = (local_x * self.sin_r + local_y * self.cos_r) / grain_scale;
        let grain = contact_grain(gu, gv);
        let signed_distance = smooth_distance + (grain - 0.5) * grain_scale * 2.4;
        let envelope_coverage = smoothstep01(0.5 - signed_distance);

        let profile_u = (local_y / local_half_width.max(0.10)).clamp(-1.15, 1.15);
        let profile_v = ((local_x - center_x) / half_length.max(0.10)).clamp(-1.15, 1.15);

        if envelope_coverage <= 1.0e-5 {
            return TipContactSample {
                coverage: 0.0,
                pressure_weight: 0.0,
                signed_distance,
                cross_coordinate_px: local_y,
                axial_coordinate_px: local_x,
                profile_u,
                profile_v,
                profile_contact: 0.0,
            };
        }

        // Pressure also determines how much recession can still touch the paper, so pushing harder
        // can re-engage worn low regions of a real tip rather than simply multiplying opacity.
        let recession = profile.wear_recession(profile_u, profile_v);
        let wear_allowance = self.wear_allowance;
        let wear_contact = if recession <= 1.0e-6 {
            1.0
        } else {
            smoothstep01((wear_allowance - recession + 0.020) / 0.055)
        };
        let reference_height = profile.reference_height(profile_u, profile_v);
        let facet_support = 0.90 + 0.10 * reference_height.sqrt();
        let profile_contact = wear_contact * facet_support;

        if wear_contact <= 1.0e-5 {
            return TipContactSample {
                coverage: 0.0,
                pressure_weight: 0.0,
                signed_distance,
                cross_coordinate_px: local_y,
                axial_coordinate_px: local_x,
                profile_u,
                profile_v,
                profile_contact,
            };
        }

        // Mechanical load stays fairly even over the actual touching face. Texture comes from the
        // paper height/support fields and from persistent facet recession, not a radial opacity
        // gradient or a synthetic brush particle mask.
        let interior = (1.0 - metric).clamp(0.0, 1.0);
        let interior_load = 0.78 + 0.22 * interior.powf(0.35);
        let profile_load = 0.78 + 0.22 * profile_contact.sqrt();
        let asperity_load = 0.50 + 0.85 * grain;
        let pressure_weight =
            interior_load * profile_load * geometry.force_density_scale * asperity_load;

        TipContactSample {
            coverage: wear_contact,
            pressure_weight,
            signed_distance,
            cross_coordinate_px: local_y,
            axial_coordinate_px: local_x,
            profile_u,
            profile_v,
            profile_contact,
        }
    }
}

/// Smooth, deterministic micro-topography attached to the pencil, not to input frames.
pub(crate) fn contact_grain(x: f32, y: f32) -> f32 {
    fn hash(x: i32, y: i32) -> f32 {
        let mut h =
            (x as u32).wrapping_mul(0x9e3779b9) ^ (y as u32).wrapping_mul(0x85ebca6b) ^ 0x491e3197;
        h ^= h >> 16;
        h = h.wrapping_mul(0x7feb352d);
        h ^= h >> 15;
        h = h.wrapping_mul(0x846ca68b);
        h ^= h >> 16;
        h as f32 / u32::MAX as f32
    }
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let tx = smoothstep01(x - x.floor());
    let ty = smoothstep01(y - y.floor());
    lerp(
        lerp(hash(ix, iy), hash(ix + 1, iy), tx),
        lerp(hash(ix, iy + 1), hash(ix + 1, iy + 1), tx),
        ty,
    )
}

/// Total normal-force response used for paper penetration and transfer. Ordinary tablet pressure is
/// deliberately useful; hard pressure remains substantially stronger, but there is no dead middle
/// range followed by a sudden charcoal jump.
pub fn pencil_pressure_force_drive(pressure: f32) -> f32 {
    let load = pencil_effective_pressure(pressure);
    1.48 * load.powf(1.25)
}

#[derive(Debug, Clone, Copy)]
pub struct ContactSample {
    /// Fraction of the coarse paper cell that could participate in contact before unresolved
    /// micro-asperity selection is applied.
    pub contact_fraction: f32,
    /// Local normal-load proxy after footprint edge weighting.
    pub normal_load: f32,
}

/// Evaluate coarse contact between the pencil tip and the *current* paper height field.
///
/// This stage intentionally does not decide the graphite grain pattern. It estimates the local
/// coarse contact/normal load after the tip geometry has already distributed total stylus force
/// over its physical footprint. Mesoscopic paper/flake response is evaluated separately.
pub fn evaluate_contact(
    current_height: f32,
    footprint_weight: f32,
    pressure: f32,
    core_hardness: f32,
    paper_compliance: f32,
) -> ContactSample {
    PreparedPaperContact::new(pressure, core_hardness, paper_compliance)
        .sample(current_height, footprint_weight)
}

pub(crate) struct PreparedPaperContact {
    pressure_drive: f32,
    contact_plane: f32,
    distribution_width: f32,
    unresolved: f32,
}
impl PreparedPaperContact {
    pub fn new(pressure: f32, core_hardness: f32, paper_compliance: f32) -> Self {
        let hardness = core_hardness.clamp(0., 1.);
        let compliance = paper_compliance.clamp(0., 1.);
        let load = pencil_effective_pressure(pressure);
        Self {
            pressure_drive: pencil_pressure_force_drive(pressure),
            contact_plane: 0.77 - load * (0.66 + 0.08 * compliance - 0.04 * hardness),
            distribution_width: 0.70 + 0.05 * compliance,
            unresolved: 0.16 + 0.18 * load,
        }
    }
    pub fn sample(&self, current_height: f32, footprint_weight: f32) -> ContactSample {
        let local_force = (footprint_weight * self.pressure_drive).clamp(0.0, 1.75);
        if local_force <= 1.0e-5 {
            return ContactSample {
                contact_fraction: 0.0,
                normal_load: 0.0,
            };
        }

        // Resolve contact from sheet relief, with a continuous subpixel population
        // so valleys remain lighter without turning into binary white pores.
        let geometric_contact = smoothstep01(
            0.5 + (current_height.clamp(0.0, 1.0) - self.contact_plane) / self.distribution_width,
        );
        // Each raster cell contains both raised fibers and valleys. A shallow material
        // film bridges some unresolved fibers, so low relief gives a lighter shade
        // instead of an almost-white cutout. Actual tip coverage still bounds the mark.
        let unresolved = self.unresolved;
        let contact_fraction = unresolved + (1.0 - unresolved) * geometric_contact;
        let normal_load = local_force;

        ContactSample {
            contact_fraction,
            normal_load,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MesoscopicContactSample {
    /// Continuous fraction of unresolved fiber tops contacted inside this document pixel.
    /// Unlike the old micro-contact gate this is deliberately not binary.
    pub contact_fraction: f32,
    /// Material-capture multiplier supplied by the statistical paper tooth.
    pub capture: f32,
    /// Slight dark-gray particle value multiplier; never a white-hole mask.
    pub particle_tone: f32,
    pub normal_load: f32,
}

/// Fast real-time approximation of sub-pixel porous contact.
///
/// One 120–300 DPI document pixel contains many real cellulose fibers. Instead of allocating and
/// testing those fibers individually for every dab, `paper_support` stores their precomputed
/// statistical contact fraction. The result is continuous coverage with grain/streak variation,
/// which is both faster and visually closer to diffuse graphite than binary micro-cells.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_mesoscopic_contact(
    _x: usize,
    _y: usize,
    _cross_coordinate_px: f32,
    _axial_coordinate_px: f32,
    tilt: f32,
    tip_face_contact: f32,
    _pressure: f32,
    paper_support: f32,
    macro_contact: ContactSample,
    _dpi: f32,
    tone_grain: u8,
    particle_variation: f32,
) -> MesoscopicContactSample {
    if macro_contact.normal_load <= 1.0e-7 || macro_contact.contact_fraction <= 1.0e-7 {
        return MesoscopicContactSample::default();
    }

    let support = paper_support.clamp(0.0, 1.0);
    let tilt = tilt.clamp(0.0, 1.0);
    let tip_face_contact = tip_face_contact.clamp(0.0, 1.0);

    // One contact solve, using the persistent sheet. This stage only describes
    // local bite/capture; a second pressure threshold would obscure the first.
    let contact_fraction = macro_contact.contact_fraction * tip_face_contact;
    let bite = 0.70 + 0.50 * support;
    let capture = (contact_fraction * bite * (1.0 + 0.08 * tilt)).clamp(0.0, 1.25);
    let normal_load = macro_contact.normal_load;
    // Vary the deposited color around the selected pencil shade, not toward white.
    // The earlier multiplier was so small that 8-bit output erased it entirely.
    let grain = (tone_grain as f32 / 255.0 - 0.5) * 2.0;
    // Slightly deeper dark particles; light particles remain shades of the pigment.
    let depth = if grain < 0.0 { 1.25 } else { 0.80 };
    let particle_tone = 1.0 + grain * depth * particle_variation.clamp(0.0, 0.35);

    MesoscopicContactSample {
        contact_fraction,
        capture,
        particle_tone,
        normal_load,
    }
}

fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_block_rejection_preserves_rotated_worn_contact_and_aa_fringe() {
        for core in [0.8, 7., 47.] {
            for (pressure, tilt, azimuth, sharpness, rotation) in [
                (0.02, 0., 0., 1., 0.),
                (0.25, 45., 37., 0., 71.),
                (0.65, 78., 113., 0.65, 37.),
                (1., 84., 281., 1., 293.),
            ] {
                let mut profile = TipProfile::fresh_default(0.58);
                profile.set_orientation(rotation);
                for _ in 0..100 {
                    profile.register_contact_wear(
                        0.4,
                        -0.25,
                        1.,
                        50000.,
                        super::super::pencil::PencilGrade::HB.formulation(),
                    );
                }
                profile.commit_pending_wear();
                let geometry = PencilTipContact::from_state_with_sharpness(
                    core, pressure, tilt, azimuth, 0.58, sharpness,
                );
                let prepared = geometry.prepare(&profile, pressure);
                let reach = geometry.reach_px().ceil() as i32 + 2;
                for top in (-reach..=reach).step_by(8) {
                    for left in (-reach..=reach).step_by(8) {
                        let possible = prepared.may_cover_box(
                            left as f32 + 0.27,
                            top as f32 + 0.63,
                            (left + 7) as f32 + 0.27,
                            (top + 7) as f32 + 0.63,
                        );
                        for y in top..top + 8 {
                            for x in left..left + 8 {
                                let (dx, dy) = (x as f32 + 0.27, y as f32 + 0.63);
                                let reference = geometry.sample(dx, dy, &profile, pressure);
                                let fast = prepared.sample_for_deposition(dx, dy, &profile);
                                if reference.coverage > 1e-5 {
                                    assert!(
                                        possible,
                                        "missed contact: core={core} tilt={tilt} at {dx},{dy}"
                                    );
                                    assert_eq!(fast, reference);
                                } else {
                                    assert!(fast.coverage <= 1e-5);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn sharpness_changes_apex_without_losing_pressure_or_side_shading() {
        let contact = |sharpness, pressure, tilt| {
            PencilTipContact::from_state_with_sharpness(10., pressure, tilt, 0., 0.58, sharpness)
        };
        let sharp = contact(1., 0.5, 8.);
        let blunt = contact(0., 0.5, 8.);
        assert!(blunt.point_radius_px > sharp.point_radius_px * 2.);
        assert!(contact(1., 0.9, 8.).point_radius_px > contact(1., 0.1, 8.).point_radius_px);
        assert!(
            (contact(0., 0.5, 80.).cross_radius_px - contact(1., 0.5, 80.).cross_radius_px).abs()
                < 0.01
        );
        assert_eq!(
            contact(0.5, 0.5, 8.).point_radius_px,
            PencilTipContact::from_state(10., 0.5, 8., 0., 0.58).point_radius_px
        );
    }

    fn macro_contact() -> ContactSample {
        ContactSample {
            contact_fraction: 0.62,
            normal_load: 0.48,
        }
    }

    #[test]
    fn tapered_point_grows_substantially_with_pressure() {
        let core_px = 2.0 * 120.0 / 25.4;
        let low = PencilTipContact::from_state(core_px, 0.08, 8.0, 0.0, 0.58);
        let medium = PencilTipContact::from_state(core_px, 0.45, 8.0, 0.0, 0.58);
        let hard = PencilTipContact::from_state(core_px, 0.90, 8.0, 0.0, 0.58);
        assert!(
            medium.point_radius_px > low.point_radius_px * 1.55,
            "ordinary pressure must visibly broaden a tapered wooden-pencil point"
        );
        assert!(
            hard.point_radius_px > medium.point_radius_px * 1.20,
            "hard pressure must continue broadening the contact rather than only darkening it"
        );
        assert!(
            hard.point_radius_px < core_px * 0.5,
            "even hard upright pressure must remain bounded by the physical core"
        );

        let light_force = pencil_pressure_force_drive(0.08);
        let hard_force = pencil_pressure_force_drive(0.90);
        assert!(
            hard_force > light_force * 5.0,
            "pressure must also expand the tonal force range strongly"
        );
    }

    #[test]
    fn upright_sharpened_point_uses_only_a_fraction_of_the_core() {
        let core_px = 2.0 * 120.0 / 25.4;
        let tip = PencilTipContact::from_state(core_px, 0.48, 8.0, 0.0, 0.58);
        assert!(tip.point_radius_px * 2.0 < core_px * 0.55);
        assert!(tip.side_fraction < 0.01);
        assert!(tip.rear_extension_px < 0.05);
    }

    #[test]
    fn tilt_exposes_a_long_directional_facet_bounded_by_core_width() {
        let core_px = 2.0 * 120.0 / 25.4;
        let upright = PencilTipContact::from_state(core_px, 0.55, 8.0, 0.0, 0.58);
        let tilted = PencilTipContact::from_state(core_px, 0.55, 76.0, 0.0, 0.58);
        assert!(tilted.rear_extension_px > core_px * 1.5);
        assert!(tilted.cross_radius_px > upright.cross_radius_px * 1.65);
        assert!(tilted.cross_radius_px * 2.0 <= core_px * 1.10);
        assert!(tilted.force_density_scale < upright.force_density_scale);
    }

    #[test]
    fn side_contact_is_one_sided_not_a_centered_capsule() {
        let core_px = 2.0 * 120.0 / 25.4;
        let tip = PencilTipContact::from_state(core_px, 0.55, 76.0, 0.0, 0.58);
        let profile = TipProfile::fresh_default(0.58);
        let behind = tip.sample(-tip.rear_extension_px * 0.72, 0.0, &profile, 0.55);
        let equally_far_ahead = tip.sample(tip.rear_extension_px * 0.72, 0.0, &profile, 0.55);
        assert!(behind.coverage > 0.0);
        assert!(equally_far_ahead.coverage <= 1.0e-5);
    }

    #[test]
    fn broad_facet_has_a_mostly_flat_mechanical_interior() {
        let core_px = 2.0 * 120.0 / 25.4;
        let tip = PencilTipContact::from_state(core_px, 0.55, 72.0, 0.0, 0.58);
        let profile = TipProfile::fresh_default(0.58);
        let center = tip
            .sample(-tip.rear_extension_px * 0.45, 0.0, &profile, 0.55)
            .pressure_weight;
        let shoulder = tip
            .sample(
                -tip.rear_extension_px * 0.45,
                tip.cross_radius_px * 0.62,
                &profile,
                0.55,
            )
            .pressure_weight;
        assert!(center > 0.0 && shoulder > 0.0);
        assert!(
            shoulder > center * 0.78,
            "paper tooth, not radial opacity, should form the texture"
        );
    }

    #[test]
    fn rotating_a_worn_profile_changes_local_contact() {
        let core_px = 2.0 * 120.0 / 25.4;
        let tip = PencilTipContact::from_state(core_px, 0.35, 68.0, 0.0, 0.6);
        let mut profile = TipProfile::fresh_default(0.6);
        let formulation = super::super::pencil::PencilGrade::B4.formulation();
        let sample = tip.sample(
            -tip.rear_extension_px * 0.35,
            tip.cross_radius_px * 0.55,
            &profile,
            0.35,
        );
        // Cut the exact contacting region, rather than a different point on the map.
        profile.register_contact_wear(
            sample.profile_u,
            sample.profile_v,
            0.95,
            2_000_000.0,
            formulation,
        );
        profile.commit_pending_wear();
        let worn_contact = tip
            .sample(
                -tip.rear_extension_px * 0.35,
                tip.cross_radius_px * 0.55,
                &profile,
                0.35,
            )
            .profile_contact;
        profile.rotate_by(90.0);
        let rotated_contact = tip
            .sample(
                -tip.rear_extension_px * 0.35,
                tip.cross_radius_px * 0.55,
                &profile,
                0.35,
            )
            .profile_contact;
        assert!((rotated_contact - worn_contact).abs() > 0.02);
    }

    #[test]
    fn mesoscopic_contact_is_continuous_not_binary() {
        let sample = evaluate_mesoscopic_contact(
            20,
            30,
            0.0,
            0.0,
            0.1,
            1.0,
            0.48,
            0.65,
            macro_contact(),
            300.0,
            128,
            0.03,
        );
        assert!(sample.contact_fraction > 0.03);
        assert!(sample.capture > 0.0);
        assert!(sample.particle_tone > 0.90 && sample.particle_tone < 1.08);
    }

    #[test]
    fn same_paper_grain_retains_its_tone_as_tip_coordinates_change() {
        let a = evaluate_mesoscopic_contact(
            20,
            30,
            0.4,
            -0.8,
            0.2,
            0.92,
            0.48,
            0.65,
            macro_contact(),
            300.0,
            128,
            0.10,
        );
        let b = evaluate_mesoscopic_contact(
            20,
            30,
            -0.9,
            0.7,
            0.2,
            0.92,
            0.48,
            0.65,
            macro_contact(),
            300.0,
            128,
            0.10,
        );
        assert!((a.contact_fraction - b.contact_fraction).abs() < 1.0e-7);
        assert!((a.capture - b.capture).abs() < 1.0e-7);
        assert!((a.particle_tone - b.particle_tone).abs() < 1.0e-7);
    }

    #[test]
    fn high_asperity_contacts_more_than_valley() {
        let peak = evaluate_contact(0.82, 1.0, 0.25, 0.6, 0.5).contact_fraction;
        let valley = evaluate_contact(0.30, 1.0, 0.25, 0.6, 0.5).contact_fraction;
        assert!(peak > valley);
    }

    #[test]
    fn pressure_brings_more_surface_into_contact() {
        let low = evaluate_contact(0.48, 1.0, 0.20, 0.5, 0.5).contact_fraction;
        let high = evaluate_contact(0.48, 1.0, 0.90, 0.5, 0.5).contact_fraction;
        assert!(high > low);
    }

    #[test]
    fn texture_depth_increases_with_pressure_without_binary_holes() {
        let low = evaluate_mesoscopic_contact(
            31,
            17,
            0.2,
            0.4,
            0.0,
            1.0,
            0.12,
            0.46,
            evaluate_contact(0.46, 1.0, 0.12, 0.5, 0.5),
            300.0,
            42,
            0.03,
        );
        let high = evaluate_mesoscopic_contact(
            31,
            17,
            0.2,
            0.4,
            0.0,
            1.0,
            0.86,
            0.46,
            evaluate_contact(0.46, 1.0, 0.86, 0.5, 0.5),
            300.0,
            42,
            0.03,
        );
        assert!(low.contact_fraction > 0.0);
        assert!(high.contact_fraction > low.contact_fraction);
    }
}
