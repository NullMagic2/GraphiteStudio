use super::*;

const HOLD_SECONDS: f64 = 0.65;

pub(super) struct QuickLine {
    anchor: Vec2,
    still_since: f64,
    endpoint: StrokePoint,
    template: Option<Vec<(StrokePoint, f32)>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> GraphiteApp {
        let mut a = GraphiteApp::with_render_state(None);
        a.replace_document(CanvasSpec::from_physical("Quick line", 50., 50., 120.));
        a.viewport.zoom = 1.;
        a
    }

    fn frame(a: &mut GraphiteApp, ctx: &egui::Context, time: f64, input: PointerFrame) {
        let _ = ctx.run_ui(egui::RawInput { time: Some(time), ..Default::default() }, |ui| {
            a.handle_drawing_input(ui, Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.)), input);
        });
    }
    fn point(x: f32, y: f32, pressed: bool) -> PointerFrame {
        PointerFrame { position: Some(Pos2::new(x, y)), primary_pressed: pressed,
            primary_down: true, pressure: 0.6, pressure_from_device: true,
            tilt_deg: Some(8.), ..Default::default() }
    }

    #[test]
    fn hold_straightens_without_new_packets_endpoint_moves_and_one_undo_restores_artwork() {
        let mut a = app();
        let ctx = egui::Context::default();
        let before = a.renderer.rgba8(&a.document);
        a.settings.tool = ToolKind::Pencil;
        a.settings.shape_opacity = 0.; // Shapes opacity must not hide a straightened pencil.
        frame(&mut a, &ctx, 0., point(40., 40., true));
        frame(&mut a, &ctx, 0.1, point(80., 110., false));
        frame(&mut a, &ctx, 0.2, point(170., 80., false));
        frame(&mut a, &ctx, 0.7, PointerFrame::default());
        assert!(!a.stroke_session.as_ref().unwrap().quick_line.snapped());
        frame(&mut a, &ctx, 0.9, PointerFrame::default());
        assert!(a.stroke_session.as_ref().unwrap().quick_line.snapped());
        frame(&mut a, &ctx, 1., point(180., 40., false));
        let s = a.stroke_session.as_ref().unwrap();
        assert!(s.points.iter().all(|p| (p.y - 40.).abs() < 0.001));
        assert!((s.points.last().unwrap().x - 180.).abs() < 0.001);
        frame(&mut a, &ctx, 1.1, PointerFrame { primary_released: true, pressure: 0., ..Default::default() });
        let finished = a.renderer.rgba8(&a.document);
        assert_ne!(finished, before);
        assert_eq!(a.document.layers[0].vectors.strokes.len(), 1);
        assert!(a.document.layers[0].vectors.strokes[0].polyline);
        // Replay uses the identical material samples, with no curved-stroke residue.
        let record = a.document.layers[0].vectors.strokes[0].clone();
        assert!(a.history.undo(&mut a.document));
        assert_eq!(a.renderer.rgba8(&a.document), before);
        let mut replayed = a.document.clone();
        record.apply(&mut replayed, &mut EditTransaction::default());
        assert_eq!(RasterRenderer::default().rgba8(&replayed), finished);
        assert!(a.history.redo(&mut a.document));
        assert_eq!(a.renderer.rgba8(&a.document), finished);
        for extension in ["graphite", "psd"] {
            let path = std::path::PathBuf::from(format!("../../quick-line-v222.{extension}"));
            a.save_project_path(&path).unwrap();
            let loaded = graphite_studio::project::load(&path).unwrap();
            assert!(loaded.document.layers[0].vectors.strokes[0].polyline);
            assert_eq!(RasterRenderer::default().rgba8(&loaded.document), finished);
        }
    }

    #[test]
    fn movement_resets_hold_and_setting_can_disable_it() {
        for enabled in [false, true] {
            let mut a = app();
            a.settings.tool = ToolKind::Pencil;
            a.settings.hold_to_straighten = enabled;
            let ctx = egui::Context::default();
            frame(&mut a, &ctx, 0., point(40., 40., true));
            for n in 1..=4 {
                frame(&mut a, &ctx, n as f64 * 0.5, point(40. + n as f32 * 25., 80., false));
                assert!(!a.stroke_session.as_ref().unwrap().quick_line.snapped());
            }
            frame(&mut a, &ctx, 3., PointerFrame::default());
            assert_eq!(a.stroke_session.as_ref().unwrap().quick_line.snapped(), enabled);
        }
    }
}

