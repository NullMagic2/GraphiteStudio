//! Small vector tool illustrations: crisp at every display scale, no font-glyph dependency.
use crate::core::pencil::ToolKind;
use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke};

/// Large pen/touch targets with vector symbols that do not depend on font glyphs.
pub fn transform_action_button(ui: &mut egui::Ui, confirm: bool) -> egui::Response {
    let label = if confirm { "Confirm" } else { "Cancel" };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(48., 48.), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
    #[cfg(test)]
    ui.data_mut(|d| d.insert_temp(egui::Id::new(("test_transform_action", confirm)), rect));
    let visuals = ui.style().interact(&response);
    ui.painter().rect(rect.shrink(1.), 6., visuals.bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
    let center = rect.center();
    let stroke = Stroke::new(2.5, if confirm { Color32::from_rgb(35, 133, 76) } else { Color32::from_rgb(180, 63, 63) });
    if confirm {
        ui.painter().add(Shape::line(vec![center + egui::vec2(-8., 0.), center + egui::vec2(-2., 6.), center + egui::vec2(9., -7.)], stroke));
    } else {
        ui.painter().line_segment([center + egui::vec2(-7., -7.), center + egui::vec2(7., 7.)], stroke);
        ui.painter().line_segment([center + egui::vec2(-7., 7.), center + egui::vec2(7., -7.)], stroke);
    }
    response.on_hover_text(graphite_studio::shortcuts::hint(ui.ctx(),if confirm {"Confirm and deselect"}else{"Cancel transform and deselect"},if confirm {graphite_studio::shortcuts::Command::Confirm}else{graphite_studio::shortcuts::Command::Cancel}))
}

pub fn tool_button(ui: &mut egui::Ui, tool: ToolKind, selected: bool) -> egui::Response {
    tool_button_sized(ui,tool,selected,34.)
}

pub fn tool_button_sized(ui: &mut egui::Ui, tool: ToolKind, selected: bool, size:f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    #[cfg(test)]
    if size==34. {ui.data_mut(|d| d.insert_temp(egui::Id::new(("test_tool_icon", tool.label())), rect));}
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            ui.is_enabled(),
            selected,
            tool.label(),
        )
    });
    let visuals = ui.style().interact_selectable(&response, selected);
    ui.painter().rect(
        rect.shrink(1.),
        3.,
        visuals.bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    paint_icon(ui.painter(), rect.shrink(6.), tool);
    use graphite_studio::shortcuts::{hint,Command};
    response.on_hover_text(match tool {
        ToolKind::Pencil => hint(ui.ctx(),"Pencil",Command::Pencil),
        ToolKind::Eraser => hint(ui.ctx(),"Eraser",Command::Eraser),
        ToolKind::VectorSelect => hint(ui.ctx(),"Vector selection",Command::VectorSelect),
        _ => tool.label().into(),
    })
}

