use super::*;
use crate::core::orientation::QuarterTurn;

impl GraphiteApp {
    pub(super) fn grow_workspace(&mut self, bounds: Rect) -> Vec2 {
        if !bounds.is_finite() {
            return Vec2::ZERO;
        }
        let old = Vec2::new(
            self.document.spec.width_px as f32,
            self.document.spec.height_px as f32,
        );
        if bounds.min.x >= 0.
            && bounds.min.y >= 0.
            && bounds.max.x <= old.x
            && bounds.max.y <= old.y
        {
            return Vec2::ZERO;
        }
        // Amortize expansion without changing the page, zoom, or pen coordinates.
        let grow = |need: f32| {
            if need > 0. {
                (need.ceil() as usize).div_ceil(128) * 128
            } else {
                0
            }
        };
        let left = grow(-bounds.min.x);
        let top = grow(-bounds.min.y);
        let right = grow(bounds.max.x - old.x);
        let bottom = grow(bounds.max.y - old.y);
        let Some(w) = self
            .document
            .spec
            .width_px
            .checked_add(left)
            .and_then(|n| n.checked_add(right))
        else {
            return Vec2::ZERO;
        };
        let Some(h) = self
            .document
            .spec
            .height_px
            .checked_add(top)
            .and_then(|n| n.checked_add(bottom))
        else {
            return Vec2::ZERO;
        };
        if graphite_studio::limits::canvas_pixels(w, h).is_err() {
            return Vec2::ZERO;
        }
        let delta = Vec2::new(left as f32, top as f32);
        let old_layout = self.viewport.layout(self.workspace_view_size, old);
        let mut map = QuarterTurn::expand(old.x as usize, old.y as usize, w, h, left, top);
        self.translate_edit_workspace(&mut map, delta);
        self.translate_liquify_workspace(&mut map);
        if let Some(s) = &mut self.stroke_session {
            s.transaction.expand(&mut map);
            s.selection.quarter_turn(&map);
            for p in &mut s.points {
                p.x += delta.x;
                p.y += delta.y;
            }
            s.last_raw.x += delta.x;
            s.last_raw.y += delta.y;
            s.curve_cursor.x += delta.x;
            s.curve_cursor.y += delta.y;
            s.smoother.translate(delta);
        }
        if let Some((a, b)) = &mut self.shape_drag {
            for p in [a, b] {
                p.x += delta.x;
                p.y += delta.y;
            }
        }
        self.history.expand_document(&mut self.document, &mut map);
        self.viewport.unbounded = true;
        let new = Vec2::new(w as f32, h as f32);
        let layout = self.viewport.layout(self.workspace_view_size, new);
        let anchor_change = layout.sheet_min + layout.sheet_size * 0.5
            - old_layout.sheet_min
            - old_layout.sheet_size * 0.5
            + ViewportState::rotate_vector(delta + (old - new) * 0.5, self.viewport.rotation)
                * self.viewport.zoom;
        self.viewport.free_pan -= anchor_change;
        if let Some(canvas) = self.workspace_canvas {
            let center = canvas.center()
                + ViewportState::rotate_vector((new - old) * 0.5 - delta, self.viewport.rotation)
                    * self.viewport.zoom;
            self.workspace_canvas = Some(Rect::from_center_size(center, new * self.viewport.zoom));
        }
        self.renderer.reset();
        self.display_pyramid = None;
        self.fallback_texture = None;
        self.prepared_preview = None;
        self.sidebar_statistics.0 = 0;
        delta
    }

    pub(super) fn brush_extent(&self, input: PointerFrame) -> f32 {
        match self.settings.tool {
            ToolKind::Pencil => {
                let diameter = self
                    .tip_state
                    .effective_core_diameter_px(self.document.spec.dpi)
                    * self.settings.pencil_geometry_scale
                    * self.settings.pressure_width_scale(input.pressure);
                let c = PencilTipContact::from_state_with_sharpness(
                    diameter,
                    input.pressure,
                    input.tilt_deg.unwrap_or(self.settings.tilt_deg),
                    input.azimuth_deg.unwrap_or(self.settings.azimuth_deg),
                    self.settings.grade.formulation().core_hardness,
                    self.settings.tip_sharpness,
                );
                (c.rear_extension_px + c.cross_radius_px + c.point_radius_px + 4.).max(4.)
            }
            ToolKind::Eraser => {
                self.settings.eraser_diameter_mm * self.document.spec.dpi / 25.4 + 4.
            }
            ToolKind::Smudge => self.settings.smudge_size_px * 0.5 + 4.,
            ToolKind::Tissue => self.settings.tissue_size_px * 0.5 + 4.,
            ToolKind::Shapes => self.settings.shape_width_px * 2. + 4.,
            _ => self.settings.brush_size_px * std::f32::consts::FRAC_1_SQRT_2 + 4.,
        }
    }
}
