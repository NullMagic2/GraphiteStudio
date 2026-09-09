use super::*;

fn app() -> GraphiteApp {
    let mut a = GraphiteApp::with_render_state(None);
    a.replace_document(CanvasSpec::from_physical("Outside page", 50., 50., 120.));
    a.fit_requested = false;
    a.viewport.zoom = 1.;
    a.settings.line_smoothing = 0.;
    a.settings.tool = ToolKind::Brush;
    a.settings.brush_size_px = 18.;
    a.workspace_view_size = Vec2::new(900., 700.);
    a.workspace_canvas = Some(Rect::from_min_size(
        Pos2::new(300., 220.),
        Vec2::splat(236.),
    ));
    a
}
fn input(a: &mut GraphiteApp, ctx: &egui::Context, position: Pos2, pressed: bool, released: bool) {
    let canvas = a.workspace_canvas.unwrap();
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 700.))),
            ..Default::default()
        },
        |ui| {
            a.handle_drawing_input(
                ui,
                canvas,
                PointerFrame {
                    position: Some(position),
                    primary_pressed: pressed,
                    primary_released: released,
                    primary_down: !released,
                    pressure: 0.8,
                    pressure_from_device: true,
                    tilt_deg: Some(65.),
                    ..Default::default()
                },
            );
        },
    );
}
fn mass_on_page(a: &GraphiteApp) -> Vec<f32> {
    let [x, y, w, h] = a.document.page_bounds();
    (y..y + h)
        .flat_map(|j| {
            (x..x + w)
                .map(move |i| a.document.surface.graphite_mass[j * a.document.spec.width_px + i])
        })
        .collect()
}

#[test]
fn strokes_cross_every_page_edge_without_pointer_resistance_and_keep_undo() {
    for tool in [ToolKind::Pencil, ToolKind::Brush, ToolKind::Tissue] {
        for angle in [0., 0.73] {
            let mut a = app();
            a.settings.tool = tool;
            a.settings.tissue_size_px = 24.;
            a.settings.tissue_load = 0.6;
            a.viewport.rotation = angle;
            let ctx = egui::Context::default();
            let original_canvas = a.workspace_canvas.unwrap();
            for (start, end) in [
                (Vec2::new(20., 80.), Vec2::new(-35., 80.)),
                (Vec2::new(210., 110.), Vec2::new(270., 110.)),
                (Vec2::new(100., 20.), Vec2::new(100., -40.)),
                (Vec2::new(140., 215.), Vec2::new(140., 275.)),
            ] {
                let from = a.viewport.document_to_screen(original_canvas, start);
                let to = a.viewport.document_to_screen(original_canvas, end);
                input(&mut a, &ctx, from, true, false);
                for step in 1..=12 {
                    input(
                        &mut a,
                        &ctx,
                        from + (to - from) * (step as f32 / 12.),
                        false,
                        false,
                    );
                }
                input(&mut a, &ctx, to, false, true);
                let record = a.document.layers[0].vectors.strokes.last().unwrap();
                let p = record.points.last().unwrap();
                let actual = Vec2::new(p.x, p.y) - a.document.page_origin();
                assert!(
                    (actual - end).length() < 0.01,
                    "{tool:?} angle {angle}: {actual:?} != {end:?}"
                );
                let screen = a
                    .viewport
                    .document_to_screen(a.workspace_canvas.unwrap(), Vec2::new(p.x, p.y));
                assert!((screen - to).length() < 0.01, "Page shifted under pointer");
                let i = a.document.index(p.x as usize, p.y as usize);
                let w = a.document.spec.width_px;
                assert!(
                    (i - w * 8..i + w * 8).any(|j| a.document.surface.total_deposit(j) > 0.),
                    "No artwork beyond edge"
                );
            }
            assert_eq!(&a.document.page_bounds()[2..], &[236, 236]);
            let finished = a.renderer.rgba8(&a.document);
            for _ in 0..4 {
                assert!(a.history.undo(&mut a.document));
            }
            assert!(a.document.surface.graphite_mass.iter().all(|&v| v == 0.));
            for _ in 0..4 {
                assert!(a.history.redo(&mut a.document));
            }
            assert!(a.renderer.rgba8(&a.document) == finished);
        }
    }
}

