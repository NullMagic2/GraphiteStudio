use super::{
    document::Document,
    history::EditTransaction,
    material::SparseDepositState,
    pencil::{PencilTipState, ToolKind, ToolSettings},
    selection::Selection,
    stroke::{StrokeEngine, StrokePoint},
};
use eframe::egui::{Rect, Vec2};
use std::sync::Arc;
/// Editable path commands remain separate from their material rasterization.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct VectorStroke {
    pub label: String,
    pub settings: ToolSettings,
    pub tip: PencilTipState,
    pub engine: StrokeEngine,
    pub points: Vec<StrokePoint>,
    pub polyline: bool,
    pub selection: Selection,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct VectorLayer {
    pub base: Option<Arc<SparseDepositState>>,
    pub strokes: Vec<Arc<VectorStroke>>,
}
impl VectorLayer {
    pub fn raster_base(doc: &Document) -> Self {
        let mut base = SparseDepositState::new(doc.spec.width_px, doc.spec.height_px);
        base.capture_from_surface(&doc.surface);
        Self {
            base: Some(Arc::new(base)),
            strokes: Vec::new(),
        }
    }
}
impl VectorStroke {
    /// Hit the retained path, including its smoothed curve, rather than individual grain pixels.
    pub fn hit_test(&self, point: Vec2, tolerance: f32, dpi: f32) -> bool {
        if self.points.is_empty() || self.opacity() <= 0. {
            return false;
        }
        if !self.bounds(dpi).expand(tolerance).contains(point.to_pos2()) {
            return false;
        }
        let radius = |p: StrokePoint| match self.settings.tool {
            ToolKind::Pencil => {
                super::contact::PencilTipContact::from_state_with_sharpness(
                    self.tip.effective_core_diameter_px(dpi) * self.settings.pencil_geometry_scale
                        * self.settings.pressure_width_scale(p.pressure),
                    p.pressure,
                    p.tilt_deg,
                    p.azimuth_deg,
                    self.settings.grade.formulation().core_hardness,
                    self.settings.tip_sharpness,
                )
                .cross_radius_px
            }
            ToolKind::Tissue => self.settings.tissue_size_px * 0.5,
            ToolKind::Eraser => self.settings.eraser_diameter_mm * dpi / 50.8,
            ToolKind::Smudge => self.settings.smudge_size_px * 0.5,
            _ => self.settings.brush_size_px * 0.5,
        };
        let segment = |a: StrokePoint, b: StrokePoint| {
            let start = Vec2::new(a.x, a.y);
            let delta = Vec2::new(b.x, b.y) - start;
            let t = ((point - start).dot(delta) / delta.length_sq().max(1e-8)).clamp(0., 1.);
            (point - start - delta * t).length() <= tolerance + radius(a.lerp(b, t))
        };
        if self.polyline {
            return self.points.windows(2).any(|p| segment(p[0], p[1]));
        }
        let first = self.points[0];
        if segment(first, first) {
            return true;
        }
        let curve = |a: StrokePoint, control: StrokePoint, b: StrokePoint| {
            let length = Vec2::new(control.x - a.x, control.y - a.y).length()
                + Vec2::new(b.x - control.x, b.y - control.y).length();
            let count = (length / tolerance.max(0.5)).ceil().clamp(2., 2048.) as usize;
            let mut previous = a;
            for i in 1..=count {
                let p = StrokePoint::quadratic(a, control, b, i as f32 / count as f32);
                if segment(previous, p) {
                    return true;
                }
                previous = p;
            }
            false
        };
        let (mut raw, mut cursor) = (first, first);
        for &p in self.points.iter().skip(1) {
            let mid = raw.lerp(p, 0.5);
            if curve(cursor, raw, mid) {
                return true;
            }
            cursor = mid;
            raw = p;
        }
        curve(cursor, raw, raw)
    }
    pub fn bounds(&self, dpi: f32) -> Rect {
        let mut bounds = Rect::NOTHING;
        for p in &self.points {
            bounds.extend_with(eframe::egui::Pos2::new(p.x, p.y));
        }
        let diameter = match self.settings.tool {
            ToolKind::Pencil => {
                self.tip.effective_core_diameter_px(dpi) * self.settings.pencil_geometry_scale * 2.
            }
            ToolKind::Smudge => self.settings.smudge_size_px,
            ToolKind::Eraser => self.settings.eraser_diameter_mm * dpi / 25.4,
            ToolKind::Tissue => self.settings.tissue_size_px,
            _ => self.settings.brush_size_px,
        };
        bounds.expand(diameter + 2.)
    }

