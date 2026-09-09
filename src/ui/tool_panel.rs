use eframe::egui;

use crate::core::{
    contact::pencil_effective_pressure,
    document::Document,
    pencil::{EraserKind, PencilGrade, PencilTipState, ToolKind, ToolSettings},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPanelAction {
    None,
    SharpenPencil,
    CleanSmudger,
    ImportBrush,
}

pub fn show_tool_panel(
    ui: &mut egui::Ui,
    settings: &mut ToolSettings,
    tip: &mut PencilTipState,
    document: &Document,
    status: &str,
    active_device_pressure: Option<f32>,
    device_pressure_seen: bool,
    active_device_tilt_deg: Option<f32>,
    active_device_rotation_deg: Option<f32>,
    device_rotation_seen: bool,
    device_orientation_seen: bool,
    smudger_load_fraction: f32,
    _statistics: (f32, f32),
) -> ToolPanelAction {
    let mut action = ToolPanelAction::None;
    ui.set_width(ui.available_width().max(266.0));
    ui.heading(settings.tool.label());
    ui.small("TOOL OPTIONS");
    ui.separator();

    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("Pencil color");
        ui.color_edit_button_srgb(&mut settings.pencil_color_rgb)
            .on_hover_text("Selected pigment RGB. Shade variation adds related tones; paper, pressure and layer blending affect the visible result.");
    });
    ui.add_space(6.0);

    ui.collapsing("Pen & touch", |ui| {
        #[cfg(windows)]
        ui.small("Choose Windows Ink or Wintab in Options → Input backend.");
        ui.add(egui::Slider::new(&mut settings.pen_pressure_gamma,0.55..=1.8).text("Pressure feel"))
            .on_hover_text("Below 1: responds to a lighter touch. Above 1: needs firmer pressure. Applies to device pressure only.");
        if ui.small_button("Reset pressure feel").clicked() { settings.pen_pressure_gamma=1.0; }
        ui.small("Use Windows Ink for EasyCanvas and current Wacom drivers. Try Wintab for an older Bamboo, XP-Pen Star 03 or another Wintab setup. Tilt and barrel rotation require a pen that reports them; unsupported sensors use the manual settings.");
        ui.small("Drag with two fingers to move the paper, pinch to zoom, and twist to rotate. R selects mouse view rotation; P or B selects Pencil; E selects Eraser.");
    });

    if let Some(rotation) = active_device_rotation_deg {
        ui.small(format!("Device barrel rotation: {rotation:.0}°"));
    } else if device_rotation_seen {
        ui.small("Barrel rotation detected. Manual tip rotation is used when no live rotation is available.");
    }

    match settings.tool {
        ToolKind::VectorSelect => {
            ui.label("Click an individual stroke on the active layer.");
            ui.label("Drag corners to resize; drag the round handle to rotate.");
            ui.label("Drag inside to move. Enter applies; Esc cancels.");
            ui.small("The checkmark confirms and deselects. The X cancels and leaves the selection tool.");
            ui.small("V selects this tool. Paths keep their pencil texture. Choose another layer in Layers to select its strokes.");
        }
        ToolKind::Brush => {
            if ui.button("Import ABR brushes…").clicked() {
                action = ToolPanelAction::ImportBrush;
            }
            ui.small("Sampled tips · Photoshop dynamics are not imported");
            ui.add(
                egui::Slider::new(&mut settings.brush_size_px, 1.0..=800.0)
                    .text("Size")
                    .suffix(" px"),
            );
            ui.add(egui::Slider::new(&mut settings.flow, 0.0..=2.0).text("Flow"));
            ui.add(egui::Slider::new(&mut settings.mouse_pressure, 0.03..=1.0).text("Pressure"));
        }
        ToolKind::Tissue => {
            ui.small("A graphite-loaded tissue stains the active layer with a soft wash. Build tone with repeated passes.");
            ui.add(
                egui::Slider::new(&mut settings.tissue_size_px, 10.0..=200.0)
                    .clamping(egui::SliderClamping::Edits)
                    .text("Size")
                    .suffix(" px"),
            );
            let mut load = settings.tissue_load * 100.;
            if ui
                .add(
                    egui::Slider::new(&mut load, 0.0..=100.0)
                        .text("Graphite load")
                        .suffix("%"),
                )
                .changed()
            {
                settings.tissue_load = load / 100.;
            }
            let mut random = settings.tissue_random_graphite * 100.;
            if ui.add(egui::Slider::new(&mut random, 0.0..=100.0)
                .text("Random graphite").suffix("%"))
                .on_hover_text("Vary graphite density in soft patches while keeping the chosen color. 0% preserves uniform loading.")
                .changed() {
                settings.tissue_random_graphite = random / 100.;
            }
            ui.add(egui::Slider::new(&mut settings.flow, 0.0..=2.0).text("Flow"));
        }
        ToolKind::Shapes => {
            egui::ComboBox::from_label("Shape")
                .selected_text(settings.shape.label())
                .show_ui(ui, |ui| {
                    for kind in crate::core::shapes::ShapeKind::ALL {
                        ui.selectable_value(&mut settings.shape, kind, kind.label());
                    }
                });
            if settings.shape == crate::core::shapes::ShapeKind::Line {
                ui.checkbox(&mut settings.shape_line_snap, "Snap shape")
                    .on_hover_text("Snap lines to 15° increments without holding Shift.");
            }
            ui.add(
                egui::Slider::new(&mut settings.shape_width_px, 1.0..=80.0)
                    .text("Stroke")
                    .suffix(" px"),
            );
            let mut opacity = (settings.shape_opacity * 100.).round();
            if ui
                .add(
                    egui::Slider::new(&mut opacity, 0.0..=100.0)
                        .text("Shape opacity")
                        .suffix("%")
                        .integer(),
                )
                .changed()
            {
                settings.shape_opacity = opacity / 100.;
            }
            ui.small("Uses the pencil's color, grade, shade variation and tip texture. Set those with the Pencil tool. Shape opacity affects new outlines; layer opacity affects the whole layer.");
            ui.small("Drag to preview; release to draw. Esc cancels. Ctrl+T moves/resizes an outlined selection; Ctrl+R rotates it.");
            ui.small("Hold Shift: lines snap to 15° increments; triangles become equilateral. Circles and squares always keep equal proportions. Keep Shift held when releasing to place the snapped shape.");
        }
        ToolKind::Lasso => {
            ui.label("Trace a freehand selection around your marks.");
            ui.label("Finish tracing to show resize and rotation handles.");
            ui.label("Ctrl+R · rotate");
            ui.label("Ctrl+D · deselect");
            ui.label("Delete · clear selected marks");
            ui.small("The checkmark confirms and deselects. The X or another tool cancels the transform and clears the selection.");
        }
        ToolKind::Pencil => {
            if ui.button("Import ABR textures…").clicked() {
                action = ToolPanelAction::ImportBrush;
            }
            if settings.pencil_texture.is_some() {
                ui.small("Imported tip shape uses the pencil's pressure, tilt, wear and paper grain. Photoshop stamp dynamics are not imported.");
            }
            let mut width_response = settings.pressure_width * 100.;
            let width_control=ui.add(egui::Slider::new(&mut width_response, 0.0..=100.0).text("Width response").suffix("%").integer())
                .on_hover_text("Higher: thinner light strokes and more thin-to-thick variation. Full pressure keeps the broad footprint. 0% restores the previous width response. With a mouse, width uses the fixed fallback pressure.");
            #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("pencil_width_response"),width_control.rect));
            if width_control.changed() {
                settings.pressure_width = width_response / 100.;
            }
            let mut smoothing = settings.line_smoothing * 100.;
            let smoothing_control=ui
                .add(
                    egui::Slider::new(&mut smoothing, 0.0..=100.0)
                        .text("Line smoothing")
                        .suffix("%")
                        .integer(),
                );
            #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("pencil_line_smoothing"),smoothing_control.rect));
            if smoothing_control.changed() {
                settings.line_smoothing = smoothing / 100.;
            }
            ui.small("0%: no added stabilization. Higher values reduce wobble with more pen lag.");
            egui::ComboBox::from_label("Grade")
                .selected_text(settings.grade.label())
                .show_ui(ui, |ui| {
                    for grade in PencilGrade::ALL {
                        ui.selectable_value(&mut settings.grade, grade, grade.label());
                    }
                });

            egui::ComboBox::from_id_salt("pencil_color_presets")
                .selected_text("Color presets")
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut settings.pencil_color_rgb,
                        [104, 104, 104],
                        "Graphite",
                    );
                    ui.selectable_value(
                        &mut settings.pencil_color_rgb,
                        [88, 80, 74],
                        "Warm graphite",
                    );
                    ui.selectable_value(&mut settings.pencil_color_rgb, [112, 72, 46], "Sepia");
                    ui.selectable_value(&mut settings.pencil_color_rgb, [132, 58, 48], "Sanguine");
                    ui.selectable_value(&mut settings.pencil_color_rgb, [48, 61, 106], "Indigo");
                })
                .response
                .on_hover_text(
                    "Quick pencil-color shades. The color picker above allows any custom color.",
                );
            ui.add(
                egui::Slider::new(&mut settings.particle_variation, 0.0..=0.35)
                    .text("Shade variation"),
            )
            .on_hover_text("Adds lighter and deeper shades of the selected pencil color. Grain stays fixed on the paper and moves with deposited material when smudged.");

            size_in_pixels(ui, &mut settings.pencil_core_diameter_mm, 1.5..=(200.*25.4/document.spec.dpi),
                document.spec.dpi, "Core diameter (px)")
                .on_hover_text("Graphite-core diameter in document pixels. Tilt exposes the broad side; upright strokes use the sharpened point. Changing this value does not resize or resample the canvas.");
            let mut sharpness = settings.tip_sharpness * 100.;
            if ui.add(egui::Slider::new(&mut sharpness, 0.0..=100.0).text("Tip sharpness").suffix("%").integer())
                .on_hover_text("0%: blunt · 50%: standard · 100%: fine. Changes new strokes without changing grade, core size or existing marks. Sharpen pencil restores the worn surface at this setting.").changed() {
                settings.tip_sharpness = sharpness / 100.;
            }
            ui.add(
                egui::Slider::new(&mut settings.mouse_pressure, 0.03..=1.0)
                    .text("Mouse pressure fallback"),
            );
            show_pressure_status(
                ui,
                active_device_pressure,
                device_pressure_seen,
                active_device_tilt_deg,
                device_orientation_seen,
            );
            ui.small("Pressure moves the contact down the tapered point: it changes both line width and graphite transfer. Tilt exposes the broad side facet. Supported pen rotation turns the worn tip independently.");
            // Keep the stored transfer value for exact replay of older strokes.
            // One opacity control replaces the former material-flow slider.
            let mut opacity=settings.flow*50.;
            let opacity_control=ui.add(egui::Slider::new(&mut opacity,0.0..=100.0).text("Opacity").suffix("%").max_decimals(1))
                .on_hover_text("Pencil mark strength. 0% adds no marks; higher values lay down denser pigment. Pressure and paper texture still affect the stroke. Saved separately for each pencil.");
            #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("pencil_opacity"),opacity_control.rect));
            if opacity_control.changed(){settings.flow=opacity/50.;}
            ui.add(egui::Slider::new(&mut settings.tilt_deg, 0.0..=80.0).text("Tilt"));
            ui.add(egui::Slider::new(&mut settings.azimuth_deg, 0.0..=360.0).text("Azimuth"));
            ui.checkbox(&mut settings.auto_azimuth, "Auto azimuth from stroke direction")
                .on_hover_text("When true, broad side-of-pencil orientation follows stroke motion whenever the stylus backend does not provide live orientation packets.");

            ui.add(
                egui::Slider::new(&mut tip.profile.orientation_deg, 0.0..=360.0)
                    .text("Tip rotation"),
            )
            .on_hover_text("Rotates the persistent worn graphite profile around the pencil barrel. A different rotation can expose a sharper unworn facet without sharpening the pencil.");
            ui.horizontal(|ui| {
                if ui.small_button("Rotate -15°").clicked() {
                    tip.profile.rotate_by(-15.0);
                }
                if ui.small_button("Rotate +15°").clicked() {
                    tip.profile.rotate_by(15.0);
                }
            });

            if ui
                .button("Sharpen pencil")
                .on_hover_text(
                    "Restore a fresh tip at your selected sharpness, keeping pencil rotation.",
                )
                .clicked()
            {
                action = ToolPanelAction::SharpenPencil;
            }

            ui.collapsing("Pencil details", |ui| {
            let f = settings.grade.formulation();
            ui.add_space(6.0);
            ui.label(format!(
                "Core: {:.0}% graphite · {:.0}% clay · {:.0}% wax",
                f.graphite_fraction * 100.0,
                f.clay_fraction * 100.0,
                f.wax_fraction * 100.0
            ));
            ui.label(format!("Core hardness: {:.0}%", f.core_hardness * 100.0));
            ui.label(format!(
                "Relative wear: {:.0}%",
                f.wear_coefficient / 1.43 * 100.0
            ));
            ui.label(format!(
                "Tip profile wear: {:.1}%",
                tip.wear_fraction() * 100.0
            ));
            ui.label(format!("Tip rotation: {:.0}°", tip.profile.orientation_deg));
            ui.label(format!(
                "Core diameter: {:.2} mm ({:.1} px at {:.0} dpi)",
                tip.core_diameter_mm,
                tip.effective_core_diameter_px(document.spec.dpi),
                document.spec.dpi
            ));
            ui.small("Grade recipes are generic research-informed approximations, not universal manufacturer specifications.");
            });
        }
        ToolKind::Smudge => {
            ui.add(
                egui::Slider::new(&mut settings.smudge_size_px, 2.0..=128.0)
                    .logarithmic(true)
                    .text("Smudge size (px)"),
            )
            .on_hover_text("Contact diameter of the blending stump in document pixels.");
            ui.add(
                egui::Slider::new(&mut settings.mouse_pressure, 0.03..=1.0)
                    .text("Mouse pressure fallback"),
            );
            show_pressure_status(
                ui,
                active_device_pressure,
                device_pressure_seen,
                active_device_tilt_deg,
                device_orientation_seen,
            );
            ui.add(
                egui::Slider::new(&mut settings.smudge_strength, 0.10..=1.40)
                    .text("Transfer"),
            )
            .on_hover_text("Controls how readily the stump picks up, carries and redeposits mobile graphite.");

            ui.label(format!("Stump load: {:.0}%", smudger_load_fraction * 100.0));
            if ui.button("Clean smudger").clicked() {
                action = ToolPanelAction::CleanSmudger;
            }
            ui.small("The smudger transports real simulated graphite/clay/wax. Loose graphite moves readily; compacted deposits resist movement. A loaded stump can leave graphite on lighter paper as you drag.");
        }
        ToolKind::Eraser => {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut settings.eraser_kind, EraserKind::Vinyl, "Vinyl");
                ui.selectable_value(&mut settings.eraser_kind, EraserKind::Kneaded, "Kneaded");
            });
            size_in_pixels(
                ui,
                &mut settings.eraser_diameter_mm,
                0.8..=(200.0 * 25.4 / document.spec.dpi),
                document.spec.dpi,
                "Eraser diameter (px)",
            );
            ui.add(
                egui::Slider::new(&mut settings.mouse_pressure, 0.03..=1.0)
                    .text("Mouse pressure fallback"),
            );
            show_pressure_status(
                ui,
                active_device_pressure,
                device_pressure_seen,
                active_device_tilt_deg,
                device_orientation_seen,
            );
            let mut lift = settings.eraser_strength * 100.;
            if ui
                .add(
                    egui::Slider::new(&mut lift, 0.0..=100.0)
                        .text("Lift strength")
                        .suffix("%")
                        .integer(),
                )
                .changed()
            {
                settings.eraser_strength = lift / 100.;
            }
            ui.small("0%: no erasing. 100%: completely removes material under the eraser from the active layer.");
            ui.small(match settings.eraser_kind {
                EraserKind::Vinyl=>"Below 100%, rub to lift graphite progressively. Stronger pressure reaches more compacted material.",
                EraserKind::Kneaded=>"Gently lifts loose graphite to lighten tones. Tap for a small highlight or make repeated passes for a gradual lift.",
            });
        }
    }

    ui.separator();
    ui.small(status);

    action
}

