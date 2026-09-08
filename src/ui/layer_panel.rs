use eframe::egui;

use super::action_icons::{self, ActionIcon};
use crate::core::document::{BlendMode, Document};

#[derive(Clone, Copy)]
struct LayerDrag(u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerPanelAction {
    None,
    Activate(usize),
    Select { index: usize, toggle: bool, range: bool },
    SetMultiple(bool),
    Add,
    RemoveActive,
    MergeDown,
    MergeSelected,
    CopyActive,
    MoveActiveUp,
    MoveActiveDown,
    SetVisible {
        index: usize,
        visible: bool,
    },
    SetBlendMode {
        id: u64,
        mode: BlendMode,
    },
    SetOpacity {
        id: u64,
        opacity: u8,
    },
    Reorder {
        dragged_id: u64,
        target_id: u64,
        above: bool,
    },
}

pub fn show_layer_panel(ui: &mut egui::Ui, document: &Document) -> LayerPanelAction {
    let mut action = LayerPanelAction::None;
    ui.set_min_width(230.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(24., 24.), egui::Sense::hover());
        action_icons::paint(
            ui.painter(),
            rect,
            ActionIcon::Layers,
            ui.visuals().text_color(),
        );
        ui.heading("Layers");
    });

    ui.horizontal_wrapped(|ui| {
        if action_icons::button(ui, ActionIcon::AddLayer, "Add layer", false).clicked() {
            action = LayerPanelAction::Add;
        }
        if ui
            .add_enabled_ui(document.layer_count() > 1, |ui| {
                action_icons::button(ui, ActionIcon::DeleteLayer, "Delete active layer", false)
            })
            .inner
            .clicked()
        {
            action = LayerPanelAction::RemoveActive;
        }
        if ui
            .add_enabled(
                document.active_layer_index() + 1 < document.layer_count(),
                egui::Button::new("Up"),
            )
            .on_hover_text("Move active layer up")
            .clicked()
        {
            action = LayerPanelAction::MoveActiveUp;
        }
        if ui
            .add_enabled(document.active_layer_index() > 0, egui::Button::new("Down"))
            .on_hover_text("Move active layer down")
            .clicked()
        {
            action = LayerPanelAction::MoveActiveDown;
        }
    });

    let selected=document.selected_layer_indices();
    let mut multiple=document.layer_selection.multiple;
    let toggle=ui.checkbox(&mut multiple,"Select multiple").on_hover_text("Tap layer names to add/remove them from the selection without a keyboard.");
    #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("test_layer_multiple"),toggle.rect));
    if toggle.changed() { action=LayerPanelAction::SetMultiple(multiple); }
    let many=selected.len()>1;
    ui.horizontal(|ui| {
    let merge = ui.add_enabled(if many {document.can_merge_selected()}else{document.can_merge_down()}, egui::Button::new(if many {format!("Merge selected ({})",selected.len())}else{"Merge down".into()}).min_size(egui::vec2(160., 40.)))
        .on_hover_text(if many {"Combine selected visible layers in one step at the highest selected position. Unselected layers stay separate. Undo restores all layers and paths."} else {"Merge the active layer with the visible layer below. Undo restores layers and editable paths. Make both layers visible first."});
    #[cfg(test)]
    ui.data_mut(|d| d.insert_temp(egui::Id::new("test_merge_down"), merge.rect));
    if merge.clicked() { action = if many {LayerPanelAction::MergeSelected}else{LayerPanelAction::MergeDown}; }
    let copy=ui.add_enabled_ui(document.layer_count()<256,|ui| action_icons::button_sized(ui,ActionIcon::CopyLayer,"Copy layer",false,egui::vec2(48.,48.))).inner;
    if copy.clicked(){action=LayerPanelAction::CopyActive;}
    });
    if many {
        ui.small(format!("{} selected · drawing and opacity use the active layer",selected.len()));
        if selected.windows(2).any(|p|p[1]!=p[0]+1) { ui.small("Merging moves the selected layers above any layers between them."); }
        if !document.can_merge_selected() { ui.small("Show all selected layers to merge them."); }
    }

    let active_layer = &document.layers[document.active_layer_index()];
    let mut mode = active_layer.blend_mode;
    egui::ComboBox::from_id_salt("active_layer_blend_mode")
        .selected_text(mode.label()).width(160.0)
        .show_ui(ui, |ui| {
            for option in BlendMode::ALL { ui.selectable_value(&mut mode, option, option.label()); }
        }).response.on_hover_text("Blend mode for the active layer. Multiply combines translucent graphite deposits without painting over the paper texture.");
    if mode != active_layer.blend_mode {
        action = LayerPanelAction::SetBlendMode {
            id: active_layer.id,
            mode,
        };
    }
    // Round only the displayed percent. Merely showing a native PSD value such as 137/255
    // must not quantize and rewrite the stored opacity.
    let mut percent = (active_layer.opacity as f32 * 100. / 255.).round();
    let response = ui.add(egui::Slider::new(&mut percent, 0.0..=100.0).integer().suffix("%").text("Opacity"))
        .on_hover_text("Opacity of the selected layer only. 0% makes it transparent; 100% shows its full graphite strength. Material and editable paths are preserved.");
    #[cfg(test)]
    ui.data_mut(|data| data.insert_temp(egui::Id::new("test_layer_opacity_rect"), response.rect));
    if response.changed() {
        action = LayerPanelAction::SetOpacity {
            id: active_layer.id,
            opacity: (percent * 255. / 100.).round().clamp(0., 255.) as u8,
        };
    }
    ui.small("Ctrl-click: select multiple · Shift-click: select a range. Drag names to reorder.");
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("layer_list_scroll")
        .max_height((ui.available_height() - 72.0).max(60.0))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Photoshop-style visual ordering: highest compositing layer is shown at the top.
            for index in (0..document.layer_count()).rev() {
                let layer = &document.layers[index];
                let active = index == document.active_layer_index();
                ui.horizontal(|ui| {
                    let mut visible = layer.visible;
                    if ui
                        .checkbox(&mut visible, "")
                        .on_hover_text(if visible { "Hide layer" } else { "Show layer" })
                        .changed()
                    {
                        action = LayerPanelAction::SetVisible { index, visible };
                    }
                    let response = ui
                        .add_sized(
                            [ui.available_width(), if multiple {44.0}else{28.0}],
                            egui::Button::new(format!(
                                "{} · {:.0}%{}",
                                layer.name,
                                layer.opacity as f32 * 100. / 255.,
                                if active && many {" · active"}else{""}
                            ))
                            .selected(selected.contains(&index))
                            .sense(egui::Sense::click_and_drag())
                            .truncate(),
                        )
                        .on_hover_text(if active {
                            "Active drawing layer"
                        } else {
                            "Make layer active"
                        });
                    if response.clicked() {
                        let modifiers=ui.input(|i|i.modifiers);
                        let toggle=modifiers.ctrl || modifiers.command || multiple;
                        action = if toggle || modifiers.shift { LayerPanelAction::Select {index,toggle,range:modifiers.shift} } else {LayerPanelAction::Activate(index)};
                    }
                    response.dnd_set_drag_payload(LayerDrag(layer.id));
                    #[cfg(test)]
                    ui.data_mut(|data| {
                        data.insert_temp(
                            egui::Id::new(("test_layer_rect", layer.id)),
                            response.rect,
                        )
                    });
                    let above = ui
                        .input(|i| i.pointer.hover_pos())
                        .is_some_and(|pos| pos.y < response.rect.center().y);
                    if let Some(dragged) = response.dnd_hover_payload::<LayerDrag>() {
                        if dragged.0 != layer.id {
                            let y = if above {
                                response.rect.top()
                            } else {
                                response.rect.bottom()
                            };
                            ui.painter().hline(
                                response.rect.x_range(),
                                y,
                                egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                            );
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                    }
                    if let Some(dragged) = response.dnd_release_payload::<LayerDrag>() {
                        action = LayerPanelAction::Reorder {
                            dragged_id: dragged.0,
                            target_id: layer.id,
                            above,
                        };
                    }
                });
            }
            // Keep distant rows reachable while dragging in a long layer list.
            if egui::DragAndDrop::has_payload_of_type::<LayerDrag>(ui.ctx()) {
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let clip = ui.clip_rect();
                    if pos.x >= clip.left() && pos.x <= clip.right() {
                        let delta = if pos.y < clip.top() + 24.0 {
                            6.0
                        } else if pos.y > clip.bottom() - 24.0 {
                            -6.0
                        } else {
                            0.0
                        };
                        if delta != 0.0 {
                            ui.scroll_with_delta(egui::vec2(0.0, delta));
                            ui.ctx().request_repaint();
                        }
                    }
                }
            }
        });

    ui.separator();
    ui.small("Pencil, smudge and eraser affect only the active layer. Paper texture and paper deformation are shared by the document.");
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{document::CanvasSpec, paper::PaperPreset};

    fn frame(ctx: &egui::Context, doc: &Document, events: Vec<egui::Event>) -> LayerPanelAction {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(280.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut result = LayerPanelAction::None;
        let _ = ctx.run_ui(input, |root| {
            egui::CentralPanel::default().show(root, |ui| {
                result = show_layer_panel(ui, doc);
            });
        });
        result
    }
    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn opacity_slider_targets_selected_layer_and_does_not_quantize_on_display() {
        let spec = CanvasSpec::from_physical("Opacity", 5., 5., 120.);
        let n = spec.pixel_count();
        let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; n]);
        doc.add_layer();
        doc.set_layer_opacity(doc.active_layer_id(), 137);
        let ctx = egui::Context::default();
        assert_eq!(frame(&ctx, &doc, vec![]), LayerPanelAction::None);
        assert_eq!(doc.layers[1].opacity, 137);
        let rect = ctx.data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::new("test_layer_opacity_rect"))
                .unwrap()
        });
        let pos = egui::pos2(rect.left() + 4., rect.center().y);
        let action = frame(
            &ctx,
            &doc,
            vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
        );
        assert!(matches!(action,LayerPanelAction::SetOpacity { id:2, opacity } if opacity < 20));
        assert_eq!(doc.layers[0].opacity, 255);
    }
    #[test]
    fn layer_icon_buttons_add_delete_and_protect_the_last_layer() {
        let spec = CanvasSpec::from_physical("UI", 5., 5., 120.);
        let n = spec.pixel_count();
        let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; n]);
        let ctx = egui::Context::default();
        frame(&ctx, &doc, vec![]);
        let click = |doc: &Document, label: &str| {
            let pos = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::new(("test_action_icon", label)))
                    .unwrap()
                    .center()
            });
            frame(
                &ctx,
                doc,
                vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
            );
            frame(&ctx, doc, vec![pointer(pos, false)])
        };
        assert_eq!(click(&doc, "Delete active layer"), LayerPanelAction::None);
        assert_eq!(click(&doc, "Add layer"), LayerPanelAction::Add);
        doc.add_layer();
        frame(&ctx, &doc, vec![]);
        assert_eq!(
            click(&doc, "Delete active layer"),
            LayerPanelAction::RemoveActive
        );
    }

    #[test]
    fn dragging_a_layer_name_emits_reorder_without_switching_material() {
        let spec = CanvasSpec::from_physical("UI", 5.0, 5.0, 120.0);
        let n = spec.pixel_count();
        let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.0; 3]; n]);
        doc.add_layer();
        doc.add_layer();
        let ctx = egui::Context::default();
        frame(&ctx, &doc, vec![]);
        let rect = |id| {
            ctx.data(|data| {
                data.get_temp::<egui::Rect>(egui::Id::new(("test_layer_rect", id)))
                    .unwrap()
            })
        };
        let start = rect(3_u64).center();
        let target = rect(1_u64).center() + egui::vec2(0.0, 9.0);
        frame(
            &ctx,
            &doc,
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
        );
        frame(
            &ctx,
            &doc,
            vec![egui::Event::PointerMoved(start + egui::vec2(0.0, 12.0))],
        );
        frame(&ctx, &doc, vec![egui::Event::PointerMoved(target)]);
        let action = frame(&ctx, &doc, vec![pointer(target, false)]);
        assert_eq!(
            action,
            LayerPanelAction::Reorder {
                dragged_id: 3,
                target_id: 1,
                above: false
            }
        );
        assert_eq!(doc.active_layer_id(), 3);
    }
}
