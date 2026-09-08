use super::*;

fn frame(
    app: &mut GraphiteApp,
    ctx: &egui::Context,
    preview: &mut crate::ui::test_render::Preview,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
            events,
            ..Default::default()
        },
        |ui| app.interface(ui),
    );
    preview.update(&output);
    output
}
fn click(
    app: &mut GraphiteApp,
    ctx: &egui::Context,
    preview: &mut crate::ui::test_render::Preview,
    pos: Pos2,
) -> egui::FullOutput {
    frame(
        app,
        ctx,
        preview,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    frame(
        app,
        ctx,
        preview,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )
}
fn text_position(output: &egui::FullOutput, text: &str) -> Option<Pos2> {
    output.shapes.iter().find_map(|s| match &s.shape {
        egui::Shape::Text(t) if t.galley.job.text == text => Some(t.pos + t.galley.size() * 0.5),
        _ => None,
    })
}

#[test]
fn recent_submenu_opens_files_and_options_slider_disables_it() {
    let mut app = GraphiteApp::with_render_state(None);
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/open");
    app.open_project_path(&fixtures.join("rgba.PNG")).unwrap();
    app.open_project_path(&fixtures.join("color.bmp")).unwrap();
    let ctx = egui::Context::default();
    let mut preview = crate::ui::test_render::Preview::default();
    for _ in 0..20 {
        let _ = frame(&mut app, &ctx, &mut preview, vec![]);
    }
    let output = frame(&mut app, &ctx, &mut preview, vec![]);
    click(
        &mut app,
        &ctx,
        &mut preview,
        text_position(&output, "File").unwrap(),
    );
    let output = frame(&mut app, &ctx, &mut preview, vec![]);
    click(
        &mut app,
        &ctx,
        &mut preview,
        text_position(&output, "Recent files").expect("recent submenu"),
    );
    for _ in 0..20 {
        let _ = frame(&mut app, &ctx, &mut preview, vec![]);
    }
    let output = frame(&mut app, &ctx, &mut preview, vec![]);
    assert!(text_position(&output, "1. color.bmp").is_some());
    assert!(text_position(&output, "2. rgba.PNG").is_some());
    preview.save(
        &ctx,
        &output,
        "../../ui-v2410-recent-files.png",
        [1440, 920],
    );
    let pos = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::new(("recent_file_button", 1usize)))
            .unwrap()
            .center()
    });
    let before = app.tabs.len();
    click(&mut app, &ctx, &mut preview, pos);
    app.wait_for_opening();
    assert_eq!(app.tabs.len(), before + 1);
    assert!(app.recent_files.files()[0].ends_with("rgba.PNG"));
    let output = frame(&mut app, &ctx, &mut preview, vec![]);
    click(
        &mut app,
        &ctx,
        &mut preview,
        text_position(&output, "Options").unwrap(),
    );
    for _ in 0..20 {
        let _ = frame(&mut app, &ctx, &mut preview, vec![]);
    }
    let output = frame(&mut app, &ctx, &mut preview, vec![]);
    preview.save(
        &ctx,
        &output,
        "../../ui-v2410-recent-options.png",
        [1440, 920],
    );
    let rect = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::new("recent_files_maximum"))
            .unwrap()
    });
    click(
        &mut app,
        &ctx,
        &mut preview,
        Pos2::new(rect.left() + 2., rect.center().y),
    );
    assert_eq!(app.recent_files.maximum(), 0);
    assert!(app.recent_files.files().is_empty());
    // The File menu no longer offers history while disabled.
    let output = frame(&mut app, &ctx, &mut preview, vec![]);
    click(
        &mut app,
        &ctx,
        &mut preview,
        text_position(&output, "File").unwrap(),
    );
    let output = frame(&mut app, &ctx, &mut preview, vec![]);
    assert!(text_position(&output, "Recent files").is_none());
}