#[test]
fn expansion_preserves_existing_material_layers_redo_and_view_anchor() {
    for zoom in [0.5, 2.5] {
        for angle in [0., 0.73] {
            let mut a = app();
            a.viewport.zoom = zoom;
            a.viewport.rotation = angle;
            a.viewport.free_pan = Vec2::new(33., -27.);
            let ctx = egui::Context::default();
            let from = a
                .viewport
                .document_to_screen(a.workspace_canvas.unwrap(), Vec2::new(100., 100.));
            input(&mut a, &ctx, from, true, false);
            input(&mut a, &ctx, from, false, true);
            let material = mass_on_page(&a);
            assert!(a.history.copy_layer(&mut a.document));
            assert!(a.history.undo(&mut a.document));
            let old = Vec2::new(
                a.document.spec.width_px as f32,
                a.document.spec.height_px as f32,
            );
            let l = a.viewport.layout(a.workspace_view_size, old);
            let canvas =
                Rect::from_center_size((l.sheet_min + l.sheet_size * 0.5).to_pos2(), old * zoom);
            a.workspace_canvas = Some(canvas);
            let anchor = a.viewport.document_to_screen(canvas, Vec2::new(85., 92.));
            let shift = a.grow_workspace(Rect::from_min_max(
                Pos2::new(-65., -87.),
                Pos2::new(350., 420.),
            ));
            let size = Vec2::new(
                a.document.spec.width_px as f32,
                a.document.spec.height_px as f32,
            );
            let l = a.viewport.layout(a.workspace_view_size, size);
            let rebuilt =
                Rect::from_center_size((l.sheet_min + l.sheet_size * 0.5).to_pos2(), size * zoom);
            assert!(
                (a.viewport
                    .document_to_screen(rebuilt, Vec2::new(85., 92.) + shift)
                    - anchor)
                    .length()
                    < 0.002
            );
            assert!(mass_on_page(&a) == material);
            assert!(a.history.redo(&mut a.document));
            assert_eq!(a.document.layer_count(), 2);
            assert!(mass_on_page(&a) == material);
            assert!(a.history.undo(&mut a.document));
            assert!(a.history.undo(&mut a.document));
            assert!(mass_on_page(&a).iter().all(|&v| v == 0.));
            assert!(a.history.redo(&mut a.document));
            assert!(mass_on_page(&a) == material);
        }
    }
}

#[test]
fn outside_artwork_survives_project_psd_and_image_exports() {
    let mut a = app();
    let ctx = egui::Context::default();
    input(&mut a, &ctx, Pos2::new(275., 300.), true, false);
    input(&mut a, &ctx, Pos2::new(560., 300.), false, false);
    input(&mut a, &ctx, Pos2::new(560., 300.), false, true);
    let expected = a.renderer.rgba8(&a.document);
    for ext in ["graphite", "psd", "png", "jpg", "bmp"] {
        let path =
            std::env::temp_dir().join(format!("graphite-outside-{}.{}", std::process::id(), ext));
        a.save_document_path(&path);
        if ["graphite", "psd"].contains(&ext) {
            let p = graphite_studio::project::load(&path).unwrap();
            assert_eq!(p.document.page, a.document.page);
            assert_eq!(p.document.layers[0].vectors.strokes.len(), 1);
            assert!(RasterRenderer::default().rgba8(&p.document) == expected);
        } else {
            let im = image::open(&path).unwrap();
            assert_eq!(
                (im.width(), im.height()),
                (
                    a.document.spec.width_px as u32,
                    a.document.spec.height_px as u32
                )
            );
            if ext != "jpg" {
                assert!(im.to_rgba8().into_raw() == expected);
            }
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn full_interface_keeps_page_stationary_during_and_after_crossing() {
    let mut a = app();
    a.viewport.zoom = 1.;
    a.viewport.rotation = 0.4;
    a.material_panel_visible = false;
    a.layers_panel_visible = false;
    let ctx = egui::Context::default();
    let mut preview = crate::ui::test_render::Preview::default();
    let mut run = |a: &mut GraphiteApp, events| {
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100., 800.))),
                events,
                ..Default::default()
            },
            |ui| a.interface(ui),
        );
        preview.update(&out);
        out
    };
    for _ in 0..3 {
        run(&mut a, vec![]);
    }
    let original = a.workspace_canvas.unwrap();
    let anchor = a.viewport.document_to_screen(original, Vec2::ZERO);
    let from = a.viewport.document_to_screen(original, Vec2::new(8., 115.));
    let to = a
        .viewport
        .document_to_screen(original, Vec2::new(-70., 115.));
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    run(
        &mut a,
        vec![egui::Event::PointerMoved(from), button(from, true)],
    );
    for i in 1..=16 {
        let pos = from + (to - from) * (i as f32 / 16.);
        run(&mut a, vec![egui::Event::PointerMoved(pos)]);
        let now = a
            .viewport
            .document_to_screen(a.workspace_canvas.unwrap(), a.document.page_origin());
        assert!(
            (now - anchor).length() < 0.1,
            "Edge expansion moved page: {anchor:?} -> {now:?}"
        );
    }
    run(&mut a, vec![button(to, false)]);
    let out = run(&mut a, vec![]);
    preview.save(&ctx, &out, "../../ui-v2413-outside.png", [1100, 800]);
    let now = a
        .viewport
        .document_to_screen(a.workspace_canvas.unwrap(), a.document.page_origin());
    assert!((now - anchor).length() < 0.1);
    assert_eq!(a.document.layers[0].vectors.strokes.len(), 1);
}
