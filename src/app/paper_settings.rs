use super::*;
pub(super) struct PaperSettingsUi {
    pub open: bool,
    pub width_mm: f32,
    pub height_mm: f32,
    pixels: bool,
}
impl Default for PaperSettingsUi {
    fn default() -> Self {
        Self {
            open: false,
            width_mm: 210.,
            height_mm: 297.,
            pixels: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_paper_creation_and_repeated_orientation_reset() {
        let mut a = GraphiteApp::with_render_state(None);
        a.replace_document(CanvasSpec::from_physical("Existing", 40., 60., 120.));
        let old = a.active_tab;
        a.paper_settings.width_mm = 80.;
        a.paper_settings.height_mm = 45.;
        a.canvas_preset = CanvasPreset::Custom;
        a.new_document_dpi = 150.;
        let s = a.configured_paper_spec(CanvasPreset::Custom, 150.);
        assert_eq!((s.width_px, s.height_px), (472, 266));
        a.replace_document(s);
        assert_ne!(old, a.active_tab);
        for landscape in [true, true, false, false] {
            a.viewport.rotation = 1.2;
            a.viewport.zoom = 2.75;
            a.rotate_view = true;
            a.handle_topbar_action(TopbarAction::CanvasOrientation(landscape));
            assert_eq!(
                a.document.spec.width_px > a.document.spec.height_px,
                landscape
            );
            assert_eq!(a.viewport.rotation, 0.);
            assert!(!a.rotate_view);
            assert!(!a.fit_requested);
            assert_eq!(a.viewport.zoom,2.75);
        }
        let ctx = egui::Context::default();
        a.paper_settings.open = true;
        let mut preview = crate::ui::test_render::Preview::default();
        for pass in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    ..Default::default()
                },
                |ui| a.interface(ui),
            );
            preview.update(&output);
            if pass == 2 {
                preview.save(&ctx, &output, "../../ui-v230-paper.png", [1440, 920]);
            }
        }
        let before = a.active_tab;
        a.document.set_paper_color([240, 230, 210]);
        a.document.set_paper_texture_opacity(0.42);
        let pos = ctx.data(|d| {
            d.get_temp::<Rect>(egui::Id::new("paper_create"))
                .unwrap()
                .center()
        });
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ui| a.interface(ui),
            );
        }
        assert_ne!(before, a.active_tab);
        assert!(!a.paper_settings.open);
        assert_eq!(
            (a.document.spec.width_px, a.document.spec.height_px),
            (472, 266)
        );
        assert_eq!(a.document.paper_color_rgb, [240, 230, 210]);
        assert_eq!(a.document.paper_texture_opacity,0.42);
    }
}
impl GraphiteApp {
    pub(super) fn configured_paper_spec(&self, preset: CanvasPreset, dpi: f32) -> CanvasSpec {
        if preset == CanvasPreset::Custom {
            CanvasSpec::from_physical(
                "Custom",
                self.paper_settings.width_mm.clamp(1., 2000.),
                self.paper_settings.height_mm.clamp(1., 2000.),
                dpi.clamp(36., 600.),
            )
        } else {
            Self::canvas_spec_for(preset, dpi, self.new_landscape)
        }
    }
    pub(super) fn show_paper_settings(&mut self, ctx: &egui::Context) {
        if !self.paper_settings.open {
            return;
        }
        let mut open = true;
        let mut action = TopbarAction::None;
        let mut create = false;
        egui::Window::new("Paper settings").fade_in(false).fade_out(false).open(&mut open).default_pos(Pos2::new(150.,80.)).default_size(Vec2::new(480.,720.)).resizable(true).show(ctx,|ui|{
            ui.spacing_mut().interact_size.y=40.;
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-150.).max(250.)).show(ui,|ui|{
                ui.heading("Current paper");
                ui.label(format!("{} · {}",self.document.spec.name,self.document.paper.label()));
                ui.label(format!("{:.2} × {:.2} mm",self.document.spec.width_mm,self.document.spec.height_mm));
                ui.label(format!("{} × {} px · {:.0} DPI",self.document.spec.width_px,self.document.spec.height_px,self.document.spec.dpi));
                ui.horizontal(|ui|{
                    for (landscape,label,icon) in [(false,"Portrait",crate::ui::action_icons::ActionIcon::Portrait),(true,"Landscape",crate::ui::action_icons::ActionIcon::Landscape)] {
                        let response=crate::ui::action_icons::button_sized(ui,icon,label,(self.document.spec.width_px>self.document.spec.height_px)==landscape,Vec2::splat(44.));
                        #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new(("paper_orientation",landscape)),response.rect));
                        if response.clicked() {
                            action=TopbarAction::CanvasOrientation(landscape);
                            if (self.paper_settings.width_mm>self.paper_settings.height_mm)!=landscape {
                                std::mem::swap(&mut self.paper_settings.width_mm,&mut self.paper_settings.height_mm);
                            }
                        }
                    }
                });
                ui.small("Resets view rotation and keeps your zoom. Changing orientation rotates the artwork too; Undo restores it.");
                ui.horizontal(|ui|{
                    ui.label("Paper color");let mut color=self.document.paper_color_rgb;
                    if ui.color_edit_button_srgb(&mut color).changed(){self.document.set_paper_color(color);self.tabs[self.active_tab].modified=true;}
                });
                let before=self.paper_texture_choice;
                egui::ComboBox::from_label("Texture").selected_text(self.paper_texture_choice.label(self.custom_paper_texture.as_ref().map(|p|p.name.as_str()))).show_ui(ui,|ui|{
                    for choice in [PaperTextureChoice::White,PaperTextureChoice::Recycled,PaperTextureChoice::Ivory,PaperTextureChoice::Custom]{
                        ui.selectable_value(&mut self.paper_texture_choice,choice,choice.label(self.custom_paper_texture.as_ref().map(|p|p.name.as_str())));
                    }
                });
                if ui.button("Load paper texture…").clicked() || (before!=self.paper_texture_choice && self.paper_texture_choice==PaperTextureChoice::Custom && self.custom_paper_texture.is_none()) {action=TopbarAction::LoadCustomPaperTexture;}
                else if before!=self.paper_texture_choice{action=TopbarAction::ApplyPaperTexture;}
                let mut opacity=self.document.paper_texture_opacity*100.;
                let response=ui.add(egui::Slider::new(&mut opacity,0.0..=100.0).text("Texture opacity").suffix("%").integer())
                    .on_hover_text("Fade the visible texture toward the paper color. 0% is flat color; 100% shows the full texture. The paper's drawing behavior stays the same.");
                #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("paper_texture_opacity"),response.rect));
                if response.changed() && self.document.set_paper_texture_opacity(opacity/100.) {self.tabs[self.active_tab].modified=true;}
                ui.collapsing("Surface information",|ui|{
                    ui.label(format!("Material coverage: {:.2}%",self.sidebar_statistics.1*100.));
                    ui.label(format!("Surface disturbance: {:.4}%",self.sidebar_statistics.2*100.));
                    ui.small("Pencil pressure and erasing can flatten or abrade the paper's raised fibers.");
                });
                ui.separator();ui.heading("New drawing");
                egui::ComboBox::from_label("Paper size").selected_text(self.canvas_preset.label()).show_ui(ui,|ui|{
                    for p in [CanvasPreset::A5,CanvasPreset::A4,CanvasPreset::Letter,CanvasPreset::Custom]{ui.selectable_value(&mut self.canvas_preset,p,p.label());}
                });
                ui.horizontal(|ui|{ui.label("DPI");ui.add(egui::DragValue::new(&mut self.new_document_dpi).range(36.0..=600.0).max_decimals(0));});
                if self.canvas_preset==CanvasPreset::Custom{
                    ui.horizontal(|ui|{ui.selectable_value(&mut self.paper_settings.pixels,false,"mm");ui.selectable_value(&mut self.paper_settings.pixels,true,"px");});
                    let scale=if self.paper_settings.pixels{self.new_document_dpi/25.4}else{1.};
                    for (label,value) in [("Width",&mut self.paper_settings.width_mm),("Height",&mut self.paper_settings.height_mm)]{
                        ui.horizontal(|ui|{ui.label(label);let mut displayed=*value*scale;
                            if ui.add(egui::DragValue::new(&mut displayed).range(scale..=2000.*scale).speed(1.).max_decimals(2)).changed(){*value=displayed/scale;}
                        });
                    }
                }
                let spec=self.configured_paper_spec(self.canvas_preset,self.new_document_dpi);
                ui.label(format!("{:.2} × {:.2} mm · {} × {} px",spec.width_mm,spec.height_mm,spec.width_px,spec.height_px));
                let valid=graphite_studio::limits::canvas_pixels(spec.width_px,spec.height_px).is_ok();
                if !valid{ui.label("Dimensions must be nonzero and representable in memory.");}
                let response=ui.add_enabled(valid,egui::Button::new("Create drawing"));
                #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("paper_create"),response.rect));
                create=response.clicked();
                ui.small("Dimensions and DPI create a new tab. Paper color and texture above apply to the current drawing and carry into the new one.");
            });
        });
        self.paper_settings.open = open;
        if action != TopbarAction::None {
            self.handle_topbar_action(action);
        }
        if create {
            let color = self.document.paper_color_rgb;
            let opacity=self.document.paper_texture_opacity;
            let old = self.active_tab;
            self.handle_topbar_action(TopbarAction::NewCanvas {
                preset: self.canvas_preset,
                dpi: self.new_document_dpi,
            });
            if old != self.active_tab {
                self.document.set_paper_color(color);
                self.document.set_paper_texture_opacity(opacity);
                self.paper_settings.open = false;
            }
        }
    }
}
