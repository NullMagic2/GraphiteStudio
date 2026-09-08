use super::*;
use graphite_studio::pencil_gallery::{Gallery, PencilPreset};
use std::collections::HashMap;
use graphite_studio::pencil_settings::{PencilControls,PencilSettings};

#[derive(Default)]
pub(super) struct GalleryUi {
    pub open: bool,
    pub data: Gallery,
    search: String,
    folder: String,
    name: String,
    edit_folder: String,
    selected: Option<usize>,
    previews: HashMap<usize, TextureHandle>,
    pub message: String,
    preferences:PencilSettings,
    preferences_changed:Option<std::time::Instant>,
    preferences_blocked:bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pencil_slider_edits_remain_independent_after_rename_and_restart() {
        let mut a=GraphiteApp::with_render_state(None);
        a.replace_document(CanvasSpec::from_physical("Preferences",40.,50.,120.));
        for _ in 0..2 {a.gallery.data.add("Same name","Collection",&standard()).unwrap();}
        a.activate_gallery_pencil(a.gallery_preset(1));
        let first=a.settings.pencil_id.clone();
        let second=a.gallery.data.pencils[1].id.clone();
        assert_ne!(first,second);
        let ctx=egui::Context::default();
        let frame=|a:&mut GraphiteApp,events|ctx.run_ui(egui::RawInput{
            screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1440.,920.))),events,..Default::default()
        },|ui|a.interface(ui));
        for _ in 0..3{frame(&mut a,vec![]);}
        for (key,x) in [("pencil_width_response",25.),("pencil_line_smoothing",72.)] {
            let rect=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(key)).unwrap());
            let pos=Pos2::new(rect.left()+x,rect.center().y);
            for pressed in [true,false] {frame(&mut a,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        }
        assert_ne!(a.settings.pressure_width,0.6);
        assert!(a.settings.line_smoothing>0.);
        let saved=PencilControls::capture(&a.settings,a.tip_state.profile.orientation_deg);
        assert_eq!(a.gallery.preferences.pencils[&first],saved,"Real slider events must queue automatic persistence");
        a.activate_gallery_pencil(a.gallery_preset(2));
        assert_eq!(a.settings.pressure_width,0.6);assert_eq!(a.settings.line_smoothing,0.);
        a.settings.pressure_width=0.9;a.settings.line_smoothing=0.15;
        a.remember_pencil_controls();
        a.activate_gallery_pencil(a.gallery_preset(0));
        a.settings.pressure_width=0.73;a.settings.line_smoothing=0.37;a.remember_pencil_controls();
        a.gallery.data.pencils[0].name="Renamed".into();a.gallery.data.pencils[0].folder="Moved".into();
        a.activate_gallery_pencil(a.gallery_preset(1));
        assert_eq!(PencilControls::capture(&a.settings,a.tip_state.profile.orientation_deg),saved);
        let root=std::env::temp_dir().join(format!("graphite-pencil-settings-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&root).unwrap();
        a.gallery.data.save(&root.join("Pencils.gallery")).unwrap();
        a.gallery.preferences.save(&root.join("pencil.settings")).unwrap();
        let text=std::fs::read_to_string(root.join("pencil.settings")).unwrap();
        assert!(text.contains("width_response") && text.contains("line_smoothing") && !text.contains("mask"));
        a.gallery=GalleryUi::default();
        a.gallery.data=Gallery::load(&root.join("Pencils.gallery")).unwrap();
        a.gallery.preferences=PencilSettings::load(&root.join("pencil.settings")).unwrap();
        a.restore_pencil_preferences();
        assert_eq!(a.settings.pencil_id,first);
        assert_eq!(PencilControls::capture(&a.settings,a.tip_state.profile.orientation_deg),saved);
        a.activate_gallery_pencil(a.gallery_preset(2));assert_eq!((a.settings.pressure_width,a.settings.line_smoothing),(0.9,0.15));
        a.activate_gallery_pencil(a.gallery_preset(0));assert_eq!((a.settings.pressure_width,a.settings.line_smoothing),(0.73,0.37));
        for name in ["Pencils.gallery","pencil.settings"]{std::fs::remove_file(root.join(name)).unwrap();}std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn imported_collection_clears_filters_and_failed_import_is_atomic() {
        let mut a = GraphiteApp::with_render_state(None);
        a.gallery.search = "Unrelated search".into();
        a.gallery.folder = "Another folder".into();
        let tip = std::sync::Arc::new(crate::core::brush::BrushTip {
            name:"Imported texture".into(), width:2,height:2,mask:vec![0,80,160,255],
        });
        a.gallery_import(&[tip.clone()],"New collection").unwrap();
        assert!(a.gallery.open && a.gallery.search.is_empty());
        assert_eq!(a.gallery.folder,"New collection");
        assert_eq!(a.gallery.data.pencils[0].folder,"New collection");
        for i in 1..255 {a.gallery.data.add(&format!("Pencil {i}"),"Existing",&standard()).unwrap();}
        assert!(a.gallery_import(&[tip.clone(),tip],"Too many").is_err());
        assert_eq!(a.gallery.data.pencils.len(),255);
        assert_eq!(a.gallery.folder,"New collection");
        assert!(!a.gallery.data.pencils.iter().any(|p|p.folder=="Too many"));
    }

    #[test]
    fn toolbar_icons_fit_windows_and_open_their_controls() {
        let mut a=GraphiteApp::with_render_state(None);
        a.replace_document(CanvasSpec::from_physical("Toolbar",40.,50.,120.));
        let ctx=egui::Context::default();
        let mut preview=crate::ui::test_render::Preview::default();
        let frame=|a:&mut GraphiteApp,width:f32,events|ctx.run_ui(egui::RawInput{
            screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(width,920.))),events,..Default::default()
        },|ui|a.interface(ui));
        for width in [900.,1024.,1280.,1440.] {
            for _ in 0..4 {let output=frame(&mut a,width,vec![]);preview.update(&output);}
            for label in ["Paper settings…","Pencil gallery…","Fit to screen","Fullscreen","Portrait","Landscape"] {
                let rect=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("test_action_icon",label))).unwrap());
                assert!(rect.left()>=0. && rect.right()<=width,"{label} outside {width}: {rect:?}");
                assert!(rect.height()>=44.);
            }
            let output=frame(&mut a,width,vec![]);preview.update(&output);
            if width==1440. || width==900. {preview.save(&ctx,&output,&format!("../../ui-v231-toolbar-{width}.png"),[width as u32,920]);}
        }
        for label in ["Paper settings…","Pencil gallery…","Fit to screen","Fullscreen"] {
            a.paper_settings.open=false;a.gallery.open=false;
            for _ in 0..3 {frame(&mut a,1440.,vec![]);}
            let pos=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("test_action_icon",label))).unwrap().center());
            for pressed in [true,false] {frame(&mut a,1440.,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
            match label {
                "Paper settings…"=>assert!(a.paper_settings.open),
                "Pencil gallery…"=>assert!(a.gallery.open),
                "Fullscreen"=>assert!(a.fullscreen),
                _=>assert!(a.viewport.zoom>0.),
            }
        }
    }
    #[test]
    fn gallery_tiles_select_embedded_texture_with_large_touch_targets() {
        let mut a = GraphiteApp::with_render_state(None);
        a.replace_document(CanvasSpec::from_physical("Gallery test", 40., 50., 120.));
        let mut settings = standard();
        settings.pencil_texture = Some(std::sync::Arc::new(crate::core::brush::BrushTip {
            name: "My textured pencil".into(),
            width: 8,
            height: 8,
            mask: (0..64).map(|i| if i % 3 == 0 { 0 } else { 255 }).collect(),
        }));
        a.gallery
            .data
            .add("My textured pencil", "Drawing", &settings)
            .unwrap();
        settings.grade = crate::core::pencil::PencilGrade::B4;
        a.gallery
            .data
            .add("Soft graphite", "Drawing", &settings)
            .unwrap();
        a.open_pencil_gallery();
        let ctx = egui::Context::default();
        let mut preview = crate::ui::test_render::Preview::default();
        let frame = |a: &mut GraphiteApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    events,
                    ..Default::default()
                },
                |ui| a.interface(ui),
            )
        };
        for pass in 0..5 {
            let output = frame(&mut a, vec![]);
            preview.update(&output);
            if pass == 4 {
                preview.save(&ctx, &output, "../../ui-v230-gallery.png", [1440, 920]);
            }
        }
        let rect = ctx.data(|d| {
            d.get_temp::<Rect>(egui::Id::new(("gallery_tile", 1usize)))
                .unwrap()
        });
        assert!(rect.width() >= 180. && rect.height() >= 100.);
        for pressed in [true, false] {
            frame(
                &mut a,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(a.settings.tool, ToolKind::Pencil);
        assert!(a.settings.pencil_texture.is_some());
        assert!(
            a.document.surface.graphite_mass.iter().all(|&v| v == 0.),
            "gallery click leaked onto paper"
        );
        a.gallery.search = "Soft".into();
        frame(&mut a, vec![]);
        assert_eq!(a.gallery.data.pencils.len(), 2);
        a.gallery.folder="Drawing".into();frame(&mut a,vec![]);
        let pos=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("gallery_delete_folder")).unwrap().center());
        for pressed in [true,false] {
            frame(&mut a,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);
        }
        assert!(a.gallery.data.pencils.is_empty());assert!(a.gallery.folder.is_empty());
        assert!(a.settings.pencil_texture.is_some(),"Removing a gallery collection must not discard the active pencil");
    }
}
fn path() -> Option<std::path::PathBuf> {
    Some(
        std::env::current_exe()
            .ok()?
            .parent()?
            .join("Pencils.gallery"),
    )
}
impl GalleryUi {
    pub fn load(&mut self) {
        if let Some(path) = path().filter(|p| p.exists()) {
            match Gallery::load(&path) {
                Ok(mut data) => {
                    let migrated=data.ensure_ids();
                    self.data=data;
                    if migrated {
                        if let Err(e)=self.data.save(&path) {self.message=e;self.preferences_blocked=true;}
                    }
                },
                Err(e) => self.message = e,
            }
        }
        if let Some(file)=path().map(|p|p.with_file_name("pencil.settings")).filter(|p|p.exists()) {
            match PencilSettings::load(&file) {
                Ok(data)=>self.preferences=data,
                Err(e)=>{self.message=format!("Cannot load pencil.settings: {e}");self.preferences_blocked=true;}
            }
        }
        if !self.preferences_blocked {
            let before=self.preferences.pencils.len();
            self.preferences.pencils.entry("standard".into()).or_insert_with(||PencilControls::capture(&standard(),0.));
            for pencil in &self.data.pencils {
                self.preferences.pencils.entry(pencil.id.clone()).or_insert_with(||PencilControls::capture(&pencil.settings,0.));
            }
            if self.preferences.pencils.len()!=before {self.preferences_changed=Some(std::time::Instant::now());}
        }
    }
    pub fn persist(&mut self) {
        if !self.preferences_blocked {
            self.preferences.pencils.retain(|id,_|id=="standard" || self.data.pencils.iter().any(|p|&p.id==id));
            for pencil in &self.data.pencils {
                self.preferences.pencils.entry(pencil.id.clone()).or_insert_with(||PencilControls::capture(&pencil.settings,0.));
            }
            self.preferences_changed=Some(std::time::Instant::now());
        }
        #[cfg(not(test))]
        {
            self.message = path()
                .ok_or("No gallery location.".to_owned())
                .and_then(|p| self.data.save(&p))
                .map(|_| "Gallery saved.".to_owned())
                .unwrap_or_else(|e| format!("Gallery not saved: {e}"));
        }
    }
    pub fn flush_preferences(&mut self) {
        if self.preferences_blocked || self.preferences_changed.is_none(){return;}
        #[cfg(not(test))]
        {
            match path().ok_or("No settings location.".to_owned()).and_then(|p|self.preferences.save(&p.with_file_name("pencil.settings"))) {
                Ok(())=>{self.preferences_changed=None;},
                Err(e)=>{self.message=format!("Pencil settings not saved: {e}");self.preferences_changed=Some(std::time::Instant::now());}
            }
        }
    }
}