pub fn paint_icon(p: &egui::Painter, r: Rect, tool: ToolKind) {
    let point = |x: f32, y: f32| {
        Pos2::new(
            r.left() + x * r.width() / 32.0,
            r.top() + y * r.height() / 32.0,
        )
    };
    let polygon = |points: &[(f32, f32)], color: Color32| {
        p.add(Shape::convex_polygon(
            points.iter().map(|&(x, y)| point(x, y)).collect(),
            color,
            Stroke::new(0.7, Color32::from_gray(63)),
        ));
    };
    match tool {
        ToolKind::VectorSelect => {
            polygon(
                &[(6., 3.), (26., 19.), (17., 20.), (13., 29.)],
                Color32::from_rgb(250, 250, 250),
            );
            p.circle_filled(point(25., 28.), 2.5, Color32::from_rgb(45, 120, 200));
        }
        ToolKind::Pencil => {
            polygon(
                &[(6., 21.), (22., 5.), (28., 11.), (12., 27.)],
                Color32::from_rgb(224, 165, 51),
            );
            polygon(
                &[(8., 22.), (23., 7.), (25., 9.), (10., 24.)],
                Color32::from_rgb(255, 215, 99),
            );
            polygon(
                &[(6., 21.), (12., 27.), (3., 30.)],
                Color32::from_rgb(222, 186, 139),
            );
            polygon(&[(4., 26.), (7., 29.), (3., 30.)], Color32::from_gray(54));
            polygon(
                &[(22., 5.), (25., 2.), (31., 8.), (28., 11.)],
                Color32::from_rgb(210, 129, 139),
            );
            p.line_segment(
                [point(20.5, 6.5), point(26.5, 12.5)],
                Stroke::new(2.4, Color32::from_gray(145)),
            );
        }
        ToolKind::Smudge => {
            polygon(
                &[(5., 23.), (20., 8.), (27., 15.), (12., 30.)],
                Color32::from_rgb(225, 220, 208),
            );
            polygon(&[(20., 8.), (28., 3.), (27., 15.)], Color32::from_gray(110));
            polygon(&[(23., 7.), (28., 3.), (27., 9.)], Color32::from_gray(62));
            p.line_segment(
                [point(9., 22.), point(20., 11.)],
                Stroke::new(2., Color32::from_rgb(250, 247, 238)),
            );
            for offset in [0., 4., 8.] {
                p.line_segment(
                    [
                        point(10. + offset, 27. - offset),
                        point(14. + offset, 23. - offset),
                    ],
                    Stroke::new(0.8, Color32::from_gray(161)),
                );
            }
        }
        ToolKind::Brush => {
            polygon(
                &[(11., 21.), (24., 3.), (28., 6.), (17., 24.)],
                Color32::from_gray(100),
            );
            polygon(
                &[(11., 18.), (18., 24.), (13., 29.), (3., 30.), (7., 25.)],
                Color32::from_gray(70),
            );
        }
        ToolKind::Tissue => {
            polygon(
                &[(3., 9.), (22., 3.), (30., 20.), (17., 30.), (2., 24.)],
                Color32::from_gray(225),
            );
            p.line_segment(
                [point(3., 9.), point(17., 30.)],
                Stroke::new(1., Color32::GRAY),
            );
            p.line_segment(
                [point(22., 3.), point(12., 17.)],
                Stroke::new(1., Color32::GRAY),
            );
            p.circle_filled(point(20., 20.), 4., Color32::from_gray(130));
        }
        ToolKind::Shapes => {
            p.rect_stroke(
                Rect::from_two_pos(point(2., 3.), point(20., 21.)),
                1.,
                Stroke::new(1.7, Color32::from_gray(65)),
                egui::StrokeKind::Inside,
            );
            p.circle_stroke(
                point(22., 23.),
                8.,
                Stroke::new(1.7, Color32::from_gray(65)),
            );
        }
        ToolKind::Lasso => {
            let points = (0..=32)
                .map(|i| {
                    let a = i as f32 / 32. * std::f32::consts::TAU;
                    point(16. + 12. * a.cos(), 13. + 8. * a.sin())
                })
                .collect();
            p.add(Shape::line(
                points,
                Stroke::new(1.5, Color32::from_gray(65)),
            ));
            p.line_segment(
                [point(7., 19.), point(12., 29.)],
                Stroke::new(1.5, Color32::from_gray(65)),
            );
        }
        ToolKind::Eraser => {
            polygon(
                &[(3., 20.), (16., 5.), (29., 13.), (16., 28.)],
                Color32::from_rgb(224, 140, 151),
            );
            polygon(
                &[(3., 20.), (16., 28.), (16., 31.), (3., 23.)],
                Color32::from_rgb(167, 91, 106),
            );
            polygon(
                &[(16., 28.), (29., 13.), (29., 17.), (16., 31.)],
                Color32::from_rgb(191, 113, 129),
            );
            polygon(
                &[(11., 11.), (16., 5.), (29., 13.), (24., 19.)],
                Color32::from_rgb(247, 226, 226),
            );
            p.line_segment(
                [point(16., 7.), point(26., 13.)],
                Stroke::new(1.2, Color32::WHITE),
            );
        }
    }
}
