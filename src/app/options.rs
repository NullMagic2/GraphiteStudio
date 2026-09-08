use super::*;
use crate::performance::{AccelerationMode, Preferences};

impl GraphiteApp {
    pub(super) fn save_preferences(&self,acceleration:Option<AccelerationMode>)->Result<(),String> {
        // Headless test instances opt into a scratch file; production sets this at startup.
        let Some(path)=&self.preferences_path else {return Ok(());};
        let mut preferences=Preferences::load_from(path).unwrap_or_default();
        preferences.recent_files=self.recent_files.clone();
        if let Some(mode)=acceleration {preferences.acceleration=mode;}
        preferences.save_to(path)
    }
    pub(super) fn set_recent_files_maximum(&mut self,maximum:u8) {
        self.recent_files.set_maximum(maximum);
        self.preference_status=match self.save_preferences(None) {
            Ok(())=>if self.recent_files.maximum()==0 {"Recent files disabled and cleared.".into()} else {format!("Recent files: remembering up to {} files.",self.recent_files.maximum())},
            Err(e)=>format!("Recent files setting changed for this run, but could not be saved: {e}"),
        };
    }
    pub(super) fn remember_recent_file(&mut self,path:&std::path::Path) {
        if self.recent_files.remember(path) {
            if let Err(e)=self.save_preferences(None) {self.preference_status=format!("Could not save the recent files list: {e}");}
        }
    }
    pub(super) fn set_acceleration(&mut self, mode: AccelerationMode) {
        if self.acceleration == mode {
            return;
        }
        self.acceleration = mode;
        self.display_pyramid = None;
        self.fallback_texture = None;
        self.last_texture_update = None;
        self.preference_status = match self.save_preferences(Some(mode))
        {
            Ok(()) => {
                if mode == self.startup_acceleration {
                    format!("{} saved.", mode.label())
                } else {
                    format!("{} saved. Display settings applied; save your drawings and reopen the app to finish switching renderer / GPU.", mode.label())
                }
            }
            Err(e) => format!(
                "Display settings applied for this run, but could not save your preference: {e}"
            ),
        };
    }

    pub(super) fn apply_display_style(&self, ctx: &egui::Context) {
        let lite = self.acceleration == AccelerationMode::IntelHd;
        let id = egui::Id::new("graphite_display_style");
        if ctx.data(|data| data.get_temp::<bool>(id)) == Some(lite) {
            return;
        }
        ctx.set_theme(egui::Theme::Light);
        ctx.style_mut_of(egui::Theme::Light, |style| {
            style.animation_time = if lite { 0. } else { 0.15 };
            style.visuals = egui::Visuals::light();
            if lite {
                style.visuals.window_shadow = egui::epaint::Shadow::NONE;
                style.visuals.popup_shadow = egui::epaint::Shadow::NONE;
            }
        });
        ctx.data_mut(|data| data.insert_temp(id, lite));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_to_screen_button_recenters_zoom_without_changing_drawing() {
        let mut app = GraphiteApp::with_render_state(None);
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![]), |ui| app.interface(ui));
        app.viewport.zoom = 4.;
        app.viewport.free_pan = Vec2::new(300., -200.);
        let revision = app.document.revision();
        let output = ctx.run_ui(input(vec![]), |ui| app.interface(ui));
        let pos = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Fit to screen" => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                input(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]),
                |ui| app.interface(ui),
            );
        }
        assert!(app.viewport.zoom < 1.);
        assert_eq!(app.viewport.free_pan, Vec2::ZERO);
        assert_eq!(app.document.revision(), revision);
    }
    #[test]
    fn profiles_preserve_strokes_exports_and_flush_after_pen_up() {
        let mut expected = None;
        for mode in AccelerationMode::ALL {
            let mut app = GraphiteApp::with_render_state(None);
            app.replace_document(CanvasSpec::from_physical("Profile", 40., 40., 120.));
            app.acceleration = mode;
            let ctx = egui::Context::default();
            app.apply_display_style(&ctx);
            assert_eq!(
                ctx.style_of(egui::Theme::Light).animation_time == 0.,
                mode == AccelerationMode::IntelHd
            );
            let _ = ctx.run_ui(Default::default(), |ui| app.ensure_texture(ui.ctx()));
            let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(188.));
            app.viewport.zoom = 1.;
            for i in 0..20 {
                let _ = ctx.run_ui(Default::default(), |ui| {
                    app.handle_drawing_input(
                        ui,
                        canvas,
                        PointerFrame {
                            position: Some(Pos2::new(
                                20. + i as f32 * 5.,
                                60. + (i as f32 * 0.2).sin() * 8.,
                            )),
                            primary_down: true,
                            primary_pressed: i == 0,
                            pressure: 0.5,
                            pressure_from_device: true,
                            tilt_deg: Some(20.),
                            rotation_deg: Some(35.),
                            ..Default::default()
                        },
                    );
                });
            }
            // Ensure a pending frame remains pending regardless of how fast this host is.
            app.last_texture_update =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(10));
            let output = ctx.run_ui(Default::default(), |ui| app.ensure_texture(ui.ctx()));
            assert!(
                output.textures_delta.set.is_empty(),
                "must defer an active-stroke upload"
            );
            app.finish_stroke();
            let output = ctx.run_ui(Default::default(), |ui| app.ensure_texture(ui.ctx()));
            assert!(
                !output.textures_delta.set.is_empty(),
                "pen-up must flush deferred updates immediately"
            );
            assert!(app.document.take_dirty().is_none());
            let pixels = app.renderer.rgba8(&app.document);
            let paths = serde_json::to_vec(&app.document.layers[0].vectors).unwrap();
            if let Some((ref expected_pixels, ref expected_paths)) = expected {
                assert_eq!(&pixels, expected_pixels);
                assert_eq!(&paths, expected_paths);
            } else {
                expected = Some((pixels, paths));
            }
            assert!(app.history.undo(&mut app.document));
            assert!(app.document.surface.graphite_mass.iter().all(|&v| v == 0.));
        }
    }

    #[test]
    fn options_menu_opens_and_shows_all_profiles() {
        let mut app = GraphiteApp::with_render_state(None);
        app.acceleration = AccelerationMode::IntelHd;
        app.startup_acceleration = AccelerationMode::IntelHd;
        app.renderer_label = "Intel HD compatibility · OpenGL".into();
        let ctx = egui::Context::default();
        let mut preview = crate::ui::test_render::Preview::default();
        let mut frame = |app: &mut GraphiteApp, events| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100., 760.))),
                    events,
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
            preview.update(&output);
            output
        };
        let output = frame(&mut app, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Options" => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Options menu button");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let output = frame(&mut app, vec![]);
        for label in [
            "Windows Ink",
            "Wintab",
            "General",
            "AMD Radeon 7900 XTX",
            "Intel HD Graphics",
        ] {
            assert!(
                output.shapes.iter().any(
                    |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == label)
                ),
                "Missing {label}"
            );
        }
        let menu_pos = pos;
        for (label, expected) in [("Wintab", true), ("Windows Ink", false)] {
            let pos = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(t.pos + t.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(app.settings.use_wintab, expected);
            // Selecting an item closes egui's menu. Reopen it for the next choice / preview.
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(menu_pos),
                        egui::Event::PointerButton {
                            pos: menu_pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        }
        let output = frame(&mut app, vec![]);
        preview.save(&ctx, &output, "../../ui-v21-backend.png", [1100, 760]);
    }
}