fn standard() -> ToolSettings {
    ToolSettings {
        pressure_width: 0.6,
        tip_sharpness: 0.65,
        ..Default::default()
    }
}
fn preview(settings: &ToolSettings) -> egui::ColorImage {
    let spec = CanvasSpec::from_physical("Preview", 50.8, 16.9, 120.);
    let n = spec.pixel_count();
    let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; n]);
    let mut settings = settings.clone();
    settings.tool = ToolKind::Pencil;
    settings.pencil_geometry_scale = 1.;
    let mut tip = PencilTipState::fresh(settings.pencil_core_diameter_mm);
    let mut engine = StrokeEngine::default();
    let mut tx = EditTransaction::default();
    let p = |i: usize| StrokePoint {
        x: 30. + i as f32 * 7.,
        y: 42. + (i as f32 * 0.2).sin() * 8.,
        pressure: 0.35 + 0.45 * (i as f32 / 25. * std::f32::consts::PI).sin(),
        tilt_deg: 65.,
        azimuth_deg: 90.,
        rotation_deg: None,
    };
    for i in 1..=25 {
        engine.apply_segment(&mut doc, &settings, &mut tip, p(i - 1), p(i), &mut tx);
    }
    RasterRenderer::default().render_full(&doc)
}
impl GraphiteApp {
    fn gallery_preset(&self,key:usize)->PencilPreset {
        let mut preset=if key==0 {PencilPreset{id:"standard".into(),name:"Standard graphite".into(),folder:String::new(),settings:standard()}}
            else {self.gallery.data.pencils[key-1].clone()};
        if let Some(saved)=self.gallery.preferences.pencils.get(&preset.id) {saved.apply(&mut preset.settings);}
        preset.settings.pencil_tip_shape=true;
        preset
    }
    pub(super) fn remember_pencil_controls(&mut self) {
        if self.settings.tool!=ToolKind::Pencil || self.gallery.preferences_blocked{return;}
        let id=if self.settings.pencil_id.is_empty() && self.settings.pencil_texture.is_none(){"standard".to_owned()}else{self.settings.pencil_id.clone()};
        if id!="standard" && !self.gallery.data.pencils.iter().any(|p|p.id==id){return;}
        let controls=PencilControls::capture(&self.settings,self.tip_state.profile.orientation_deg);
        self.gallery.preferences.pencils.insert(id.clone(),controls);
        self.gallery.preferences.last_pencil=Some(id.clone());
        self.gallery.preferences_changed=Some(std::time::Instant::now());
        let key=if id=="standard"{Some(0)}else{self.gallery.data.pencils.iter().position(|p|p.id==id).map(|i|i+1)};
        if let Some(key)=key {self.gallery.previews.remove(&key);}
    }
    fn activate_gallery_pencil(&mut self,preset:PencilPreset) {
        self.select_tool(ToolKind::Pencil);
        preset.apply(&mut self.settings);
        let rotation=if let Some(saved)=self.gallery.preferences.pencils.get(&preset.id) {
            saved.apply(&mut self.settings);saved.tip_rotation
        }else{0.};
        self.tip_state.sharpen(self.settings.pencil_core_diameter_mm,self.settings.grade.formulation());
        self.tip_state.profile.set_orientation(rotation);
        self.remember_pencil_controls();
    }
    pub(super) fn restore_pencil_preferences(&mut self) {
        let key=self.gallery.preferences.last_pencil.as_ref().and_then(|id|self.gallery.data.pencils.iter().position(|p|&p.id==id).map(|i|i+1)).unwrap_or(0);
        self.activate_gallery_pencil(self.gallery_preset(key));
    }
    pub(super) fn tick_pencil_preferences(&mut self,ctx:&egui::Context) {
        if let Some(changed)=self.gallery.preferences_changed {
            if changed.elapsed()>=std::time::Duration::from_millis(600) && !self.canvas_contact_active() {
                self.gallery.flush_preferences();
            }
            if self.gallery.preferences_changed.is_some(){ctx.request_repaint_after(std::time::Duration::from_millis(600));}
        }
    }
    pub(super) fn open_pencil_gallery(&mut self) {
        self.finish_stroke();
        self.paper_settings.open = false;
        self.gallery.open = true;
    }
    pub(super) fn gallery_import(
        &mut self,
        tips: &[std::sync::Arc<crate::core::brush::BrushTip>],
        folder: &str,
    ) -> Result<(), String> {
        let first=self.gallery.data.pencils.len()+1;
        let mut next = self.gallery.data.clone();
        for tip in tips {
            let mut settings = self.settings.clone();
            settings.pencil_texture = Some(tip.clone());
            next.add(&tip.name, folder, &settings)?;
        }
        self.gallery.data = next;
        self.gallery.folder = folder.trim().chars().take(100).collect();
        self.gallery.edit_folder = self.gallery.folder.clone();
        self.gallery.search.clear();
        self.gallery.selected = None;
        self.gallery.previews.clear();
        self.gallery.persist();
        self.open_pencil_gallery();
        if !tips.is_empty(){self.activate_gallery_pencil(self.gallery_preset(first));}
        Ok(())
    }
    pub(super) fn show_pencil_gallery(&mut self, ctx: &egui::Context) {
        if !self.gallery.open {
            return;
        }
        let mut open = true;
        let mut use_pencil: Option<PencilPreset> = None;
        let mut import = false;
        let mut add_folder = false;
        let mut delete_folder = false;
        let mut done = false;
        egui::Window::new("Pencil gallery")
            .fade_in(false)
            .fade_out(false)
            .open(&mut open)
            .default_pos(Pos2::new(100., 80.))
            .default_size(Vec2::new(820., 620.))
            .min_width(340.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.spacing_mut().interact_size.y = 44.;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Search");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.gallery.search).desired_width(220.),
                    );
                    import = ui
                        .add_sized([140., 44.], egui::Button::new("Import ABR…"))
                        .clicked();
                    done = ui
                        .add_sized([80., 44.], egui::Button::new("Done"))
                        .clicked();
                });
                ui.horizontal_wrapped(|ui| {
                    add_folder=ui.add_sized([130.,44.],egui::Button::new("Add folder…")).on_hover_text("Choose a folder and import its ABR files as a collection.").clicked();
                    let delete=ui.add_enabled(!self.gallery.folder.is_empty(),egui::Button::new("Delete folder"));
                    #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("gallery_delete_folder"),delete.rect));
                    delete_folder=delete.on_hover_text("Remove the selected collection and its gallery pencils. Files on disk and existing strokes are kept.").clicked();
                });
                let mut folders: Vec<_> = self
                    .gallery
                    .data
                    .pencils
                    .iter()
                    .map(|p| p.folder.clone())
                    .collect();
                folders.extend(self.gallery.data.collections.iter().map(|c|c.name.clone()));
                folders.sort();
                folders.dedup();
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(self.gallery.folder.is_empty(), "All pencils")
                        .clicked()
                    {
                        self.gallery.folder.clear();
                    }
                    for folder in folders.iter().filter(|f| !f.is_empty()) {
                        if ui
                            .selectable_label(self.gallery.folder == *folder, folder)
                            .clicked()
                        {
                            self.gallery.folder = folder.clone();
                            self.gallery.edit_folder = folder.clone();
                        }
                    }
                });
                ui.separator();
                let query = self.gallery.search.to_lowercase();
                let visible: Vec<usize> = (0..=self.gallery.data.pencils.len())
                    .filter(|&key| {
                        if key == 0 {
                            return self.gallery.folder.is_empty()
                                && "standard graphite".contains(&query);
                        }
                        let p = &self.gallery.data.pencils[key - 1];
                        (self.gallery.folder.is_empty() || p.folder == self.gallery.folder)
                            && p.name.to_lowercase().contains(&query)
                    })
                    .collect();
                let mut generated = 0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 170.).max(130.))
                    .show(ui, |ui| {
                        // Size the grid from the viewport after scrollbar space is
                        // reserved, and share all remaining width between tiles.
                        let gap = ui.spacing().item_spacing.x;
                        let width = ui.available_width();
                        let columns = ((width + gap) / (200. + gap)).floor().max(1.) as usize;
                        let tile_width = (width - gap * (columns - 1) as f32) / columns as f32;
                        let preview_height = (tile_width - 12.) / 3.;
                        let tile_height = (preview_height + 48.).max(112.);
                        for row in visible.chunks(columns) {
                            ui.horizontal(|ui| {
                                for &key in row {
                                    let preset = self.gallery_preset(key);
                                    let (rect, response) = ui
                                        .allocate_exact_size(Vec2::new(tile_width, tile_height), Sense::click());
                                    #[cfg(test)]
                                    ui.data_mut(|d| {
                                        d.insert_temp(egui::Id::new(("gallery_tile", key)), rect)
                                    });
                                    if ui.is_rect_visible(rect)
                                        && !self.gallery.previews.contains_key(&key)
                                        && generated < 2
                                    {
                                        self.gallery.previews.insert(
                                            key,
                                            ctx.load_texture(
                                                format!("pencil_preview_{key}"),
                                                preview(&preset.settings),
                                                TextureOptions::LINEAR,
                                            ),
                                        );
                                        generated += 1;
                                    }
                                    let selected = self.gallery.selected == Some(key);
                                    let visual =
                                        ui.style().interact_selectable(&response, selected);
                                    ui.painter().rect(
                                        rect.shrink(1.),
                                        6.,
                                        visual.bg_fill,
                                        Stroke::new(
                                            if selected { 2. } else { 1. },
                                            if selected {
                                                Color32::from_rgb(45, 120, 200)
                                            } else {
                                                visual.bg_stroke.color
                                            },
                                        ),
                                        egui::StrokeKind::Inside,
                                    );
                                    if let Some(texture) = self.gallery.previews.get(&key) {
                                        ui.painter().image(
                                            texture.id(),
                                            Rect::from_min_size(
                                                rect.min + Vec2::splat(6.),
                                                Vec2::new(tile_width - 12., preview_height),
                                            ),
                                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                            Color32::WHITE,
                                        );
                                    } else if ui.is_rect_visible(rect) {
                                        ctx.request_repaint();
                                    }
                                    let text = ui.painter().layout(
                                        preset.name.clone(),
                                        egui::FontId::proportional(14.),
                                        visual.text_color(),
                                        tile_width - 16.,
                                    );
                                    ui.painter().with_clip_rect(rect.shrink(6.)).galley(
                                        rect.min + Vec2::new(8., preview_height + 12.),
                                        text,
                                        visual.text_color(),
                                    );
                                    response.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::WidgetType::Button,
                                            true,
                                            selected,
                                            &preset.name,
                                        )
                                    });
                                    if response.clicked() {
                                        self.gallery.selected = Some(key);
                                        self.gallery.name = preset.name.clone();
                                        self.gallery.edit_folder = preset.folder.clone();
                                        use_pencil = Some(preset);
                                    }
                                }
                            });
                        }
                        if visible.is_empty() {
                            ui.label("No pencils match this search.");
                        }
                    });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut self.gallery.name).desired_width(180.));
                    ui.label("Folder");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.gallery.edit_folder)
                            .hint_text("My pencils")
                            .desired_width(150.),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save current pencil").clicked() {
                        match self.gallery.data.add(
                            &self.gallery.name,
                            &self.gallery.edit_folder,
                            &self.settings,
                        ) {
                            Ok(()) => {
                                self.gallery.selected = Some(self.gallery.data.pencils.len());
                                let key=self.gallery.data.pencils.len();
                                let id=self.gallery.data.pencils[key-1].id.clone();
                                self.gallery.preferences.pencils.insert(id,PencilControls::capture(&self.settings,self.tip_state.profile.orientation_deg));
                                self.gallery.persist();
                                use_pencil=Some(self.gallery_preset(key));
                            }
                            Err(e) => self.gallery.message = e,
                        }
                    }
                    if let Some(key) = self
                        .gallery
                        .selected
                        .filter(|k| *k > 0 && *k <= self.gallery.data.pencils.len())
                    {
                        if ui.button("Rename / move").clicked()
                            && !self.gallery.name.trim().is_empty()
                        {
                            let p = &mut self.gallery.data.pencils[key - 1];
                            p.name = self.gallery.name.trim().chars().take(100).collect();
                            p.folder = self.gallery.edit_folder.trim().chars().take(100).collect();
                            self.gallery.persist();
                        }
                        if ui.button("Remove from gallery").clicked() {
                            self.gallery.data.pencils.remove(key - 1);
                            self.gallery.previews.clear();
                            self.gallery.selected = None;
                            self.gallery.persist();
                        }
                    }
                });
                ui.small(&self.gallery.message);
            });
        self.gallery.open = open && !done;
        if delete_folder {
            self.gallery.data.delete_folder(&self.gallery.folder);
            self.gallery.folder.clear();self.gallery.edit_folder.clear();self.gallery.selected=None;
            self.gallery.previews.clear();self.gallery.persist();
            use_pencil=None;
        }
        if add_folder {
            if let Some(directory)=self.file_dialog().set_title("Add pencil folder collection").pick_folder() {
                match self.gallery.data.add_folder(&directory,&self.settings) {
                    Ok(name)=>{self.gallery.folder=name.clone();self.gallery.edit_folder=name;self.gallery.search.clear();self.gallery.selected=None;self.gallery.previews.clear();self.gallery.persist();},
                    Err(e)=>self.gallery.message=format!("Folder not added: {e}")
                }
            }
        }
        if let Some(preset) = use_pencil {
            self.activate_gallery_pencil(preset);
        }
        if import {
            self.import_brushes();
        }
    }
}