fn size_in_pixels(
    ui: &mut egui::Ui,
    mm: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    dpi: f32,
    label: &str,
) -> egui::Response {
    let scale = dpi / 25.4;
    let mut px = *mm * scale;
    let response = ui.add(
        egui::Slider::new(&mut px, *range.start() * scale..=*range.end() * scale)
            .clamping(egui::SliderClamping::Edits)
            .custom_formatter(|value, _| format!("{value:.2}"))
            .text(label),
    );
    // Merely displaying the control must never quantize or change stored dimensions.
    if response.changed() {
        *mm = px / scale;
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn displaying_pixel_controls_does_not_quantize_physical_sizes() {
        for dpi in [36.0, 120.0, 300.0, 600.0] {
            let ctx = egui::Context::default();
            let mut pencil = 2.037_f32;
            let mut eraser = 6.179_f32;
            for _ in 0..3 {
                let _ = ctx.run_ui(Default::default(), |ui| {
                    size_in_pixels(ui, &mut pencil, 1.5..=4.0, dpi, "Core diameter (px)");
                    size_in_pixels(ui, &mut eraser, 0.8..=(200.0 * 25.4 / dpi), dpi, "Eraser diameter (px)");
                });
            }
            assert_eq!(pencil, 2.037);
            assert_eq!(eraser, 6.179);
        }
    }
}

fn show_pressure_status(
    ui: &mut egui::Ui,
    active_device_pressure: Option<f32>,
    device_pressure_seen: bool,
    active_device_tilt_deg: Option<f32>,
    device_orientation_seen: bool,
) {
    match active_device_pressure {
        Some(pressure) => {
            ui.label(format!(
                "Device pressure: {:.0}% · pencil load: {:.0}%",
                pressure * 100.0,
                pencil_effective_pressure(pressure) * 100.0,
            ));
        }
        None if device_pressure_seen => {
            ui.small(
                "Pressure-sensitive pen/touch detected. Lifted: mouse fallback is shown above.",
            );
        }
        None => {
            ui.small("No pressure packet detected yet. Mouse fallback uses subtle speed dynamics; real pen pressure overrides it automatically when available.");
        }
    }

    match active_device_tilt_deg {
        Some(tilt) => {
            ui.label(format!("Device tilt: {:.0}°", tilt));
        }
        None if device_orientation_seen => {
            ui.small(
                "Stylus orientation detected. Lifted: manual tilt/azimuth sliders are shown above.",
            );
        }
        None => {
            ui.small("No live tilt/orientation packet detected. Manual tilt is used; auto azimuth can still follow stroke direction.");
        }
    }
}