impl QuickLine {
    pub(super) fn new(point: StrokePoint, now: f64) -> Self {
        Self { anchor: Vec2::new(point.x, point.y), still_since: now, endpoint: point, template: None }
    }
    pub(super) fn snapped(&self) -> bool { self.template.is_some() }
    pub(super) fn observe(&mut self, point: StrokePoint, now: f64, zoom: f32) {
        let position = Vec2::new(point.x, point.y);
        if (position - self.anchor).length() * zoom > 2.5 {
            self.anchor = position;
            self.still_since = now;
        }
        self.endpoint = point;
    }
}

impl GraphiteApp {
    pub(super) fn tick_quick_line(&mut self, ui: &egui::Ui) {
        let Some(s) = self.stroke_session.as_ref() else { return; };
        if s.settings.tool != ToolKind::Pencil || !s.settings.hold_to_straighten
            || s.quick_line.snapped() || !ui.input(|i| i.focused) { return; }
        let first = s.points[0];
        let end = s.quick_line.endpoint;
        if Vec2::new(end.x - first.x, end.y - first.y).length() * self.viewport.zoom < 12. { return; }
        let remaining = HOLD_SECONDS - (ui.input(|i| i.time) - s.quick_line.still_since);
        if remaining > 0. {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(remaining));
            return;
        }
        let s = self.stroke_session.as_mut().unwrap();
        // Save the original pressure/tilt variation along arc length, then place
        // those material samples along the straight chord instead of the curve.
        let mut template = vec![(first, 0.)];
        let mut previous = first;
        let mut distance = 0.;
        for point in s.points.iter().skip(1).copied().chain(std::iter::once(end)) {
            let step = Vec2::new(point.x - previous.x, point.y - previous.y).length();
            if step > 0.001 {
                distance += step;
                template.push((point, distance));
                previous = point;
            }
        }
        if distance <= 0.001 { return; }
        for (_, fraction) in &mut template { *fraction /= distance; }
        s.quick_line.template = Some(template);
        s.settings.shape_opacity = 1.; // Pencil opacity is independent of the Shapes control.
        self.render_quick_line();
        self.status = "Straight line: move the pencil to adjust the endpoint, then lift to place. Ctrl+Z undoes it.".into();
        ui.ctx().request_repaint();
    }

    pub(super) fn move_quick_line(&mut self, ui: &egui::Ui, canvas: Rect, input: PointerFrame) -> bool {
        if !self.stroke_session.as_ref().is_some_and(|s| s.quick_line.snapped()) { return false; }
        if input.wants_pan() { self.finish_stroke(); return true; }
        if input.primary_down || input.primary_released {
            if let Some(pos) = input.position {
                let point = self.viewport.screen_to_document(canvas, pos);
                let s = self.stroke_session.as_mut().unwrap();
                let old = s.quick_line.endpoint;
                if Vec2::new(point.x - old.x, point.y - old.y).length() > 0.01 {
                    s.quick_line.endpoint.x = point.x;
                    s.quick_line.endpoint.y = point.y;
                    self.render_quick_line();
                    ui.ctx().request_repaint();
                }
            }
        }
        if input.primary_released { self.finish_stroke(); ui.ctx().request_repaint(); }
        true
    }

    fn render_quick_line(&mut self) {
        let s = self.stroke_session.as_mut().unwrap();
        std::mem::take(&mut s.transaction).rollback(&mut self.document);
        self.tip_state = s.initial_tip.clone();
        self.stroke_engine = s.initial_engine.clone();
        let template = s.quick_line.template.as_ref().unwrap();
        let first = template[0].0;
        let end = s.quick_line.endpoint;
        s.points = template.iter().map(|&(mut p, t)| {
            p.x = first.x + (end.x - first.x) * t;
            p.y = first.y + (end.y - first.y) * t;
            p
        }).collect();
        self.stroke_engine.begin_pencil_stroke(s.points[0]);
        for pair in s.points.windows(2) {
            self.stroke_engine.apply_segment(&mut self.document, &s.settings, &mut self.tip_state, pair[0], pair[1], &mut s.transaction);
        }
        s.last_raw = *s.points.last().unwrap();
        s.curve_cursor = s.last_raw;
    }
}
