use super::*;
use crate::core::document::DirtyRect;
use crate::core::{
    liquify::{Mode, Settings, Warp},
    material::PixelDepositState,
};
use std::{
    collections::VecDeque,
    sync::mpsc::{Receiver, TryRecvError},
};
#[derive(Clone)]
struct Dab {
    center: Vec2,
    delta: Vec2,
    direction: Vec2,
    force: f32,
    dose: f32,
    settings: Settings,
}
type JobResult = Result<(Warp, Vec<(usize, PixelDepositState)>), String>;
pub(super) struct Session {
    warp: Option<Warp>,
    job: Option<Receiver<JobResult>>,
    queue: VecDeque<Dab>,
    transaction: EditTransaction,
    last: Option<Vec2>,
    direction: Vec2,
    velocity: Vec2,
    force: f32,
    held: bool,
    coast: f32,
    last_time: f64,
    pending_amount: Option<f32>,
    pending_reset: bool,
    last_rotation: Option<f32>,
}

impl Session {
    fn enqueue(&mut self, dab: Dab) {
        if dab.settings.mode == Mode::Push && dab.delta.length_sq() < 1e-12 {
            return;
        }
        if let Some(last) = self.queue.back_mut() {
            // Combine redundant straight pen samples, never corners, pressure
            // changes, distortion, or separate non-contiguous stroke segments.
            if dab.settings.mode == Mode::Push
                && last.settings.mode == Mode::Push
                && dab.settings.distortion == 0.
                && last.settings.distortion == 0.
                && dab.settings.size == last.settings.size
                && dab.settings.pressure == last.settings.pressure
                && (dab.force - last.force).abs() < 0.001
                && last.direction.dot(dab.direction) > 0.9999
                && (last.center + last.delta * 0.5 - (dab.center - dab.delta * 0.5)).length_sq()
                    < 0.001
                && (last.delta + dab.delta).length() <= dab.settings.size * 0.06
            {
                let start = last.center - last.delta * 0.5;
                last.delta += dab.delta;
                last.center = start + last.delta * 0.5;
                last.dose += dab.dose;
                return;
            }
        }
        self.queue.push_back(dab);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> GraphiteApp {
        let mut a = GraphiteApp::with_render_state(None);
        a.replace_document(CanvasSpec::from_physical("Liquify study", 50., 50., 120.));
        a.fit_requested = false;
        a.viewport.zoom = 1.;
        a.liquify.settings.size = 90.;
        a.document.add_layer();
        let w = a.document.spec.width_px;
        let h = a.document.spec.height_px;
        for y in 20..h - 20 {
            for x in 20..w - 20 {
                let color = if (x / 16 + y / 16) % 2 == 0 {
                    [0.05, 0.5, 0.7]
                } else {
                    [0.9, 0.25, 0.08]
                };
                a.document.surface.set_deposit_pixel(
                    y * w + x,
                    PixelDepositState {
                        graphite_mass: 0.8,
                        color_r_mass: 0.8 * color[0],
                        color_g_mass: 0.8 * color[1],
                        color_b_mass: 0.8 * color[2],
                        ..Default::default()
                    },
                );
            }
        }
        let active = a.document.active_layer_index();
        a.document.layers[active].vectors =
            std::sync::Arc::new(crate::core::vector::VectorLayer::raster_base(&a.document));
        a.document.mark_all_dirty();
        a.workspace_canvas = Some(Rect::from_min_size(
            Pos2::new(300., 200.),
            Vec2::splat(236.),
        ));
        a
    }
    fn pen(
        a: &mut GraphiteApp,
        ctx: &egui::Context,
        time: f64,
        p: Pos2,
        pressed: bool,
        released: bool,
    ) {
        let canvas = a.workspace_canvas.unwrap();
        let _ = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 800.))),
                ..Default::default()
            },
            |ui| {
                a.handle_drawing_input(
                    ui,
                    canvas,
                    PointerFrame {
                        position: Some(p),
                        primary_pressed: pressed,
                        primary_released: released,
                        primary_down: !released,
                        pressure: 0.7,
                        pressure_from_device: true,
                        ..Default::default()
                    },
                );
            },
        );
    }
    #[test]
    fn pausing_push_removes_stale_momentum() {
        let mut a = app();
        let ctx = egui::Context::default();
        a.begin_liquify();
        a.liquify.settings.momentum = 1.;
        pen(&mut a, &ctx, 0., Pos2::new(410., 310.), true, false);
        pen(&mut a, &ctx, 0.02, Pos2::new(420., 310.), false, false);
        a.flush_liquify();
        let initial = a.liquify.session.as_ref().unwrap().velocity.length();
        for j in 1..=20 {
            pen(
                &mut a,
                &ctx,
                0.02 + j as f64 * 0.02,
                Pos2::new(420., 310.),
                false,
                false,
            );
        }
        let session = a.liquify.session.as_ref().unwrap();
        assert!(session.velocity.length() < initial * 0.001);
        assert!(session.job.is_none() && session.queue.is_empty());
    }
    #[test]
    fn push_samples_coalesce_without_losing_corners_or_pressure() {
        let mut a = app();
        a.begin_liquify();
        let s = a.liquify.session.as_mut().unwrap();
        let make = |x, direction, force| Dab {
            center: Vec2::new(x, 64.),
            delta: direction * 2.,
            direction,
            force,
            dose: 0.1,
            settings: Settings {
                size: 200.,
                ..Default::default()
            },
        };
        s.enqueue(make(41., Vec2::X, 1.));
        s.enqueue(make(43., Vec2::X, 1.));
        assert_eq!(s.queue.len(), 1);
        assert_eq!(s.queue[0].delta, Vec2::new(4., 0.));
        assert_eq!(s.queue[0].center, Vec2::new(42., 64.));
        s.enqueue(make(45., Vec2::X, 0.5));
        assert_eq!(s.queue.len(), 2);
        s.enqueue(make(46., Vec2::Y, 0.5));
        assert_eq!(s.queue.len(), 3);
        s.enqueue(Dab {
            delta: Vec2::ZERO,
            ..make(46., Vec2::X, 1.)
        });
        assert_eq!(s.queue.len(), 3);
    }
    #[test]
    fn background_effects_cancel_apply_undo_layers_and_reopen() {
        for mode in Mode::ALL.into_iter().filter(|m| *m != Mode::Reconstruct) {
            let mut a = app();
            let ctx = egui::Context::default();
            let before = a.renderer.rgba8(&a.document);
            let paper = a.document.surface.current_height.clone();
            let vectors = a.document.layers[1].vectors.clone();
            a.begin_liquify();
            a.liquify.settings.mode = mode;
            pen(&mut a, &ctx, 0., Pos2::new(410., 310.), true, false);
            assert_eq!(
                a.liquify.session.as_ref().unwrap().job.is_some(),
                mode != Mode::Push,
                "Stationary Push should stay idle; other modes start a job"
            );
            pen(&mut a, &ctx, 0.1, Pos2::new(435., 326.), false, true);
            assert!(a.liquify.session.as_ref().unwrap().job.is_some());
            a.flush_liquify();
            assert!(
                a.renderer.rgba8(&a.document) != before,
                "{} had no effect",
                mode.label()
            );
            assert!(a.document.surface.current_height == paper);
            assert_eq!(a.document.layer_count(), 2);
            assert_eq!(a.document.layers[0].deposit.occupied_pixel_count(), 0);
            a.finish_liquify(false);
            assert!(a.renderer.rgba8(&a.document) == before);
            assert!(std::sync::Arc::ptr_eq(
                &vectors,
                &a.document.layers[1].vectors
            ));
            assert!(!a.history.can_undo());
            a.begin_liquify();
            pen(&mut a, &ctx, 1., Pos2::new(410., 310.), true, false);
            pen(&mut a, &ctx, 1.1, Pos2::new(435., 326.), false, true);
            a.finish_liquify(true);
            let finished = a.renderer.rgba8(&a.document);
            assert!(finished != before);
            assert!(a.history.undo(&mut a.document));
            assert!(a.renderer.rgba8(&a.document) == before);
            assert!(std::sync::Arc::ptr_eq(
                &vectors,
                &a.document.layers[1].vectors
            ));
            assert!(a.history.redo(&mut a.document));
            assert!(a.renderer.rgba8(&a.document) == finished);
            for extension in ["graphite", "psd"] {
                let path = std::env::temp_dir().join(format!(
                    "graphite-liquify-{}-{}.{extension}",
                    std::process::id(),
                    mode as u32
                ));
                a.save_project_path(&path).unwrap();
                let reopened = graphite_studio::project::load(&path).unwrap();
                assert_eq!(reopened.document.layer_count(), 2);
                assert!(RasterRenderer::default().rgba8(&reopened.document) == finished);
                std::fs::remove_file(path).unwrap();
            }
        }
    }
    #[test]
    fn adjust_reset_and_outside_page_jobs_keep_history_consistent() {
        let mut a = app();
        let ctx = egui::Context::default();
        let original = a.renderer.rgba8(&a.document);
        a.begin_liquify();
        pen(&mut a, &ctx, 0., Pos2::new(410., 310.), true, false);
        pen(&mut a, &ctx, 0.1, Pos2::new(435., 310.), false, true);
        a.flush_liquify();
        let effect = a.renderer.rgba8(&a.document);
        assert!(effect != original);
        a.liquify.session.as_mut().unwrap().pending_amount = Some(0.);
        a.dispatch_liquify(Some(ctx.clone()));
        assert!(a.liquify.session.as_ref().unwrap().job.is_some());
        a.flush_liquify();
        assert!(a.renderer.rgba8(&a.document) == original);
        a.liquify.session.as_mut().unwrap().pending_amount = Some(1.);
        a.flush_liquify();
        assert!(a.renderer.rgba8(&a.document) == effect);
        a.liquify.session.as_mut().unwrap().pending_reset = true;
        a.flush_liquify();
        assert!(a.renderer.rgba8(&a.document) == original);
        assert!(!a.history.can_undo());
        pen(&mut a, &ctx, 1., Pos2::new(330., 290.), true, false);
        pen(&mut a, &ctx, 1.1, Pos2::new(280., 290.), false, true);
        a.finish_liquify(true);
        assert!(a.document.page.is_some());
        let [x, y, _, _] = a.document.page_bounds();
        assert!((y..y + 236).any(|j| (0..x).any(|i| a
            .document
            .surface
            .total_deposit(a.document.index(i, j))
            > 0.)));
        assert!(a.history.undo(&mut a.document));
        let rgba = a.renderer.rgba8(&a.document);
        let w = a.document.spec.width_px;
        for j in 0..236 {
            for i in 0..236 {
                assert_eq!(
                    &rgba[((j + y) * w + i + x) * 4..((j + y) * w + i + x) * 4 + 4],
                    &original[(j * 236 + i) * 4..(j * 236 + i) * 4 + 4]
                );
            }
        }
    }
    #[test]
    fn momentum_continues_only_when_enabled_and_stops_on_cancel() {
        for momentum in [0., 0.7] {
            let mut a = app();
            let ctx = egui::Context::default();
            a.begin_liquify();
            a.liquify.settings.momentum = momentum;
            pen(&mut a, &ctx, 0., Pos2::new(410., 310.), true, false);
            pen(&mut a, &ctx, 0.1, Pos2::new(420., 310.), false, true);
            a.flush_liquify();
            let before = a.renderer.rgba8(&a.document);
            for i in 1..5 {
                let canvas = a.workspace_canvas.unwrap();
                let _ = ctx.run_ui(
                    egui::RawInput {
                        time: Some(0.1 + i as f64 / 60.),
                        ..Default::default()
                    },
                    |ui| a.liquify_input(ui, canvas, PointerFrame::default()),
                );
                a.flush_liquify();
            }
            assert_eq!(a.renderer.rgba8(&a.document) != before, momentum > 0.);
            a.liquify_request_finish(false);
            a.poll_liquify(&ctx);
            assert!(a.liquify.session.is_none());
        }
    }
    #[test]
    fn fullscreen_icon_and_liquify_panel_render_and_modes_are_clickable() {
        let mut a = app();
        a.fullscreen = true;
        a.fit_requested = true;
        let ctx = egui::Context::default();
        let mut preview = crate::ui::test_render::Preview::default();
        let mut frame = |a: &mut GraphiteApp, events| {
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280., 900.))),
                    events,
                    ..Default::default()
                },
                |ui| a.interface(ui),
            );
            preview.update(&out);
            out
        };
        for _ in 0..3 {
            frame(&mut a, vec![]);
        }
        let icon = ctx.data(|d| {
            d.get_temp::<Rect>(egui::Id::new(("liquify_icon", 48u32)))
                .unwrap()
        });
        let transform = ctx.data(|d| {
            d.get_temp::<Rect>(egui::Id::new((
                "quick_tool",
                ToolKind::VectorSelect.label(),
            )))
            .unwrap()
        });
        assert!(icon.top() > transform.top() || icon.left() > transform.left());
        for pressed in [true, false] {
            frame(
                &mut a,
                vec![
                    egui::Event::PointerMoved(icon.center()),
                    egui::Event::PointerButton {
                        pos: icon.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(a.liquify.session.is_some());
        let before = a.renderer.rgba8(&a.document);
        for mode in Mode::ALL {
            frame(&mut a, vec![]);
            let rect = ctx.data(|d| {
                d.get_temp::<Rect>(egui::Id::new(("liquify_mode", mode.label())))
                    .unwrap()
            });
            for pressed in [true, false] {
                frame(
                    &mut a,
                    vec![
                        egui::Event::PointerMoved(rect.center()),
                        egui::Event::PointerButton {
                            pos: rect.center(),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
            assert_eq!(a.liquify.settings.mode, mode);
        }
        assert!(a.renderer.rgba8(&a.document) == before);
        assert!(!a.history.can_undo());
        a.liquify.settings.mode = Mode::TwirlRight;
        let settings = Settings {
            mode: Mode::TwirlRight,
            size: 150.,
            pressure: 0.9,
            ..Default::default()
        };
        for _ in 0..18 {
            a.liquify.session.as_mut().unwrap().queue.push_back(Dab {
                center: Vec2::splat(125.),
                delta: Vec2::ZERO,
                direction: Vec2::X,
                force: 1.,
                dose: 1.,
                settings: settings.clone(),
            });
        }
        a.flush_liquify();
        let out = frame(&mut a, vec![]);
        preview.save(&ctx, &out, "../../ui-v2414-liquify.png", [1280, 900]);
    }
}
#[derive(Default)]
pub(super) struct LiquifyUi {
    pub session: Option<Session>,
    pub settings: Settings,
    rect: Option<Rect>,
    pointer_blocked: bool,
    adjust: bool,
    amount: f32,
    finish: Option<bool>,
}
impl GraphiteApp {
    pub(super) fn liquify_controls_cover(&self, p: Pos2) -> bool {
        self.liquify.session.is_some() && self.liquify.rect.is_some_and(|r| r.contains(p))
    }
    pub(super) fn pause_liquify(&mut self) {
        if let Some(s) = &mut self.liquify.session {
            s.held = false;
            s.coast = 0.;
        }
    }
    pub(super) fn liquify_request_finish(&mut self, commit: bool) {
        self.liquify.finish = Some(commit);
        if let Some(s) = &mut self.liquify.session {
            s.held = false;
            s.coast = 0.;
            if !commit {
                s.queue.clear();
            }
        }
    }
    pub(super) fn begin_liquify(&mut self) {
        if self.liquify.session.is_some() {
            return;
        }
        self.finish_stroke();
        self.cancel_transform();
        self.quick_controls.close_size();
        self.shape_drag = None;
        self.editing.selected_path = None;
        self.rotate_view = false;
        self.liquify.amount = 1.;
        self.liquify.finish = None;
        self.liquify.session = Some(Session {
            warp: Some(Warp::new(&self.document)),
            job: None,
            queue: VecDeque::new(),
            transaction: Default::default(),
            last: None,
            direction: Vec2::X,
            velocity: Vec2::ZERO,
            force: 1.,
            held: false,
            coast: 0.,
            last_time: 0.,
            pending_amount: None,
            pending_reset: false,
            last_rotation: None,
        });
        self.status =
            "Liquify the active layer. Apply keeps the result; Cancel restores it.".into();
    }
    pub(super) fn translate_liquify_workspace(
        &mut self,
        map: &mut crate::core::orientation::QuarterTurn,
    ) {
        if let Some(s) = &mut self.liquify.session {
            assert!(s.job.is_none(), "Only expand between deformation batches");
            if let Some(w) = &mut s.warp {
                w.expand(map);
            }
            s.transaction.expand(map);
        }
    }
    fn accept_liquify_job(&mut self, result: JobResult) {
        match result {
            Ok((warp, updates)) => {
                let s = self.liquify.session.as_mut().unwrap();
                s.job = None;
                s.warp = Some(warp);
                let mut dirty: Option<DirtyRect> = None;
                for (i, value) in updates {
                    if value != self.document.surface.deposit_pixel(i) {
                        s.transaction.remember(i, &self.document);
                        self.document.surface.set_deposit_pixel(i, value);
                        let x = i % self.document.spec.width_px;
                        let y = i / self.document.spec.width_px;
                        let r = DirtyRect::new(x, y, x + 1, y + 1);
                        dirty = Some(dirty.map_or(r, |v| v.union(r)));
                    }
                }
                if let Some(r) = dirty {
                    self.document.mark_dirty(r);
                }
            }
            Err(e) => {
                if let Some(s) = self.liquify.session.take() {
                    s.transaction.rollback(&mut self.document);
                }
                self.status = format!("Liquify cancelled: {e}");
            }
        }
    }
    fn dispatch_liquify(&mut self, ctx: Option<egui::Context>) {
        let Some(s) = &mut self.liquify.session else {
            return;
        };
        if s.job.is_some() {
            return;
        }
        if s.pending_reset {
            std::mem::take(&mut s.transaction).rollback(&mut self.document);
            let warp = s.warp.as_mut().unwrap();
            warp.offsets.clear();
            warp.amount = 1.;
            s.queue.clear();
            s.pending_reset = false;
            s.pending_amount = None;
            self.liquify.amount = 1.;
            self.document.mark_all_dirty();
        }
        let amount = s.pending_amount.take();
        if s.queue.is_empty() && amount.is_none() {
            return;
        }
        // Large brushes publish smaller batches so a long queue cannot withhold
        // visual feedback while eight expensive full footprints are evaluated.
        let batch = if s.queue.front().is_some_and(|d| d.settings.size >= 256.) {
            2
        } else {
            8
        };
        let dabs: Vec<_> = s.queue.drain(..s.queue.len().min(batch)).collect();
        let mut bounds = Rect::NOTHING;
        for dab in &dabs {
            bounds = bounds.union(Rect::from_center_size(
                (dab.center + self.document.page_origin()).to_pos2(),
                Vec2::splat(dab.settings.size + 8.),
            ));
        }
        self.grow_workspace(bounds);
        let origin = self.document.page_origin();
        let selection = self.document.selection.clone();
        let s = self.liquify.session.as_mut().unwrap();
        let mut warp = s.warp.take().unwrap();
        let (send, recv) = std::sync::mpsc::channel();
        s.job = Some(recv);
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Preserve every stroke segment but sample the original only once
                // for this presented batch. This also removes duplicate UI patches.
                let mut changed = Vec::new();
                if let Some(amount) = amount {
                    warp.amount = amount.clamp(0., 1.);
                    changed.extend(warp.offsets.keys().copied());
                }
                for d in dabs {
                    changed.extend(warp.deform(
                        &selection,
                        d.center + origin,
                        d.delta,
                        d.direction,
                        d.force,
                        d.dose,
                        &d.settings,
                    ));
                }
                changed.sort_unstable();
                changed.dedup();
                let patches = warp.resample(&changed);
                (warp, patches)
            }))
            .map_err(|_| {
                "The deformation worker failed; original artwork was restored.".to_owned()
            });
            let _ = send.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }
    pub(super) fn poll_liquify(&mut self, ctx: &egui::Context) {
        let result = self
            .liquify
            .session
            .as_ref()
            .and_then(|s| s.job.as_ref())
            .and_then(|r| match r.try_recv() {
                Ok(v) => Some(v),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("Worker disconnected".into())),
            });
        if let Some(r) = result {
            self.accept_liquify_job(r);
        }
        self.dispatch_liquify(Some(ctx.clone()));
        if self
            .liquify
            .session
            .as_ref()
            .is_some_and(|s| s.job.is_some() || !s.queue.is_empty())
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(8));
        }
        if self.liquify.finish.is_some()
            && self
                .liquify
                .session
                .as_ref()
                .is_some_and(|s| s.job.is_none() && s.queue.is_empty())
        {
            let commit = self.liquify.finish.take().unwrap();
            self.finish_liquify(commit);
        }
    }
    // Explicit save/tool/layer changes finish queued work before changing its document.
    // During drawing and Apply/Cancel, polling above keeps the UI nonblocking.
    fn flush_liquify(&mut self) {
        loop {
            self.dispatch_liquify(None);
            let result = self
                .liquify
                .session
                .as_ref()
                .and_then(|s| s.job.as_ref())
                .map(|r| {
                    r.recv()
                        .unwrap_or_else(|_| Err("Worker disconnected".into()))
                });
            if let Some(result) = result {
                self.accept_liquify_job(result);
            } else {
                break;
            }
        }
    }
    pub(super) fn finish_liquify(&mut self, commit: bool) {
        if self.liquify.session.is_none() {
            return;
        }
        if let Some(s) = &mut self.liquify.session {
            s.coast = 0.;
            s.held = false;
            if !commit {
                s.queue.clear();
            }
        }
        self.flush_liquify();
        let Some(s) = self.liquify.session.take() else {
            return;
        };
        let changed = s.warp.as_ref().is_some_and(|w| {
            w.offsets
                .keys()
                .any(|&i| self.document.surface.deposit_pixel(i) != w.original.get(i))
        });
        if commit && changed {
            let layer = self.document.active_layer_index();
            self.document.layers[layer].vectors = std::sync::Arc::new(
                crate::core::vector::VectorLayer::raster_base(&self.document),
            );
            self.history.push(s.transaction, &self.document);
            self.tabs[self.active_tab].modified = true;
            self.status =
                "Liquify applied. Undo restores the original material and editable paths.".into();
        } else {
            s.transaction.rollback(&mut self.document);
            self.status = "Liquify closed; original artwork retained.".into();
        }
        self.liquify.rect = None;
        self.liquify.finish = None;
        self.liquify.pointer_blocked = false;
    }
    pub(super) fn show_liquify(&mut self, ctx: &egui::Context) {
        if self.liquify.session.is_none() {
            self.liquify.rect = None;
            return;
        }
        let mut open = true;
        let mut reset = false;
        let mut apply = false;
        let mut cancel = false;
        let mut amount_changed = false;
        let out=egui::Window::new("Liquify").id(egui::Id::new("liquify_controls")).open(&mut open).default_pos(Pos2::new(90.,140.)).default_width(260.).resizable(false).fade_in(false).fade_out(false).show(ctx,|ui| {
            ui.strong(self.document.active_layer_name());
            egui::Grid::new("liquify_modes").num_columns(2).spacing([8.,6.]).show(ui,|ui| {
                for (i,mode) in Mode::ALL.into_iter().enumerate() {
                    let r=ui.add_sized([118.,32.],egui::Button::selectable(self.liquify.settings.mode==mode,mode.label()));
                    #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new(("liquify_mode",mode.label())),r.rect));
                    if r.clicked(){self.liquify.settings.mode=mode;if let Some(s)=&mut self.liquify.session{s.coast=0.;}}
                    if i%2==1 {ui.end_row();}
                }
            });
            ui.separator();
            ui.add(egui::Slider::new(&mut self.liquify.settings.size,2.0..=1200.0).logarithmic(true).text("Size").suffix(" px"));
            ui.add(egui::Slider::new(&mut self.liquify.settings.pressure,0.0..=1.0).text("Pressure"));
            ui.add(egui::Slider::new(&mut self.liquify.settings.distortion,0.0..=1.0).text("Distortion"));
            ui.add(egui::Slider::new(&mut self.liquify.settings.momentum,0.0..=1.0).text("Momentum"));
            ui.horizontal(|ui|{if ui.add_sized([118.,32.],egui::Button::selectable(self.liquify.adjust,"Adjust")).clicked(){self.liquify.adjust=!self.liquify.adjust;}reset=ui.add_sized([118.,32.],egui::Button::new("Reset")).clicked();});
            if self.liquify.adjust {amount_changed=ui.add(egui::Slider::new(&mut self.liquify.amount,0.0..=1.0).text("Amount")).changed();}
            ui.small("Warps this layer's pixels. Other layers stay separate. Undo restores editable paths.");
            ui.horizontal(|ui|{apply=ui.add_sized([118.,36.],egui::Button::new("Apply")).clicked();cancel=ui.add_sized([118.,36.],egui::Button::new("Cancel")).clicked();});
            if self.liquify.session.as_ref().is_some_and(|s|s.job.is_some()){ui.spinner();}
        });
        self.liquify.rect = out.map(|o| o.response.rect);
        if reset || amount_changed {
            if let Some(s) = &mut self.liquify.session {
                s.coast = 0.;
                s.held = false;
                if reset {
                    s.pending_reset = true;
                    s.queue.clear();
                } else {
                    s.pending_amount = Some(self.liquify.amount);
                }
            }
        }
        if !open || cancel || apply {
            self.liquify.finish = Some(apply);
            if let Some(s) = &mut self.liquify.session {
                s.coast = 0.;
                s.held = false;
                if !apply {
                    s.queue.clear();
                }
            }
        }
    }
    pub(super) fn liquify_input(&mut self, ui: &egui::Ui, canvas: Rect, input: PointerFrame) {
        if self.liquify.finish.is_some() {
            return;
        }
        let over = input
            .position
            .is_some_and(|p| self.liquify.rect.is_some_and(|r| r.contains(p)));
        if over && input.primary_pressed {
            self.liquify.pointer_blocked = true;
        }
        let blocked =
            over || self.liquify.pointer_blocked || input.wants_pan() || input.touch_navigation;
        let now = ui.input(|i| i.time);
        let origin = self.document.page_origin();
        let Some(s) = &mut self.liquify.session else {
            return;
        };
        let dt = (now - s.last_time).clamp(0., 0.05) as f32;
        s.last_time = now;
        if !ui.input(|i| i.focused) {
            s.held = false;
            s.coast = 0.;
        }
        let mut settings = self.liquify.settings.clone();
        if !blocked {
            if let Some(pos) = input.position.filter(|p| ui.clip_rect().contains(*p)) {
                let point = self.viewport.screen_to_document(canvas, pos) - origin;
                if input.primary_pressed {
                    s.last = Some(point);
                    s.held = true;
                    s.coast = 0.;
                    s.velocity = Vec2::ZERO;
                    s.last_rotation = input.rotation_deg;
                }
                if s.held && (input.primary_down || input.primary_pressed || input.primary_released)
                {
                    let previous = s.last.unwrap_or(point);
                    let delta = point - previous;
                    let roll = if matches!(settings.mode, Mode::TwirlLeft | Mode::TwirlRight) {
                        match (s.last_rotation, input.rotation_deg) {
                            (Some(a), Some(b)) => (b - a + 180.).rem_euclid(360.) - 180.,
                            _ => 0.,
                        }
                    } else {
                        0.
                    };
                    if let Some(rotation) = input.rotation_deg {
                        s.last_rotation = Some(rotation);
                    }
                    if roll.abs() > 0.01 {
                        settings.mode = if roll > 0. {
                            Mode::TwirlRight
                        } else {
                            Mode::TwirlLeft
                        };
                    }
                    if delta.length_sq() > 1e-6 {
                        s.direction = delta.normalized();
                        s.velocity = delta / dt.max(1. / 240.);
                    } else {
                        // A pause before lifting should stop the throw, instead
                        // of reusing the velocity of a much earlier movement.
                        s.velocity *= (-dt * 25.).exp();
                    }
                    if !input.primary_released {
                        s.force = if input.pressure_from_device {
                            input.pressure
                        } else {
                            1.
                        };
                    }
                    let steps = (delta.length() / (settings.size * 0.12).max(1.))
                        .ceil()
                        .max(1.) as usize;
                    for i in 1..=steps {
                        if settings.mode == Mode::Push && delta.length_sq() < 1e-12 {
                            break;
                        }
                        s.enqueue(Dab {
                            center: previous
                                + delta
                                    * ((i as f32
                                        - if settings.mode == Mode::Push { 0.5 } else { 0. })
                                        / steps as f32),
                            delta: delta / steps as f32,
                            direction: s.direction,
                            force: s.force,
                            dose: (dt * 30.
                                + delta.length() / settings.size.max(1.) * 2.
                                + roll.abs() / 30.)
                                .max(if input.primary_pressed { 0.25 } else { 0. })
                                / steps as f32,
                            settings: settings.clone(),
                        });
                    }
                    s.last = Some(point);
                }
            }
            if s.held
                && settings.mode != Mode::Push
                && input.position.is_none()
                && !input.primary_released
            {
                s.velocity *= (-dt * 25.).exp();
                if let Some(point) = s.last {
                    s.enqueue(Dab {
                        center: point,
                        delta: Vec2::ZERO,
                        direction: s.direction,
                        force: s.force,
                        dose: dt * 30.,
                        settings: settings.clone(),
                    });
                }
            }
        }
        if s.held && settings.mode == Mode::Push && input.position.is_none() {
            s.velocity *= (-dt * 25.).exp();
        }
        if input.primary_released {
            s.held = false;
            s.coast = if blocked { 0. } else { settings.momentum };
            self.liquify.pointer_blocked = false;
        }
        if !s.held && s.coast > 0.005 && !blocked {
            let decay = 3. + (1. - settings.momentum) * 12.;
            let next_coast = s.coast * (-dt * decay).exp();
            let integrated = (s.coast - next_coast) / decay;
            let delta = s.velocity * integrated;
            let point = s.last.unwrap_or_default() + delta;
            s.enqueue(Dab {
                center: point,
                delta,
                direction: s.direction,
                force: s.force,
                dose: integrated * 30.,
                settings: settings.clone(),
            });
            s.last = Some(point);
            s.coast = next_coast;
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        if s.held {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        self.dispatch_liquify(Some(ui.ctx().clone()));
    }
}
