use eframe::egui::Vec2;

#[derive(Default)]
pub struct RotationGesture(pub f32);
impl RotationGesture {
    pub fn advance(&mut self, delta: f32, snap: bool) -> f32 {
        self.0 += delta;
        if snap {
            (self.0 / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2
        } else {
            self.0
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CanvasLayout {
    /// Total scrollable workspace size in content coordinates.
    pub content_size: Vec2,
    /// Zoomed paper size in screen points.
    pub sheet_size: Vec2,
    /// Paper top-left in scroll-content coordinates.
    pub sheet_min: Vec2,
    /// Whether the paper itself is larger than the visible viewport on each axis.
    pub overflow: [bool; 2],
}

#[derive(Debug, Clone)]
pub struct ViewportState {
    pub unbounded: bool,
    pub zoom: f32,
    /// View-only rotation; document pixels and path coordinates remain unchanged.
    pub rotation: f32,
    /// Free sheet translation on axes where the full paper fits in the viewport. Once an axis
    /// overflows, the native ScrollArea offset becomes the source of truth for navigation there.
    pub free_pan: Vec2,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            unbounded: false,
            zoom: 1.0,
            rotation: 0.,
            free_pan: Vec2::ZERO,
        }
    }
}

impl ViewportState {
    pub fn rotate_vector(v: Vec2, angle: f32) -> Vec2 {
        let (s, c) = angle.sin_cos();
        Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
    }

    fn rotated_size(&self, size: Vec2) -> Vec2 {
        let (s, c) = self.rotation.sin_cos();
        Vec2::new(
            size.x * c.abs() + size.y * s.abs(),
            size.x * s.abs() + size.y * c.abs(),
        )
    }

    /// `canvas` is the unrotated sheet rectangle, centered within its rotated bounds.
    pub fn document_to_screen(&self, canvas: eframe::egui::Rect, p: Vec2) -> eframe::egui::Pos2 {
        canvas.center() + Self::rotate_vector(p * self.zoom - canvas.size() * 0.5, self.rotation)
    }

    pub fn screen_to_document(&self, canvas: eframe::egui::Rect, p: eframe::egui::Pos2) -> Vec2 {
        (Self::rotate_vector(p - canvas.center(), -self.rotation) + canvas.size() * 0.5)
            / self.zoom.max(1e-6)
    }

    pub fn contains(&self, canvas: eframe::egui::Rect, p: eframe::egui::Pos2) -> bool {
        let local = self.screen_to_document(canvas, p);
        local.x >= 0.
            && local.y >= 0.
            && local.x <= canvas.width() / self.zoom
            && local.y <= canvas.height() / self.zoom
    }

    pub const MIN_ZOOM: f32 = 0.03;
    pub const MAX_ZOOM: f32 = 32.0;
    pub const WORKSPACE_MARGIN: f32 = 28.0;

    pub fn fit(&mut self, view_size: Vec2, doc_size_px: Vec2) {
        if doc_size_px.x <= 0.0 || doc_size_px.y <= 0.0 {
            return;
        }
        let usable = Vec2::new(
            (view_size.x - Self::WORKSPACE_MARGIN * 2.0).max(32.0),
            (view_size.y - Self::WORKSPACE_MARGIN * 2.0).max(32.0),
        );
        let bounds = self.rotated_size(doc_size_px);
        let fit_x = usable.x / bounds.x;
        let fit_y = usable.y / bounds.y;
        self.zoom = fit_x.min(fit_y).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.free_pan = Vec2::ZERO;
    }

    pub fn layout(&self, view_size: Vec2, doc_size_px: Vec2) -> CanvasLayout {
        let sheet_size = self.rotated_size(doc_size_px) * self.zoom;
        let overflow = [sheet_size.x > view_size.x, sheet_size.y > view_size.y];

        let content_x = if overflow[0] {
            sheet_size.x + Self::WORKSPACE_MARGIN * 2.0
        } else {
            view_size.x
        };
        let content_y = if overflow[1] {
            sheet_size.y + Self::WORKSPACE_MARGIN * 2.0
        } else {
            view_size.y
        };

        let sheet_x = if overflow[0] {
            Self::WORKSPACE_MARGIN + if self.unbounded {self.free_pan.x} else {0.}
        } else {
            (view_size.x - sheet_size.x) * 0.5 + self.free_pan.x
        };
        let sheet_y = if overflow[1] {
            Self::WORKSPACE_MARGIN + if self.unbounded {self.free_pan.y} else {0.}
        } else {
            (view_size.y - sheet_size.y) * 0.5 + self.free_pan.y
        };

        CanvasLayout {
            content_size: Vec2::new(content_x.max(1.0), content_y.max(1.0)),
            sheet_size,
            sheet_min: Vec2::new(sheet_x, sheet_y),
            overflow,
        }
    }

    /// Pan the paper directly on axes where it fits entirely in the viewport. Overflowing axes
    /// are panned by the surrounding ScrollArea, so they are intentionally ignored here.
    pub fn pan_fitting_axes(&mut self, delta: Vec2, layout: CanvasLayout) {
        if self.unbounded || !layout.overflow[0] {
            self.free_pan.x += delta.x;
        }
        if self.unbounded || !layout.overflow[1] {
            self.free_pan.y += delta.y;
        }
    }

    /// Zoom while keeping the document coordinate under the cursor as stable as possible.
    ///
    /// Returns a content-motion delta suitable for `Ui::scroll_with_delta`. Native scroll state
    /// owns overflowing axes; `free_pan` owns axes on which the sheet still fits.
    pub fn zoom_at(
        &mut self,
        cursor_in_view: Vec2,
        current_scroll_offset: Vec2,
        view_size: Vec2,
        doc_size_px: Vec2,
        factor: f32,
    ) -> Vec2 {
        self.gesture_at(
            cursor_in_view,
            Vec2::ZERO,
            current_scroll_offset,
            view_size,
            doc_size_px,
            factor,
        )
    }

    /// Move and scale the sheet together, keeping the same document point under
    /// the moving finger center. Clamping happens after both operations.
    pub fn gesture_at(
        &mut self,
        cursor_in_view: Vec2,
        translation: Vec2,
        current_scroll_offset: Vec2,
        view_size: Vec2,
        doc_size_px: Vec2,
        factor: f32,
    ) -> Vec2 {
        self.gesture_rotate_at(
            cursor_in_view,
            translation,
            current_scroll_offset,
            view_size,
            doc_size_px,
            factor,
            0.,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn gesture_rotate_at(
        &mut self,
        cursor_in_view: Vec2,
        translation: Vec2,
        current_scroll_offset: Vec2,
        view_size: Vec2,
        doc_size_px: Vec2,
        factor: f32,
        rotation: f32,
    ) -> Vec2 {
        let old_zoom = self.zoom;
        let old_layout = self.layout(view_size, doc_size_px);
        let cursor_content = current_scroll_offset + cursor_in_view - translation;
        let doc_at_cursor = Self::rotate_vector(
            cursor_content - old_layout.sheet_min - old_layout.sheet_size * 0.5,
            -self.rotation,
        ) / old_zoom.max(1e-6);

        let new_zoom = (old_zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        if (new_zoom - old_zoom).abs() < f32::EPSILON && translation == Vec2::ZERO && rotation == 0.
        {
            return Vec2::ZERO;
        }
        self.zoom = new_zoom;
        self.rotation = (self.rotation + rotation + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;

        // Start from a centered sheet on any axis that transitioned from scrolling back to fit.
        let preliminary = self.layout(view_size, doc_size_px);
        let new_anchor = Self::rotate_vector(doc_at_cursor, self.rotation) * new_zoom
            + preliminary.sheet_size * 0.5;
        if self.unbounded {
            self.free_pan+=current_scroll_offset+cursor_in_view-preliminary.sheet_min-new_anchor;
            return Vec2::ZERO;
        }
        let mut desired_scroll = current_scroll_offset;

        if preliminary.overflow[0] {
            self.free_pan.x = 0.0;
        } else {
            desired_scroll.x = 0.0;
            let centered_x = (view_size.x - preliminary.sheet_size.x) * 0.5;
            self.free_pan.x = cursor_in_view.x - new_anchor.x - centered_x;
        }
        if preliminary.overflow[1] {
            self.free_pan.y = 0.0;
        } else {
            desired_scroll.y = 0.0;
            let centered_y = (view_size.y - preliminary.sheet_size.y) * 0.5;
            self.free_pan.y = cursor_in_view.y - new_anchor.y - centered_y;
        }

        let new_layout = self.layout(view_size, doc_size_px);
        if new_layout.overflow[0] {
            desired_scroll.x = (new_layout.sheet_min.x + new_anchor.x - cursor_in_view.x)
                .clamp(0.0, (new_layout.content_size.x - view_size.x).max(0.0));
        }
        if new_layout.overflow[1] {
            desired_scroll.y = (new_layout.sheet_min.y + new_anchor.y - cursor_in_view.y)
                .clamp(0.0, (new_layout.content_size.y - view_size.y).max(0.0));
        }

        // scroll_with_delta is expressed as content motion, opposite to positive scroll offset.
        current_scroll_offset - desired_scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snap_accumulates_small_movements_and_releases_without_losing_angle() {
        let mut gesture = RotationGesture::default();
        for degrees in 1..=360 {
            let result = gesture.advance(1f32.to_radians(), true);
            let quadrant = result / std::f32::consts::FRAC_PI_2;
            assert!((quadrant - quadrant.round()).abs() < 1e-5);
            if degrees % 90 == 0 {
                assert!((result.to_degrees() - degrees as f32).abs() < 0.001);
            }
        }
        let free = gesture.advance(12f32.to_radians(), false);
        assert!((free.to_degrees() - 372.).abs() < 0.002);
    }

    #[test]
    fn rotated_fit_mapping_and_combined_gestures_preserve_the_anchor() {
        use eframe::egui::{Pos2, Rect};
        let view = Vec2::new(900., 700.);
        let doc = Vec2::new(1000., 1400.);
        for angle in [0., 0.42, 1.57, -2.4] {
            let mut vp = ViewportState {
                rotation: angle,
                ..Default::default()
            };
            vp.fit(view, doc);
            let layout = vp.layout(view, doc);
            assert!(!layout.overflow[0] && !layout.overflow[1]);
            let canvas = Rect::from_center_size(
                (layout.sheet_min + layout.sheet_size * 0.5).to_pos2(),
                doc * vp.zoom,
            );
            for p in [Vec2::ZERO, doc, Vec2::new(135., 270.)] {
                let screen = vp.document_to_screen(canvas, p);
                assert!(Rect::from_min_size(Pos2::ZERO, view).contains(screen));
                assert!((vp.screen_to_document(canvas, screen) - p).length() < 0.001);
            }
            for zoom in [0.25, 0.55, 2.] {
                vp.zoom = zoom;
                vp.free_pan = Vec2::ZERO;
                let layout = vp.layout(view, doc);
                let offset = (layout.content_size - view) * 0.5;
                let canvas = Rect::from_center_size(
                    (layout.sheet_min + layout.sheet_size * 0.5 - offset).to_pos2(),
                    doc * vp.zoom,
                );
                let center = Vec2::new(420., 330.);
                let point = vp.screen_to_document(canvas, center.to_pos2());
                let movement = Vec2::new(10., -8.);
                let motion = vp.gesture_rotate_at(
                    center + movement,
                    movement,
                    offset,
                    view,
                    doc,
                    1.13,
                    0.31,
                );
                let layout = vp.layout(view, doc);
                let canvas = Rect::from_center_size(
                    (layout.sheet_min + layout.sheet_size * 0.5 - offset + motion).to_pos2(),
                    doc * vp.zoom,
                );
                assert!(
                    (vp.document_to_screen(canvas, point).to_vec2() - center - movement).length()
                        < 0.002
                );
            }
        }
    }

    #[test]
    fn two_finger_pan_and_pinch_follow_the_moving_center() {
        let view = Vec2::new(800., 600.);
        let doc = Vec2::new(1200., 1600.);
        let previous_center = Vec2::new(430., 280.);
        let delta = Vec2::new(10., -10.);
        for initial_zoom in [0.25, 0.45, 1., 4., ViewportState::MAX_ZOOM] {
            for factor in [1., 1.5, 0.7] {
                let mut vp = ViewportState {
                    zoom: initial_zoom,
                    rotation: 0.,
                    free_pan: Vec2::ZERO,
                    unbounded: false,
                };
                let offset = Vec2::new(
                    if initial_zoom >= 1. { 200. } else { 0. },
                    if initial_zoom >= 1. {
                        300.
                    } else if initial_zoom >= 0.45 {
                        60.
                    } else {
                        0.
                    },
                );
                let before = (offset + previous_center - vp.layout(view, doc).sheet_min) / vp.zoom;
                let motion =
                    vp.gesture_at(previous_center + delta, delta, offset, view, doc, factor);
                let after = (offset - motion + previous_center + delta
                    - vp.layout(view, doc).sheet_min)
                    / vp.zoom;
                assert!(
                    (after - before).length() < 0.001,
                    "zoom {initial_zoom}, factor {factor}: {before:?} -> {after:?}"
                );
                if factor == 1. {
                    assert_eq!(vp.zoom, initial_zoom);
                }
            }
        }
    }

    #[test]
    fn pinch_anchor_stays_on_the_same_document_pixel() {
        let view = Vec2::new(800., 600.);
        let doc = Vec2::new(1200., 1600.);
        let cursor = Vec2::new(370., 280.);
        for initial_zoom in [0.25, 1.0, 4.0] {
            let mut vp = ViewportState {
                zoom: initial_zoom,
                rotation: 0.,
                free_pan: Vec2::ZERO,
                unbounded: false,
            };
            let offset = if initial_zoom >= 1.0 {
                Vec2::new(200., 300.)
            } else {
                Vec2::ZERO
            };
            let before = (offset + cursor - vp.layout(view, doc).sheet_min) / vp.zoom;
            let motion = vp.zoom_at(cursor, offset, view, doc, 1.5);
            let after = (offset - motion + cursor - vp.layout(view, doc).sheet_min) / vp.zoom;
            assert!((after - before).length() < 0.001);
        }
    }

    #[test]
    fn scroll_content_only_grows_when_sheet_overflows() {
        let mut viewport = ViewportState::default();
        viewport.zoom = 0.5;
        let view = Vec2::new(1000.0, 700.0);
        let doc = Vec2::new(1000.0, 1000.0);
        let layout = viewport.layout(view, doc);
        assert_eq!(layout.content_size, view);
        assert!(!layout.overflow[0] && !layout.overflow[1]);

        viewport.zoom = 1.2;
        let layout = viewport.layout(view, doc);
        assert!(layout.overflow[0] && layout.overflow[1]);
        assert!(layout.content_size.x > view.x);
        assert!(layout.content_size.y > view.y);
    }

    #[test]
    fn fit_resets_free_pan() {
        let mut viewport = ViewportState {
            zoom: 2.0,
            rotation: 0.,
            free_pan: Vec2::new(100.0, -80.0),
            unbounded: false,
        };
        viewport.fit(Vec2::new(1000.0, 700.0), Vec2::new(800.0, 1200.0));
        assert_eq!(viewport.free_pan, Vec2::ZERO);
        assert!(viewport.zoom < 1.0);
    }
}
