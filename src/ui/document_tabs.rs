use super::action_icons::{self, ActionIcon};
use eframe::egui::{self, Color32, Rect, Sense, Stroke};

pub struct TabView {
    pub id: u64,
    pub title: String,
    pub zoom: f32,
    pub layer: String,
    pub modified: bool,
}
#[derive(Debug, PartialEq)]
pub enum TabAction {
    None,
    Activate(u64),
    Close(u64),
    New,
    Reorder(u64, u64, bool),
}
#[derive(Clone, Copy)]
struct DragTab(u64);

pub fn show(ui: &mut egui::Ui, tabs: &[TabView], active: u64) -> TabAction {
    let mut action = TabAction::None;
    egui::Frame::new()
        .fill(Color32::from_rgb(230, 232, 235))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("drawing_tabs")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    ui.horizontal(|ui| {
                        for tab in tabs {
                            ui.push_id(tab.id, |ui| {
                                let label = format!(
                                    "{} @ {:.0}% ({}, RGB){}",
                                    tab.title,
                                    tab.zoom * 100.,
                                    tab.layer,
                                    if tab.modified { " *" } else { "" }
                                );
                                let font = egui::FontId::proportional(12.5);
                                let color =
                                    Color32::from_gray(if tab.id == active { 35 } else { 85 });
                                let galley =
                                    ui.painter().layout_no_wrap(label.clone(), font, color);
                                let width = (galley.size().x + 49.).clamp(190., 360.);
                                let (rect, _) =
                                    ui.allocate_exact_size(egui::vec2(width, 34.), Sense::hover());
                                let close_rect = Rect::from_center_size(
                                    egui::pos2(rect.right() - 16., rect.center().y),
                                    egui::vec2(24., 24.),
                                );
                                let body = Rect::from_min_max(
                                    rect.min,
                                    egui::pos2(close_rect.left() - 2., rect.bottom()),
                                );
                                let response =
                                    ui.interact(body, ui.id().with("tab"), Sense::click_and_drag());
                                let close =
                                    ui.interact(close_rect, ui.id().with("close"), Sense::click());
                                let fill = if tab.id == active {
                                    Color32::WHITE
                                } else if response.hovered() {
                                    Color32::from_gray(244)
                                } else {
                                    Color32::from_rgb(227, 230, 234)
                                };
                                ui.painter().rect_filled(rect, 0., fill);
                                ui.painter().vline(
                                    rect.right(),
                                    rect.y_range(),
                                    Stroke::new(1., Color32::from_gray(195)),
                                );
                                if tab.id == active {
                                    ui.painter().hline(
                                        rect.x_range(),
                                        rect.top() + 1.,
                                        Stroke::new(2., Color32::from_rgb(75, 133, 190)),
                                    );
                                }
                                let text_rect = body.shrink2(egui::vec2(10., 0.));
                                ui.painter()
                                    .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                                    .galley(
                                        egui::pos2(
                                            text_rect.left(),
                                            rect.center().y - galley.size().y * 0.5,
                                        ),
                                        galley,
                                        color,
                                    );
                                if close.hovered() {
                                    ui.painter().rect_filled(
                                        close_rect,
                                        3.,
                                        Color32::from_gray(210),
                                    );
                                }
                                action_icons::paint(
                                    ui.painter(),
                                    close_rect.shrink(3.),
                                    ActionIcon::Close,
                                    color,
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::Button,
                                        true,
                                        tab.id == active,
                                        &label,
                                    )
                                });
                                close.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        "Close drawing",
                                    )
                                });
                                if response.clicked() {
                                    action = TabAction::Activate(tab.id);
                                }
                                if close.clicked()
                                    || response.clicked_by(egui::PointerButton::Middle)
                                {
                                    action = TabAction::Close(tab.id);
                                }
                                response.dnd_set_drag_payload(DragTab(tab.id));
                                let before = ui
                                    .input(|i| i.pointer.hover_pos())
                                    .is_some_and(|p| p.x < rect.center().x);
                                if let Some(drag) = response.dnd_hover_payload::<DragTab>() {
                                    if drag.0 != tab.id {
                                        ui.painter().vline(
                                            if before { rect.left() } else { rect.right() },
                                            rect.y_range(),
                                            Stroke::new(2., Color32::from_rgb(50, 120, 200)),
                                        );
                                    }
                                }
                                if let Some(drag) = response.dnd_release_payload::<DragTab>() {
                                    if drag.0 != tab.id {
                                        action = TabAction::Reorder(drag.0, tab.id, before);
                                    }
                                }
                                #[cfg(test)]
                                ui.data_mut(|d| {
                                    d.insert_temp(egui::Id::new(("test_tab_body", tab.id)), body);
                                    d.insert_temp(
                                        egui::Id::new(("test_tab_close", tab.id)),
                                        close_rect,
                                    );
                                });
                                response.on_hover_text(label);
                                close.on_hover_text("Close drawing");
                            });
                        }
                        ui.add_space(5.);
                        if action_icons::button(ui, ActionIcon::Plus, "New drawing", false)
                            .clicked()
                        {
                            action = TabAction::New;
                        }
                    });
                });
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>) -> TabAction {
        let mut action = TabAction::None;
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 100.),
                )),
                events,
                ..Default::default()
            },
            |root| {
                egui::CentralPanel::default().show(root, |ui| {
                    let tabs: Vec<_> = (1..=3)
                        .map(|id| TabView {
                            id,
                            title: format!("Drawing {id}"),
                            zoom: 1.5,
                            layer: "Layer 2".into(),
                            modified: true,
                        })
                        .collect();
                    action = show(ui, &tabs, 1);
                });
            },
        );
        action
    }
    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn rect(ctx: &egui::Context, key: &str, id: u64) -> Rect {
        ctx.data(|d| d.get_temp(egui::Id::new((key, id))).unwrap())
    }
    #[test]
    fn tabs_activate_close_and_drag_by_stable_identity() {
        let ctx = egui::Context::default();
        frame(&ctx, vec![]);
        let pos = rect(&ctx, "test_tab_body", 2).center();
        frame(
            &ctx,
            vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
        );
        assert_eq!(
            frame(&ctx, vec![pointer(pos, false)]),
            TabAction::Activate(2)
        );
        let pos = rect(&ctx, "test_tab_close", 2).center();
        frame(
            &ctx,
            vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
        );
        assert_eq!(frame(&ctx, vec![pointer(pos, false)]), TabAction::Close(2));
        let start = rect(&ctx, "test_tab_body", 3).center();
        let target = rect(&ctx, "test_tab_body", 1).left_center() + egui::vec2(10., 0.);
        frame(
            &ctx,
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
        );
        frame(
            &ctx,
            vec![egui::Event::PointerMoved(start - egui::vec2(15., 0.))],
        );
        frame(&ctx, vec![egui::Event::PointerMoved(target)]);
        assert_eq!(
            frame(&ctx, vec![pointer(target, false)]),
            TabAction::Reorder(3, 1, true)
        );
    }
}
