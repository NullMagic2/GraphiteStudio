use super::*;
use std::path::Path;
#[derive(Default, Clone, Copy, PartialEq)]
pub(super) enum SaveAsFormat {
    #[default]
    Psd,
    Graphite,
    Png,
    Jpeg,
    Bmp,
}
impl SaveAsFormat {
    const ALL: [Self; 5] = [Self::Psd, Self::Graphite, Self::Png, Self::Jpeg, Self::Bmp];
    fn label(self) -> &'static str {
        match self {
            Self::Psd => "Photoshop document (PSD)",
            Self::Graphite => "Graphite project",
            Self::Png => "PNG image",
            Self::Jpeg => "JPEG image",
            Self::Bmp => "Bitmap image",
        }
    }
    fn extension(self) -> &'static str {
        match self {
            Self::Psd => "psd",
            Self::Graphite => "graphite",
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Bmp => "bmp",
        }
    }
    fn editable(self) -> bool {
        matches!(self, Self::Psd | Self::Graphite)
    }
}
impl GraphiteApp {
    pub(super) fn save_project(&mut self, save_as: bool) {
        self.finish_stroke();
        self.apply_transform();
        let path = if !save_as {
            self.tabs[self.active_tab].project_path.clone()
        } else {
            None
        };
        if let Some(path) = path {
            self.save_document_path(&path);
        } else {
            self.save_as_open = true;
        }
    }
    pub(super) fn show_save_as(&mut self, ctx: &egui::Context) {
        if !self.save_as_open {
            return;
        }
        let mut choose = false;
        let mut cancel = false;
        let response=egui::Modal::new(egui::Id::new("save_project_as")).show(ctx,|ui|{
            ui.set_width(370.);ui.heading("Save project as…");ui.add_space(10.);
            ui.horizontal(|ui|{ui.label("Format");egui::ComboBox::from_id_salt("save_format").selected_text(self.save_as_format.label()).width(270.).show_ui(ui,|ui|{
                for format in SaveAsFormat::ALL {ui.selectable_value(&mut self.save_as_format,format,format.label());}
            });});
            if self.save_as_format==SaveAsFormat::Psd {
                ui.horizontal(|ui|{ui.label("Bit depth");egui::ComboBox::from_id_salt("save_psd_depth").selected_text(self.psd_bit_depth.label()).show_ui(ui,|ui|{
                    for depth in PsdBitDepth::ALL{ui.selectable_value(&mut self.psd_bit_depth,depth,depth.label());}
                });});
            }
            ui.add_space(8.);
            ui.label(if self.save_as_format.editable(){"Keeps layers and editable paths."}else{"Saves a flattened image. Save a PSD or Graphite project to keep layers and editable paths."});
            ui.add_space(12.);ui.horizontal(|ui|{
                choose=ui.add_sized([170.,44.],egui::Button::new("Choose location…")).clicked();
                let button=ui.add_sized([110.,44.],egui::Button::new("Cancel"));
                #[cfg(test)] ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new("save_cancel"),button.rect));
                cancel=button.clicked();
            });
        });
        if cancel || response.should_close() {
            self.save_as_open = false;
        }
        if choose {
            let format = self.save_as_format;
            if let Some(mut path) = rfd::FileDialog::new()
                .set_title("Save project as…")
                .add_filter(format.label(), &[format.extension()])
                .set_file_name(format!(
                    "{}.{}",
                    self.tabs[self.active_tab].title,
                    format.extension()
                ))
                .save_file()
            {
                if path.extension().is_none() {
                    path.set_extension(format.extension());
                }
                self.save_document_path(&path);
                self.save_as_open = false;
            }
        }
    }
    pub(super) fn save_document_path(&mut self, path: &Path) {
        self.finish_stroke();
        self.apply_transform();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("graphite") || e.eq_ignore_ascii_case("psd"))
        {
            match self.save_project_path(path) {
                Ok(()) => self.status = format!("Saved editable project to {}", path.display()),
                Err(e) => self.status = format!("Project not saved: {e}"),
            }
        } else {
            self.save_image_path(path);
        }
    }
    pub(super) fn save_project_path(&mut self, path: &Path) -> Result<(), String> {
        self.finish_stroke();
        self.apply_transform();
        let data = graphite_studio::project::ProjectRef {
            document: &self.document,
            settings: &self.settings,
            tip: &self.tip_state,
            engine: &self.stroke_engine,
            brushes: &self.brush_library,
            custom_paper: &self.custom_paper_texture,
            zoom: self.viewport.zoom,
            pan: [self.viewport.free_pan.x, self.viewport.free_pan.y],
        };
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("psd"))
        {
            graphite_studio::project::save_psd(
                path,
                &data,
                &mut self.renderer,
                self.psd_bit_depth,
            )?;
        } else {
            graphite_studio::project::save(path, &data)?;
        }
        let tab = &mut self.tabs[self.active_tab];
        tab.project_path = Some(path.to_owned());
        tab.modified = false;
        if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
            tab.title = name.to_owned();
        }
        Ok(())
    }
    pub(super) fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open editable Graphite project")
            .add_filter("Graphite / Photoshop project", &["graphite", "psd"])
            .pick_file()
        else {
            return;
        };
        if let Err(e) = self.open_project_path(&path) {
            self.status = format!("Project not opened: {e}");
        }
    }
    pub(super) fn open_project_path(&mut self, path: &Path) -> Result<(), String> {
        let mut project = graphite_studio::project::load(path)?;
        if project.settings.tool == ToolKind::Brush {
            project.settings.tool = ToolKind::Pencil;
            project.settings.pencil_texture = project.settings.brush_tip.clone();
        }
        let pixels = self.document.spec.pixel_count()
            + self
                .tabs
                .iter()
                .filter_map(|t| t.stored.as_ref())
                .map(|s| s.document.spec.pixel_count())
                .sum::<usize>();
        if pixels + project.document.spec.pixel_count() > 24_000_000 {
            return Err("Save and close another drawing before opening this project.".into());
        }
        let choice = if project.custom_paper.is_some() {
            PaperTextureChoice::Custom
        } else {
            match project.document.paper_texture_label.as_str() {
                "Recycled" => PaperTextureChoice::Recycled,
                "Ivory" => PaperTextureChoice::Ivory,
                _ => PaperTextureChoice::White,
            }
        };
        let dpi = project.document.spec.dpi;
        let state = DrawingState {
            document: project.document,
            history: History::default(),
            viewport: ViewportState {
                rotation: 0.,
                zoom: project.zoom,
                free_pan: Vec2::new(project.pan[0], project.pan[1]),
            },
            stroke_engine: project.engine,
            settings: project.settings,
            tip_state: project.tip,
            canvas_preset: self.canvas_preset,
            paper_texture_choice: choice,
            custom_paper_texture: project.custom_paper,
            new_document_dpi: dpi,
            psd_bit_depth: self.psd_bit_depth,
            fit_requested: false,
            status: format!("Opened editable project {}", path.display()),
        };
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(DrawingTab {
            project_path: Some(path.to_owned()),
            modified: false,
            id,
            title: path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("Project")
                .to_owned(),
            stored: Some(state),
        });
        self.activate_tab(self.tabs.len() - 1);
        self.brush_library = project.brushes;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unified_save_preserves_editable_projects_and_image_copy_state() {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Save formats", 12., 16., 120.));
        let start = StrokePoint {
            x: 15.,
            y: 20.,
            pressure: 0.7,
            tilt_deg: 8.,
            azimuth_deg: 20.,
            rotation_deg: None,
        };
        let mut tx = EditTransaction::default();
        app.stroke_engine.apply_segment(
            &mut app.document,
            &app.settings,
            &mut app.tip_state,
            start,
            StrokePoint {
                x: 35.,
                y: 45.,
                ..start
            },
            &mut tx,
        );
        app.tabs[app.active_tab].modified = true;
        let root =
            std::env::temp_dir().join(format!("graphite-unified-save-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let project = root.join("Drawing.graphite");
        app.save_document_path(&project);
        assert_eq!(
            app.tabs[app.active_tab].project_path.as_ref(),
            Some(&project)
        );
        for extension in ["png", "jpg", "bmp"] {
            app.tabs[app.active_tab].modified = true;
            let path = root.join(format!("Copy.{extension}"));
            app.save_document_path(&path);
            assert!(image::open(&path).is_ok(), "{}", app.status);
            assert!(app.tabs[app.active_tab].modified);
            assert_eq!(
                app.tabs[app.active_tab].project_path.as_ref(),
                Some(&project)
            );
            std::fs::remove_file(path).unwrap();
        }
        for depth in PsdBitDepth::ALL {
            app.psd_bit_depth = depth;
            let path = root.join(format!("Drawing-{}.psd", depth.bits()));
            app.save_document_path(&path);
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(u16::from_be_bytes([bytes[22], bytes[23]]), depth.bits());
            let loaded = graphite_studio::project::load(&path).unwrap();
            assert_eq!(
                loaded.document.surface.graphite_mass,
                app.document.surface.graphite_mass
            );
            assert!(!app.tabs[app.active_tab].modified);
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_file(project).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn save_as_contains_format_and_psd_depth_and_cancels_cleanly() {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Dialog", 12., 16., 120.));
        app.save_project(true);
        assert!(app.save_as_open);
        let ctx = egui::Context::default();
        let mut preview = crate::ui::test_render::Preview::default();
        for pass in 0..4 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
            preview.update(&output);
            if pass == 3 {
                preview.save(&ctx, &output, "../../ui-v235-save-as.png", [1440, 920]);
            }
        }
        let before = app.tabs[app.active_tab].modified;
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.interface(ui),
        );
        assert!(!app.save_as_open);
        assert_eq!(before, app.tabs[app.active_tab].modified);
        app.save_project(true);
        for _ in 0..3 {let _=ctx.run_ui(Default::default(),|ui|app.interface(ui));}
        let pos=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("save_cancel")).unwrap().center());
        for pressed in [true,false] {
            let _=ctx.run_ui(egui::RawInput{events:vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}],..Default::default()},|ui|app.interface(ui));
        }
        assert!(!app.save_as_open,"Cancel button did not respond");
    }
}