    pub fn rotated(&self, center: eframe::egui::Pos2, angle: f32) -> Self {
        let mut out = self.clone();
        let degrees = angle.to_degrees();
        for point in &mut out.points {
            let p = rotate(eframe::egui::Pos2::new(point.x, point.y), center, angle);
            point.x = p.x;
            point.y = p.y;
            point.azimuth_deg = (point.azimuth_deg + degrees).rem_euclid(360.);
        }
        out.settings.brush_angle_deg = (out.settings.brush_angle_deg + degrees).rem_euclid(360.);
        for point in &mut out.selection.polygon {
            *point = rotate(point.to_pos2(), center, angle).to_vec2();
        }
        out
    }
    pub fn transformed(&self, from: Rect, to: Rect) -> Self {
        let mut out = self.clone();
        let scale = to.size() / from.size();
        let width = (scale.x * scale.y).sqrt();
        for p in &mut out.points {
            let v = to.min.to_vec2() + (Vec2::new(p.x, p.y) - from.min.to_vec2()) * scale;
            p.x = v.x;
            p.y = v.y;
        }
        out.settings.brush_size_px *= width;
        out.settings.shape_width_px *= width;
        out.settings.tissue_size_px *= width;
        out.settings.smudge_size_px *= width;
        out.settings.eraser_diameter_mm *= width;
        out.settings.pencil_geometry_scale *= width;
        if !out.selection.polygon.is_empty() {
            out.selection.polygon = out
                .selection
                .polygon
                .iter()
                .map(|p| to.min.to_vec2() + (*p - from.min.to_vec2()) * scale)
                .collect();
        }
        out
    }
    pub fn apply(&self, doc: &mut Document, tx: &mut EditTransaction) {
        let opacity = self.opacity();
        if opacity <= 0. {
            return;
        }
        if opacity < 1. {
            let mut local = EditTransaction::default();
            self.apply_full(doc, &mut local);
            local.finish_with_opacity(doc, tx, opacity);
        } else {
            self.apply_full(doc, tx);
        }
    }

    pub fn opacity(&self) -> f32 {
        if self.polyline && self.settings.tool == ToolKind::Pencil {
            self.settings.shape_opacity.clamp(0., 1.)
        } else {
            1.
        }
    }

    fn apply_full(&self, doc: &mut Document, tx: &mut EditTransaction) {
        let Some(&first) = self.points.first() else {
            return;
        };
        let mut engine = self.engine.clone();
        let mut tip = self.tip.clone();
        if self.settings.tool == ToolKind::Pencil {
            engine.begin_pencil_stroke(first);
        }
        let old_selection = doc.selection.clone();
        doc.selection = self.selection.clone();
        if !self.selection.polygon.is_empty() {
            doc.selection.set(
                self.selection.polygon.clone(),
                doc.spec.width_px,
                doc.spec.height_px,
            );
        }
        if self.polyline {
            for pair in self.points.windows(2) {
                engine.apply_segment(doc, &self.settings, &mut tip, pair[0], pair[1], tx);
            }
        } else {
            engine.apply_dab(doc, &self.settings, &mut tip, first, tx);
            let (mut raw, mut cursor) = (first, first);
            for &point in self.points.iter().skip(1) {
                let mid = raw.lerp(point, 0.5);
                engine.apply_quadratic_segment(doc, &self.settings, &mut tip, cursor, raw, mid, tx);
                cursor = mid;
                raw = point;
            }
            engine.apply_quadratic_segment(doc, &self.settings, &mut tip, cursor, raw, raw, tx);
        }
        doc.selection = old_selection;
    }
}
pub fn replay(doc: &mut Document, layer: &VectorLayer, tx: &mut EditTransaction) {
    // Re-evaluate path geometry without repeatedly abrading the shared paper.
    let height = doc.surface.current_height.clone();
    let abrasion = doc.surface.abrasion.clone();
    for i in 0..doc.spec.pixel_count() {
        if doc.surface.total_deposit(i) > 0. {
            tx.remember(i, doc);
        }
    }
    doc.surface.clear_deposit();
    if let Some(base) = &layer.base {
        // Remember empty destinations before restoring the retained raster base.
        let mut scratch = doc.surface.clone();
        base.load_into_surface(&mut scratch);
        for i in 0..doc.spec.pixel_count() {
            if scratch.total_deposit(i) > 0. {
                tx.remember(i, doc);
                doc.surface.set_deposit_pixel(i, scratch.deposit_pixel(i));
            }
        }
    }
    for stroke in &layer.strokes {
        stroke.apply(doc, tx);
    }
    doc.surface.current_height = height;
    doc.surface.abrasion = abrasion;
    doc.mark_all_dirty();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaling_pencil_geometry_is_not_clamped_to_physical_core_sizes() {
        let settings = ToolSettings::default();
        let p = StrokePoint {
            x: 10.,
            y: 20.,
            pressure: 0.7,
            tilt_deg: 65.,
            azimuth_deg: 35.,
            rotation_deg: Some(80.),
        };
        let original = VectorStroke {
            label: "Pencil".into(),
            tip: PencilTipState::default(),
            engine: StrokeEngine::default(),
            points: vec![p],
            polyline: false,
            selection: Selection::default(),
            settings,
        };
        let a = Rect::from_min_size(eframe::egui::Pos2::ZERO, Vec2::splat(100.));
        let b = Rect::from_min_size(eframe::egui::Pos2::new(25., 30.), Vec2::splat(800.));
        let enlarged = original.transformed(a, b);
        assert_eq!(enlarged.settings.pencil_geometry_scale, 8.);
        assert_eq!(enlarged.tip.core_diameter_mm, 2.);
        assert_eq!(enlarged.points[0].x, 105.);
        assert_eq!(enlarged.points[0].pressure, p.pressure);
        assert_eq!(enlarged.points[0].tilt_deg, p.tilt_deg);
        assert_eq!(enlarged.points[0].rotation_deg, p.rotation_deg);
        let restored = enlarged.transformed(b, a);
        assert_eq!(restored.points[0].x, p.x);
        assert_eq!(restored.settings.pencil_geometry_scale, 1.);
    }
}

pub fn rotate(p: eframe::egui::Pos2, center: eframe::egui::Pos2, angle: f32) -> eframe::egui::Pos2 {
    let (s, c) = angle.sin_cos();
    let v = p - center;
    center + Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}
