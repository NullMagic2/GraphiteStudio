use super::*;

fn frame(app: &mut GraphiteApp, ctx: &egui::Context, time: f64, input: PointerFrame) {
    let _ = ctx.run_ui(
        egui::RawInput {
            time: Some(time),
            ..Default::default()
        },
        |ui| {
            app.handle_drawing_input(
                ui,
                Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.)),
                input,
            );
        },
    );
}
fn point(x: f32, y: f32, pressed: bool) -> PointerFrame {
    PointerFrame {
        position: Some(Pos2::new(x, y)),
        primary_pressed: pressed,
        primary_down: true,
        pressure: 0.6,
        pressure_from_device: true,
        tilt_deg: Some(8.),
        ..Default::default()
    }
}

#[test]
fn long_pencil_holds_preserve_the_curve_and_undo_redo() {
    for legacy_enabled in [false, true] {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Freehand", 50., 50., 120.));
        app.viewport.zoom = 1.;
        // Older projects/pencil presets may still carry the removed setting.
        let mut settings = serde_json::to_value(&app.settings).unwrap();
        settings["hold_to_straighten"] = legacy_enabled.into();
        app.settings = serde_json::from_value(settings).unwrap();
        app.settings.tool = ToolKind::Pencil;
        let ctx = egui::Context::default();
        let blank = app.renderer.rgba8(&app.document);
        frame(&mut app, &ctx, 0., point(40., 40., true));
        frame(&mut app, &ctx, 0.1, point(80., 110., false));
        frame(&mut app, &ctx, 0.2, point(170., 80., false));
        let curve = app.renderer.rgba8(&app.document);
        assert_ne!(curve, blank);
        for time in [0.9, 2., 10., 60.] {
            frame(&mut app, &ctx, time, PointerFrame::default());
            assert_eq!(
                app.renderer.rgba8(&app.document),
                curve,
                "Hold at {time}s changed the curve"
            );
        }
        frame(&mut app, &ctx, 60.1, point(180., 40., false));
        frame(
            &mut app,
            &ctx,
            60.2,
            PointerFrame {
                primary_released: true,
                ..Default::default()
            },
        );
        let record = app.document.layers[0].vectors.strokes.last().unwrap();
        assert!(!record.polyline);
        assert!(record.points.iter().any(|p| p.y > 80.));
        let finished = app.renderer.rgba8(&app.document);
        assert!(app.history.undo(&mut app.document));
        assert_eq!(app.renderer.rgba8(&app.document), blank);
        assert!(app.history.redo(&mut app.document));
        assert_eq!(app.renderer.rgba8(&app.document), finished);
        let path = std::env::temp_dir().join(format!(
            "graphite-freehand-{}-{legacy_enabled}.graphite",
            std::process::id()
        ));
        app.save_project_path(&path).unwrap();
        let reopened = graphite_studio::project::load(&path).unwrap();
        assert!(
            !reopened.document.layers[0]
                .vectors
                .strokes
                .last()
                .unwrap()
                .polyline
        );
        assert_eq!(
            RasterRenderer::default().rgba8(&reopened.document),
            finished
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn older_pencil_preferences_load_without_reintroducing_hold_controls() {
    let defaults =
        graphite_studio::pencil_settings::PencilControls::capture(&ToolSettings::default(), 0.);
    let mut old = serde_json::to_value(&defaults).unwrap();
    old["hold_to_straighten"] = true.into();
    let loaded: graphite_studio::pencil_settings::PencilControls =
        serde_json::from_value(old).unwrap();
    assert_eq!(loaded, defaults);
    assert!(serde_json::to_value(&loaded)
        .unwrap()
        .get("hold_to_straighten")
        .is_none());
}
