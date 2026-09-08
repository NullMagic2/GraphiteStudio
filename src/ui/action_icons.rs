use eframe::egui::{self, Color32, Rect, Shape, Stroke};

#[derive(Clone, Copy)]
pub enum ActionIcon {
    Layers,
    AddLayer,
    DeleteLayer,
    Close,
    Plus,
    Portrait,
    Landscape,
    PaperSettings,
    PencilGallery,
    Fit,
    Fullscreen,
    Undo,
    Redo,
}

pub fn paint(p: &egui::Painter, rect: Rect, icon: ActionIcon, color: Color32) {
    let pt =
        |x: f32, y: f32| rect.min + egui::vec2(x * rect.width() / 24., y * rect.height() / 24.);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], Stroke::new(1.5, color));
    };
    match icon {
        ActionIcon::Undo | ActionIcon::Redo => {
            let point = |x:f32,y:f32| pt(if matches!(icon,ActionIcon::Redo){24.-x}else{x},y);
            p.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [point(4.,8.),point(23.,3.),point(24.,23.),point(12.,20.)],
                false,Color32::TRANSPARENT,Stroke::new(1.8,color),
            ));
            p.add(Shape::line(vec![point(9.,3.),point(4.,8.),point(9.,13.)],Stroke::new(1.8,color)));
        }
        ActionIcon::PaperSettings => {
            p.add(Shape::closed_line(vec![pt(4.,2.),pt(14.,2.),pt(20.,8.),pt(20.,22.),pt(4.,22.)], Stroke::new(1.5,color)));
            p.add(Shape::line(vec![pt(14.,2.),pt(14.,8.),pt(20.,8.)],Stroke::new(1.5,color)));
            for (y,x) in [(12.,10.),(17.,14.)] {
                line((7.,y),(17.,y));
                p.circle_filled(pt(x,y),2.,color);
            }
        }
        ActionIcon::PencilGallery => {
            for x in [3.,10.,17.] {
                p.add(Shape::closed_line(vec![pt(x,8.),pt(x+2.,3.),pt(x+4.,8.),pt(x+4.,21.),pt(x,21.)],Stroke::new(1.3,color)));
                line((x,8.),(x+4.,8.));
                line((x,18.),(x+4.,18.));
            }
        }
        ActionIcon::Fit | ActionIcon::Fullscreen => {
            for (x,y,sx,sy) in [(2.,2.,1.,1.),(22.,2.,-1.,1.),(2.,22.,1.,-1.),(22.,22.,-1.,-1.)] {
                p.add(Shape::line(vec![pt(x,y+5.*sy),pt(x,y),pt(x+5.*sx,y)],Stroke::new(1.5,color)));
            }
            if matches!(icon,ActionIcon::Fit) {
                p.rect_stroke(Rect::from_min_max(pt(7.,5.),pt(17.,19.)),0.,Stroke::new(1.3,color),egui::StrokeKind::Inside);
            } else {
                line((8.,8.),(3.,3.)); line((16.,8.),(21.,3.));
                line((8.,16.),(3.,21.)); line((16.,16.),(21.,21.));
            }
        }
        ActionIcon::Portrait | ActionIcon::Landscape => {
            let (x, y, w, h) = if matches!(icon, ActionIcon::Portrait) {
                (5., 2., 14., 20.)
            } else {
                (2., 5., 20., 14.)
            };
            p.rect(
                egui::Rect::from_min_max(pt(x, y), pt(x + w, y + h)),
                1.,
                Color32::from_rgb(250, 248, 241),
                Stroke::new(1.5, color),
                egui::StrokeKind::Inside,
            );
            p.line_segment(
                [pt(x + 3., y + 4.), pt(x + w - 3., y + 4.)],
                Stroke::new(1., color),
            );
            p.line_segment(
                [pt(x + 3., y + 7.), pt(x + w - 3., y + 7.)],
                Stroke::new(1., color),
            );
        }
        ActionIcon::Layers => {
            p.add(Shape::closed_line(
                vec![pt(2., 8.), pt(12., 3.), pt(22., 8.), pt(12., 13.)],
                Stroke::new(1.5, color),
            ));
            for y in [12., 16.] {
                p.add(Shape::line(
                    vec![pt(2., y), pt(12., y + 5.), pt(22., y)],
                    Stroke::new(1.5, color),
                ));
            }
        }
        ActionIcon::AddLayer => {
            p.add(Shape::closed_line(
                vec![
                    pt(4., 2.),
                    pt(15., 2.),
                    pt(20., 7.),
                    pt(20., 22.),
                    pt(4., 22.),
                ],
                Stroke::new(1.5, color),
            ));
            p.add(Shape::line(
                vec![pt(15., 2.), pt(15., 7.), pt(20., 7.)],
                Stroke::new(1.3, color),
            ));
            line((8., 14.), (16., 14.));
            line((12., 10.), (12., 18.));
        }
        ActionIcon::DeleteLayer => {
            line((3., 6.), (21., 6.));
            line((9., 3.), (15., 3.));
            p.add(Shape::line(
                vec![pt(5., 7.), pt(6., 21.), pt(18., 21.), pt(19., 7.)],
                Stroke::new(1.5, color),
            ));
            line((9., 10.), (9., 18.));
            line((15., 10.), (15., 18.));
        }
        ActionIcon::Close => {
            line((7., 7.), (17., 17.));
            line((17., 7.), (7., 17.));
        }
        ActionIcon::Plus => {
            line((5., 12.), (19., 12.));
            line((12., 5.), (12., 19.));
        }
    }
}

pub fn button(ui: &mut egui::Ui, icon: ActionIcon, label: &str, selected: bool) -> egui::Response {
    button_sized(ui, icon, label, selected, egui::vec2(30., 28.))
}

/// Keep readable labels alongside scalable vector icons and a touch-sized target.
pub fn labeled_button(ui: &mut egui::Ui, icon: ActionIcon, label: &str) -> egui::Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, Color32::PLACEHOLDER);
    let size = egui::vec2(galley.size().x + 42., 44.);
    let (rect,response) = ui.allocate_exact_size(size,egui::Sense::click());
    #[cfg(test)]
    ui.data_mut(|d|d.insert_temp(egui::Id::new(("test_action_icon",label)),rect));
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,ui.is_enabled(),label));
    let v = ui.style().interact(&response);
    ui.painter().rect(rect.shrink(1.),4.,v.bg_fill,v.bg_stroke,egui::StrokeKind::Inside);
    paint(ui.painter(),Rect::from_center_size(egui::pos2(rect.left()+18.,rect.center().y),egui::vec2(22.,22.)),icon,v.text_color());
    ui.painter().galley(egui::pos2(rect.left()+34.,rect.center().y-galley.size().y*0.5),galley,v.text_color());
    response
}
pub fn button_sized(
    ui: &mut egui::Ui,
    icon: ActionIcon,
    label: &str,
    selected: bool,
    size: egui::Vec2,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    #[cfg(test)]
    ui.data_mut(|d| d.insert_temp(egui::Id::new(("test_action_icon", label)), rect));
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    let v = ui.style().interact_selectable(&response, selected);
    ui.painter().rect(
        rect.shrink(1.),
        4.,
        v.bg_fill,
        v.bg_stroke,
        egui::StrokeKind::Inside,
    );
    paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), egui::vec2(20., 20.)),
        icon,
        v.text_color(),
    );
    response.on_hover_text(label)
}
