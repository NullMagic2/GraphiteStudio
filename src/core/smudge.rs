use std::f32::consts::PI;

use super::{
    document::{DirtyRect, Document},
    history::EditTransaction,
};

/// Mobile material carried by a blending stump / smudger.
///
/// This is deliberately composition-aware rather than a sampled color. The stump therefore
/// transports the same graphite/clay/wax material that exists on the paper, and its directional
/// state comes from the picked-up deposit rather than from blurring rendered pixels.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Default, PartialEq)]
pub struct SmudgeReservoir {
    pub graphite_mass: f32,
    pub clay_mass: f32,
    pub wax_mass: f32,
    pub orientation_x: f32,
    pub orientation_y: f32,
    pub color_r_mass: f32,
    pub color_g_mass: f32,
    pub color_b_mass: f32,
}

impl SmudgeReservoir {
    pub fn total_mass(self) -> f32 {
        self.graphite_mass + self.clay_mass + self.wax_mass
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn load_fraction(self, diameter_px: f32) -> f32 {
        let capacity = reservoir_capacity(diameter_px);
        if capacity <= 1.0e-8 {
            0.0
        } else {
            (self.total_mass() / capacity).clamp(0.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SmudgeCell {
    index: usize,
    footprint: f32,
    leading: f32,
    trailing: f32,
}

/// A soft paper stump compresses slightly as force rises, but unlike the graphite tip we keep that
/// geometric response deliberately small: smudging pressure should primarily change transport and
/// contact, not act like a second brush-size control.
pub fn smudge_pressure_radius_scale(pressure: f32) -> f32 {
    let p = pressure.clamp(0.0, 1.0);
    0.86 + 0.18 * p.sqrt()
}

/// Apply one material-transport contact of the smudger.
///
/// Existing reservoir material is redeposited preferentially on the trailing half of the contact,
/// then the leading half picks up accessible material. That ordering gives a moving stump a natural
/// carry/smear direction and prevents the result from depending on raster scan order.
pub fn apply_smudge_dab(
    document: &mut Document,
    tx: &mut EditTransaction,
    reservoir: &mut SmudgeReservoir,
    center_x: f32,
    center_y: f32,
    diameter_px: f32,
    pressure: f32,
    strength: f32,
    motion: Option<(f32, f32)>,
) -> Option<DirtyRect> {
    let pressure = pressure.clamp(0.0, 1.0);
    if pressure <= 1.0e-4 {
        return None;
    }

    let diameter_px = diameter_px.max(1.0);
    let radius = diameter_px * 0.5 * smudge_pressure_radius_scale(pressure);
    let reach = radius.ceil() as i32 + 1;
    let min_x = (center_x.floor() as i32 - reach).max(0) as usize;
    let min_y = (center_y.floor() as i32 - reach).max(0) as usize;
    let max_x = (center_x.ceil() as i32 + reach + 1)
        .min(document.spec.width_px as i32)
        .max(0) as usize;
    let max_y = (center_y.ceil() as i32 + reach + 1)
        .min(document.spec.height_px as i32)
        .max(0) as usize;
    if min_x >= max_x || min_y >= max_y {
        return None;
    }

    let motion = normalized_motion(motion);
    let can_deposit = reservoir.total_mass() > 1e-8;
    let mut cells = Vec::with_capacity(((max_x - min_x) * (max_y - min_y)).min(1_000_000));
    for y in min_y..max_y {
        for x in min_x..max_x {
            if !document.selection.allows(document.index(x, y)) {
                continue;
            }
            // An empty stump cannot affect clean paper. Avoid computing its
            // expensive footprint there, while retaining the original scan order.
            if !can_deposit && document.surface.total_deposit(document.index(x, y)) <= 1e-9 {
                continue;
            }
            let dx = (x as f32 + 0.5) - center_x;
            let dy = (y as f32 + 0.5) - center_y;
            let r2 = (dx * dx + dy * dy) / (radius * radius);
            if r2 >= 1.0 {
                continue;
            }

            // A stump has a soft but defined contact edge. Directional leading/trailing weights are
            // symmetric when there is no motion (e.g. the first dab of a stroke).
            let footprint = (1.0 - r2).powf(1.18);
            let directional = motion
                .map(|(mx, my)| ((dx * mx + dy * my) / radius).clamp(-1.0, 1.0))
                .unwrap_or(0.0);
            cells.push(SmudgeCell {
                index: document.index(x, y),
                footprint,
                leading: (1.0 + 0.52 * directional).clamp(0.42, 1.58),
                trailing: (1.0 - 0.52 * directional).clamp(0.42, 1.58),
            });
        }
    }
    if cells.is_empty() {
        return None;
    }

    let strength = strength.clamp(0.05, 1.5);
    let mut changed = false;

    // 1) A dirty stump lays down some of what it is already carrying, biased toward the trailing
    // side. Valley/fiber capture keeps the redeposit coupled to paper structure rather than making
    // a uniform airbrush trail.
    let reservoir_before_deposit = reservoir.total_mass();
    if reservoir_before_deposit > 1.0e-8 {
        let release_fraction = ((0.035 + 0.14 * pressure.powf(0.72)) * strength).clamp(0.0, 0.30);
        let release_budget = reservoir_before_deposit * release_fraction;

        let mut weight_sum = 0.0f32;
        for cell in &cells {
            let remaining = document.surface.remaining_capacity(cell.index);
            if remaining <= 1.0e-8 {
                continue;
            }
            let valley = (1.0 - document.surface.current_height[cell.index]).clamp(0.0, 1.0);
            let fiber = document.surface.fiber[cell.index];
            let capture = 0.44 + 0.34 * valley + 0.22 * fiber;
            weight_sum += cell.footprint * cell.trailing * capture * remaining.min(0.45);
        }

        if weight_sum > 1.0e-8 && release_budget > 1.0e-8 {
            let g_ratio = reservoir.graphite_mass / reservoir_before_deposit;
            let c_ratio = reservoir.clay_mass / reservoir_before_deposit;
            let w_ratio = reservoir.wax_mass / reservoir_before_deposit;
            let ox_per_mass = reservoir.orientation_x / reservoir_before_deposit;
            let oy_per_mass = reservoir.orientation_y / reservoir_before_deposit;
            let cr_per_mass = reservoir.color_r_mass / reservoir_before_deposit;
            let cg_per_mass = reservoir.color_g_mass / reservoir_before_deposit;
            let cb_per_mass = reservoir.color_b_mass / reservoir_before_deposit;
            let motion_axis = motion.map(|(mx, my)| {
                let a2 = my.atan2(mx) * 2.;
                (a2.cos(), a2.sin())
            });
            let alignment = (0.10 + 0.22 * pressure) * strength.min(1.0);
            let compact_fraction = 0.018 + 0.12 * pressure.powf(1.3);

            let mut actually_released = 0.0f32;
            let mut released_ox = 0.0f32;
            let mut released_oy = 0.0f32;
            for cell in &cells {
                let remaining = document.surface.remaining_capacity(cell.index);
                if remaining <= 1.0e-8 {
                    continue;
                }
                let valley = (1.0 - document.surface.current_height[cell.index]).clamp(0.0, 1.0);
                let fiber = document.surface.fiber[cell.index];
                let capture = 0.44 + 0.34 * valley + 0.22 * fiber;
                let weight = cell.footprint * cell.trailing * capture * remaining.min(0.45);
                if weight <= 1.0e-10 {
                    continue;
                }
                let amount = (release_budget * weight / weight_sum).min(remaining);
                if amount <= 1.0e-9 {
                    continue;
                }

                tx.remember(cell.index, document);
                document.surface.graphite_mass[cell.index] += amount * g_ratio;
                document.surface.clay_mass[cell.index] += amount * c_ratio;
                document.surface.wax_mass[cell.index] += amount * w_ratio;
                document.surface.color_r_mass[cell.index] += amount * cr_per_mass;
                document.surface.color_g_mass[cell.index] += amount * cg_per_mass;
                document.surface.color_b_mass[cell.index] += amount * cb_per_mass;

                // Smudged material lands mostly loose, with only modest instantaneous packing.
                let compacted = amount * compact_fraction;
                document.surface.loose_mass[cell.index] += amount - compacted;
                document.surface.compacted_mass[cell.index] += compacted;

                let mut ox = amount * ox_per_mass;
                let mut oy = amount * oy_per_mass;
                if let Some((cos, sin)) = motion_axis {
                    ox = ox * (1.0 - alignment) + amount * alignment * cos;
                    oy = oy * (1.0 - alignment) + amount * alignment * sin;
                }
                document.surface.orientation_x[cell.index] += ox;
                document.surface.orientation_y[cell.index] += oy;

                actually_released += amount;
                released_ox += amount * ox_per_mass;
                released_oy += amount * oy_per_mass;
                changed = true;
            }

            if actually_released > 0.0 {
                reservoir.graphite_mass =
                    (reservoir.graphite_mass - actually_released * g_ratio).max(0.0);
                reservoir.clay_mass = (reservoir.clay_mass - actually_released * c_ratio).max(0.0);
                reservoir.wax_mass = (reservoir.wax_mass - actually_released * w_ratio).max(0.0);
                reservoir.orientation_x -= released_ox;
                reservoir.orientation_y -= released_oy;
                reservoir.color_r_mass =
                    (reservoir.color_r_mass - actually_released * cr_per_mass).max(0.0);
                reservoir.color_g_mass =
                    (reservoir.color_g_mass - actually_released * cg_per_mass).max(0.0);
                reservoir.color_b_mass =
                    (reservoir.color_b_mass - actually_released * cb_per_mass).max(0.0);
            }
        }
    }

    // 2) Pick material up from the leading half. Loose particles are substantially more mobile;
    // compacted graphite can move, but only weakly. The capacity limit makes a saturated stump pick
    // up less material until it has deposited some elsewhere.
    let capacity_remaining = (reservoir_capacity(diameter_px) - reservoir.total_mass()).max(0.0);
    if capacity_remaining > 1.0e-8 {
        let mut requests: Vec<(usize, f32, f32)> = Vec::with_capacity(cells.len());
        let mut requested_total = 0.0f32;
        let pressure_shear = pressure.powf(0.68);

        for cell in &cells {
            let total = document.surface.total_deposit(cell.index);
            if total <= 1.0e-8 {
                requests.push((cell.index, 0.0, 0.0));
                continue;
            }
            let accessibility = 0.28 + 0.72 * document.surface.current_height[cell.index];
            let shear = cell.footprint * cell.leading * pressure_shear * strength * accessibility;
            let loose = document.surface.loose_mass[cell.index];
            let compacted = document.surface.compacted_mass[cell.index];
            let loose_take = loose * (shear * 0.19).clamp(0.0, 0.72);
            let compact_take = compacted * (shear * 0.024).clamp(0.0, 0.12);
            requested_total += loose_take + compact_take;
            requests.push((cell.index, loose_take, compact_take));
        }

        let pickup_scale = if requested_total > capacity_remaining {
            capacity_remaining / requested_total
        } else {
            1.0
        };

        for (index, loose_request, compact_request) in requests {
            let loose_removed = loose_request * pickup_scale;
            let compact_removed = compact_request * pickup_scale;
            let removed = loose_removed + compact_removed;
            if removed <= 1.0e-9 {
                continue;
            }

            let total_before = document.surface.total_deposit(index);
            if total_before <= 1.0e-9 {
                continue;
            }
            tx.remember(index, document);
            let fraction = (removed / total_before).clamp(0.0, 1.0);

            let g = document.surface.graphite_mass[index] * fraction;
            let c = document.surface.clay_mass[index] * fraction;
            let w = document.surface.wax_mass[index] * fraction;
            let ox = document.surface.orientation_x[index] * fraction;
            let oy = document.surface.orientation_y[index] * fraction;
            let cr = document.surface.color_r_mass[index] * fraction;
            let cg = document.surface.color_g_mass[index] * fraction;
            let cb = document.surface.color_b_mass[index] * fraction;

            document.surface.graphite_mass[index] -= g;
            document.surface.clay_mass[index] -= c;
            document.surface.wax_mass[index] -= w;
            document.surface.loose_mass[index] =
                (document.surface.loose_mass[index] - loose_removed).max(0.0);
            document.surface.compacted_mass[index] =
                (document.surface.compacted_mass[index] - compact_removed).max(0.0);
            document.surface.orientation_x[index] -= ox;
            document.surface.orientation_y[index] -= oy;
            document.surface.color_r_mass[index] -= cr;
            document.surface.color_g_mass[index] -= cg;
            document.surface.color_b_mass[index] -= cb;

            reservoir.graphite_mass += g;
            reservoir.clay_mass += c;
            reservoir.wax_mass += w;
            reservoir.orientation_x += ox;
            reservoir.orientation_y += oy;
            reservoir.color_r_mass += cr;
            reservoir.color_g_mass += cg;
            reservoir.color_b_mass += cb;
            changed = true;
        }
    }

    // 3) Friction under the stump slightly packs material that remains on the page. This gives a
    // repeatedly rubbed passage a more polished/mechanically stable character without inventing
    // or destroying material.
    let packing_pressure = pressure.powf(1.2);
    for cell in &cells {
        let loose = document.surface.loose_mass[cell.index];
        if loose <= 1.0e-9 {
            continue;
        }
        let compact =
            loose * (cell.footprint * packing_pressure * strength * 0.0045).clamp(0.0, 0.025);
        if compact > 1.0e-9 {
            tx.remember(cell.index, document);
            document.surface.loose_mass[cell.index] -= compact;
            document.surface.compacted_mass[cell.index] += compact;
            changed = true;
        }
    }

    changed.then_some(DirtyRect::new(min_x, min_y, max_x, max_y))
}

fn reservoir_capacity(diameter_px: f32) -> f32 {
    // Effective surface capacity rather than literal stump pore volume. Scaling with contact area
    // keeps loading behavior proportional when the user changes smudger size.
    let radius = diameter_px.max(1.0) * 0.5;
    PI * radius * radius * 0.034
}

fn normalized_motion(motion: Option<(f32, f32)>) -> Option<(f32, f32)> {
    let (x, y) = motion?;
    let len = (x * x + y * y).sqrt();
    (len > 1.0e-5).then_some((x / len, y / len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        document::CanvasSpec,
        paper::{generate_builtin_albedo, PaperPreset, PaperTexturePreset},
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

    #[test]
    fn pressure_changes_smudger_contact_size_only_modestly() {
        let low = smudge_pressure_radius_scale(0.05);
        let high = smudge_pressure_radius_scale(1.0);
        assert!(high > low);
        assert!(high / low < 1.20);
    }

    #[test]
    fn loose_graphite_is_much_more_mobile_than_compacted_graphite() {
        fn pickup_for(loose: f32, compacted: f32) -> f32 {
            let mut document = new_test_document(20.0, 20.0, 100.0, PaperPreset::DrawingMedium);
            let cx = document.spec.width_px / 2;
            let cy = document.spec.height_px / 2;
            let i = document.index(cx, cy);
            document.surface.graphite_mass[i] = 1.0;
            document.surface.loose_mass[i] = loose;
            document.surface.compacted_mass[i] = compacted;
            document.surface.orientation_x[i] = 1.0;
            let mut reservoir = SmudgeReservoir::default();
            let mut tx = EditTransaction::default();
            let _ = apply_smudge_dab(
                &mut document,
                &mut tx,
                &mut reservoir,
                cx as f32 + 0.5,
                cy as f32 + 0.5,
                10.0,
                0.7,
                1.0,
                None,
            );
            reservoir.total_mass()
        }

        let loose_pickup = pickup_for(1.0, 0.0);
        let compact_pickup = pickup_for(0.0, 1.0);
        assert!(loose_pickup > compact_pickup * 4.0);
    }

    #[test]
    fn loaded_stump_redeposits_more_on_trailing_side() {
        let mut document = new_test_document(30.0, 30.0, 100.0, PaperPreset::DrawingMedium);
        let cx = document.spec.width_px / 2;
        let cy = document.spec.height_px / 2;
        let mut reservoir = SmudgeReservoir {
            graphite_mass: 1.0,
            orientation_x: 1.0,
            color_r_mass: 0.2,
            color_g_mass: 0.2,
            color_b_mass: 0.2,
            ..Default::default()
        };
        let mut tx = EditTransaction::default();
        let _ = apply_smudge_dab(
            &mut document,
            &mut tx,
            &mut reservoir,
            cx as f32 + 0.5,
            cy as f32 + 0.5,
            18.0,
            0.7,
            1.0,
            Some((1.0, 0.0)),
        );

        let mut trailing = 0.0;
        let mut leading = 0.0;
        for y in 0..document.spec.height_px {
            for x in 0..document.spec.width_px {
                let g = document.surface.graphite_mass[document.index(x, y)];
                if x < cx {
                    trailing += g;
                }
                if x > cx {
                    leading += g;
                }
            }
        }
        assert!(
            trailing > leading,
            "motion to the right should bias redeposition to the trailing/left side"
        );
    }

    #[test]
    fn smudging_conserves_total_material_between_paper_and_reservoir() {
        let mut document = new_test_document(20.0, 20.0, 100.0, PaperPreset::DrawingMedium);
        let cx = document.spec.width_px / 2;
        let cy = document.spec.height_px / 2;
        let i = document.index(cx, cy);
        document.surface.graphite_mass[i] = 0.8;
        document.surface.clay_mass[i] = 0.15;
        document.surface.wax_mass[i] = 0.05;
        document.surface.loose_mass[i] = 0.85;
        document.surface.compacted_mass[i] = 0.15;
        document.surface.orientation_x[i] = 1.0;

        let before = document.surface.total_deposit(i);
        let mut reservoir = SmudgeReservoir::default();
        let mut tx = EditTransaction::default();
        let _ = apply_smudge_dab(
            &mut document,
            &mut tx,
            &mut reservoir,
            cx as f32 + 0.5,
            cy as f32 + 0.5,
            12.0,
            0.75,
            1.0,
            Some((1.0, 0.0)),
        );

        let paper_after: f32 = document.surface.graphite_mass.iter().sum::<f32>()
            + document.surface.clay_mass.iter().sum::<f32>()
            + document.surface.wax_mass.iter().sum::<f32>();
        assert!((before - (paper_after + reservoir.total_mass())).abs() < 1.0e-5);
    }
}
