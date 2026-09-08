use super::{
    document::{DirtyRect, Document},
    history::EditTransaction,
    pencil::{ToolKind, ToolSettings},
    stroke::StrokePoint,
};
/// Deposit colored graphite into the active material layer. Tissue has a broad,
/// feathered contact; a sampled brush uses its imported mask without a round clip.
pub fn apply(
    doc: &mut Document,
    settings: &ToolSettings,
    point: StrokePoint,
    travel: f32,
    tx: &mut EditTransaction,
) -> Option<DirtyRect> {
    let tissue = settings.tool == ToolKind::Tissue;
    let size = if tissue {
        settings.tissue_size_px
    } else {
        settings.brush_size_px
    }
    .clamp(0.1, 8192.);
    let p = point.pressure.clamp(0., 1.);
    if p <= 0. || travel <= 0. || settings.flow <= 0. || (tissue && settings.tissue_load <= 0.) {
        return None;
    }
    let size = size * (0.65 + 0.35 * p.sqrt());
    let (w, h) = if let Some(tip) = settings.brush_tip.as_ref().filter(|_| !tissue) {
        let longest = tip.width.max(tip.height) as f32;
        (
            size * tip.width as f32 / longest,
            size * tip.height as f32 / longest,
        )
    } else {
        (size, size)
    };
    // Tissue is rotationally symmetric; avoid rotating its bounds and every pixel.
    let (sin, cos) = if tissue {
        (0., 1.)
    } else {
        settings.brush_angle_deg.to_radians().sin_cos()
    };
    let bw = cos.abs() * w + sin.abs() * h;
    let bh = sin.abs() * w + cos.abs() * h;
    let x0 = (point.x - bw * 0.5 - 1.).floor().max(0.) as usize;
    let y0 = (point.y - bh * 0.5 - 1.).floor().max(0.) as usize;
    let x1 = (point.x + bw * 0.5 + 1.)
        .ceil()
        .max(0.)
        .min(doc.spec.width_px as f32) as usize;
    let y1 = (point.y + bh * 0.5 + 1.)
        .ceil()
        .max(0.)
        .min(doc.spec.height_px as f32) as usize;
    let f = settings.grade.formulation();
    let rgb = settings.pencil_color_rgb.map(|v| v as f32 / 255.);
    let strength = if tissue {
        settings.tissue_load * 0.20
    } else {
        1.5
    };
    let mut changed = false;
    let inv_radius2 = 4. / (size * size);
    let work_scale = p * settings.flow * strength * travel / size.max(1.);
    for y in y0..y1 {
        let dy = y as f32 + 0.5 - point.y;
        let row_radius2 = dy * dy * inv_radius2;
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - point.x;
            let coverage = if tissue {
                // Same (1-r²)^3 falloff, evaluated directly without sqrt/rotation/division.
                let falloff = (1. - (dx * dx * inv_radius2 + row_radius2)).max(0.);
                falloff * falloff * falloff
            } else {
                let u = (dx * cos + dy * sin) / w + 0.5;
                let v = (-dx * sin + dy * cos) / h + 0.5;
                if let Some(tip) = settings.brush_tip.as_ref() {
                    tip.sample(u, v)
                } else {
                    let r = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.;
                    ((1. - r) * size * 0.5 + 0.5).clamp(0., 1.)
                }
            };
            if coverage <= 0.00001 {
                continue;
            }
            let i = doc.index(x, y);
            if !doc.selection.allows(i) {
                continue;
            }
            let support = doc.surface.contact_support[i] as f32 / 255.;
            let grain = doc.color_grain[i] as f32 / 255.;
            let capacity = doc.surface.local_capacity(i);
            let remaining = (capacity - doc.surface.total_deposit(i)).max(0.);
            let work = coverage * work_scale * (0.8 + 0.2 * support);
            let amount = remaining * (-(-work).exp_m1());
            if amount <= 1e-8 {
                continue;
            }
            tx.remember(i, doc);
            doc.surface.graphite_mass[i] += amount * f.graphite_fraction;
            doc.surface.clay_mass[i] += amount * f.clay_fraction;
            doc.surface.wax_mass[i] += amount * f.wax_fraction;
            doc.surface.loose_mass[i] += amount * 0.85;
            doc.surface.compacted_mass[i] += amount * 0.15;
            let tone =
                1. + (grain - 0.5) * settings.particle_variation * if tissue { 0.3 } else { 0.8 };
            doc.surface.color_r_mass[i] += amount * (rgb[0] * tone).clamp(0., 1.);
            doc.surface.color_g_mass[i] += amount * (rgb[1] * tone).clamp(0., 1.);
            doc.surface.color_b_mass[i] += amount * (rgb[2] * tone).clamp(0., 1.);
            changed = true;
        }
    }
    changed.then_some(DirtyRect::new(x0, y0, x1, y1))
}
