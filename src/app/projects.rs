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
            if let Some(mut path) = self.file_dialog()
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
        self.remember_recent_file(path);
        Ok(())
    }
    pub(super) fn open_project(&mut self) {
        self.quick_controls.close_size();
        let Some(path) = self.file_dialog()
            .set_title("Open drawing or image")
            .add_filter("All supported drawings and images", graphite_studio::import::OPEN_EXTENSIONS)
            .add_filter("Graphite project", &["graphite"])
            .add_filter("Photoshop document", &["psd"])
            .add_filter("PNG image", &["png"])
            .add_filter("JPEG image", &["jpg", "jpeg"])
            .add_filter("Bitmap image", &["bmp"])
            .pick_file()
        else {
            return;
        };
        self.open_file_with_feedback(&path);
    }
    pub(super) fn open_recent_project(&mut self,index:usize) {
        let Some(path)=self.recent_files.files().get(index).cloned() else {return;};
        self.quick_controls.close_size();
        self.open_file_with_feedback(&path);
    }
    #[cfg(test)]
    pub(super) fn open_project_path(&mut self, path: &Path) -> Result<(), String> {
        let opened = graphite_studio::import::open(path)?;
        self.install_opened_document(path, opened)
    }
    pub(super) fn install_opened_document(&mut self, path: &Path, opened: graphite_studio::import::OpenedDocument) -> Result<(), String> {
        let mut project = opened.project;
        if project.settings.tool == ToolKind::Brush {
            project.settings.tool = ToolKind::Pencil;
            project.settings.pencil_texture = project.settings.brush_tip.clone();
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
            fit_requested: !opened.editable_source,
            status: format!("Opened {}: {}", opened.description, path.display()),
        };
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(DrawingTab {
            project_path: opened.editable_source.then(||path.to_owned()),
            modified: !opened.editable_source,
            id,
            title: path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("Project")
                .to_owned(),
            stored: Some(state),
        });
        self.activate_tab(self.tabs.len() - 1);
        if opened.editable_source {self.brush_library = project.brushes;}
        self.remember_recent_file(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recent_files_track_only_success_and_persist_with_display_preferences() {
        let root=std::env::temp_dir().join(format!("graphite-recent-app-{}",std::process::id()));std::fs::create_dir_all(&root).unwrap();
        let mut app=GraphiteApp::with_render_state(None);app.preferences_path=Some(root.join("settings.json"));
        std::fs::write(root.join("settings.json"),br#"{"acceleration":"IntelHd"}"#).unwrap();
        app.set_recent_files_maximum(2);
        let fixtures=Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/open");
        app.open_project_path(&fixtures.join("rgba.PNG")).unwrap();
        assert_eq!(crate::performance::Preferences::load_from(&root.join("settings.json")).unwrap().acceleration,crate::performance::AccelerationMode::IntelHd);
        app.open_project_path(&fixtures.join("color.bmp")).unwrap();
        app.open_project_path(&fixtures.join("rgba.PNG")).unwrap();
        assert_eq!(app.recent_files.files().len(),2);assert!(app.recent_files.files()[0].ends_with("rgba.PNG"));
        let before=app.recent_files.clone();
        assert!(app.open_project_path(&root.join("missing.png")).is_err());assert_eq!(app.recent_files,before);
        assert!(app.save_project_path(&root.join("missing-folder/failed.graphite")).is_err());assert_eq!(app.recent_files,before);
        app.save_image_path(&root.join("copy.png"));assert!(app.recent_files.files()[0].ends_with("copy.png"));
        app.save_project_path(&root.join("saved.graphite")).unwrap();assert!(app.recent_files.files()[0].ends_with("saved.graphite"));
        let before=app.tabs.len();app.handle_topbar_action(TopbarAction::OpenRecent(1));app.wait_for_opening();assert_eq!(app.tabs.len(),before+1);assert!(app.recent_files.files()[0].ends_with("copy.png"));
        app.set_acceleration(crate::performance::AccelerationMode::IntelHd);
        let saved=crate::performance::Preferences::load_from(app.preferences_path.as_deref().unwrap()).unwrap();assert_eq!(saved.recent_files,app.recent_files);assert_eq!(saved.acceleration,app.acceleration);
        app.set_recent_files_maximum(0);app.open_project_path(&fixtures.join("color.bmp")).unwrap();
        let saved=crate::performance::Preferences::load_from(app.preferences_path.as_deref().unwrap()).unwrap();assert_eq!(saved.recent_files.maximum(),0);assert!(saved.recent_files.files().is_empty());
        app.set_recent_files_maximum(255);assert_eq!(app.recent_files.maximum(),10);assert!(app.recent_files.files().is_empty());
        for file in ["settings.json","copy.png","saved.graphite"] {std::fs::remove_file(root.join(file)).unwrap();}std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn opening_supported_files_uses_new_tabs_and_keeps_failed_opens_out() {
        let mut app=GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Existing drawing",12.,16.,120.));
        app.tabs[app.active_tab].modified=true;
        let original=app.document.surface.graphite_mass.clone();
        let initial_tab=app.active_tab;
        let fixtures=Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/open");
        let mut names=vec!["rgba.PNG".to_string(),"palette.png".into(),"gray16.png".into(),"color.bmp".into(),"color.jpeg".into(),"color.jpg".into(),"rotated.jpg".into(),"group-mask.psd".into()];
        for depth in [8,16,32] {for compression in 0..4 {names.push(format!("layers-{depth}-{compression}.psd"));}}
        names.push("shape-layers.psd".into());
        for name in names {
            let before=app.tabs.len();app.open_project_path(&fixtures.join(&name)).unwrap_or_else(|e|panic!("{name}: {e}"));
            assert_eq!(app.tabs.len(),before+1);
            assert!(app.tabs[app.active_tab].modified);
            assert!(app.tabs[app.active_tab].project_path.is_none(),"imported originals must use Save As");
            assert!(app.fit_requested);
        }
        let tab=app.active_tab;let count=app.tabs.len();
        assert!(app.open_project_path(&fixtures.join("unsupported-vector-style.psd")).is_err());
        assert_eq!((app.active_tab,app.tabs.len()),(tab,count));
        let ctx=egui::Context::default();let mut preview=crate::ui::test_render::Preview::default();
        for frame in 0..4 {
            let output=ctx.run_ui(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1440.,920.))),..Default::default()},|ui|app.interface(ui));
            preview.update(&output);
            if frame==3 {preview.save(&ctx,&output,"../../ui-v248-open-vectors.png",[1440,920]);}
        }
        app.document.activate_layer(1);
        app.select_tool(ToolKind::VectorSelect);app.editing.selected_path=Some(0);
        let before=app.document.layers[1].vectors.strokes[0].shape.clone();
        app.begin_transform(&ctx);assert!(app.editing.transform.is_some());
        app.set_transform_scale_percent(75.);app.apply_transform();
        assert_ne!(app.document.layers[1].vectors.strokes[0].shape,before);
        assert!(app.history.undo(&mut app.document));
        assert_eq!(app.document.layers[1].vectors.strokes[0].shape,before);
        app.activate_tab(initial_tab);
        assert_eq!(app.document.surface.graphite_mass,original);assert!(app.tabs[initial_tab].modified);
    }
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
