use super::*;
use graphite_studio::shortcuts::{Binding,Command,Shortcuts,SizeModifier};

pub(super) struct ShortcutEditor {
    pub bindings:Shortcuts,
    pub open:bool,
    draft:Shortcuts,
    recording:Option<Command>,
    message:String,
}
impl Default for ShortcutEditor {
    fn default()->Self {
        let mut out=Self{bindings:Shortcuts::default(),open:false,draft:Shortcuts::default(),recording:None,message:String::new()};
        #[cfg(not(test))]
        if let Some(path)=Shortcuts::path().filter(|p|p.exists()) {match Shortcuts::load(&path) {Ok(s)=>out.bindings=s,Err(e)=>out.message=format!("Could not load shortcuts: {e}. Defaults are active.")}}
        let _=&mut out;
        out
    }
}
impl GraphiteApp {
    pub(super) fn open_shortcuts(&mut self) {
        self.finish_stroke();
        self.shortcuts.draft=self.shortcuts.bindings.clone();
        self.shortcuts.recording=None;
        self.shortcuts.open=true;
        self.quick_controls.close_size();
    }
    pub(super) fn show_shortcuts(&mut self,ctx:&egui::Context) {
        if !self.shortcuts.open {return;}
        if let Some(command)=self.shortcuts.recording {
            let captured=ctx.input_mut(|i| {
                let b=i.events.iter().find_map(|e|if let egui::Event::Key{key,modifiers,pressed:true,repeat:false,..}=e {Some(Binding::capture(*key,*modifiers))}else{None});
                i.events.retain(|e|!matches!(e,egui::Event::Key{..}|egui::Event::Text(_)));
                b
            });
            if let Some(b)=captured {match self.shortcuts.draft.set(command,Some(b)) {
                Ok(())=>{self.shortcuts.recording=None;self.shortcuts.message.clear();},
                Err(e)=>self.shortcuts.message=e,
            }}
        }
        let mut apply=false;let mut cancel=false;
        let id=egui::Id::new("shortcut_editor");
        egui::Modal::new(id).area(egui::Modal::default_area(id).fade_in(false)).show(ctx,|ui| {
            ui.set_width(610.);
            ui.heading("Keyboard shortcuts");
            ui.label("Click a shortcut, then press the key combination you want.");
            ui.small("Changes take effect when you save. Shift snapping and mouse/touch gestures remain unchanged.");
            if let Some(c)=self.shortcuts.recording {
                ui.horizontal(|ui| {ui.strong(format!("Press keys for {}…",c.label()));if ui.button("Stop recording").clicked(){self.shortcuts.recording=None;}});
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false,false]).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .max_height((ctx.content_rect().height()-280.).clamp(140.,520.)).show(ui,|ui| {
                egui::Grid::new("shortcut_rows").striped(true).min_row_height(38.).show(ui,|ui| {
                    for c in Command::ALL {
                        ui.add_sized([250.,36.],egui::Label::new(c.label()));
                        let label=if self.shortcuts.recording==Some(c) {"Press keys…".into()}else{self.shortcuts.draft.label(c)};
                        let r=ui.add_sized([230.,36.],egui::Button::new(label));
                        #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new(("shortcut_assign",c.label())),r.rect));
                        if r.clicked() {self.shortcuts.recording=Some(c);self.shortcuts.message.clear();}
                        if ui.add_sized([60.,36.],egui::Button::new("Clear")).clicked() {
                            let _=self.shortcuts.draft.set(c,None);self.shortcuts.recording=None;self.shortcuts.message.clear();
                        }
                        ui.end_row();
                    }
                });
            });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Press to open size slider");
                    egui::ComboBox::from_id_salt("size_modifier").selected_text(format!("{:?}",self.shortcuts.draft.size_modifier)).show_ui(ui,|ui| {
                        for m in [SizeModifier::Shift,SizeModifier::Ctrl,SizeModifier::Alt,SizeModifier::Disabled] {ui.selectable_value(&mut self.shortcuts.draft.size_modifier,m,format!("{m:?}"));}
                    });
                });
            if !self.shortcuts.message.is_empty(){ui.colored_label(Color32::from_rgb(170,60,35),&self.shortcuts.message);}
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_sized([140.,40.],egui::Button::new("Restore defaults")).clicked(){self.shortcuts.draft=Shortcuts::default();self.shortcuts.recording=None;self.shortcuts.message.clear();}
                let save=ui.add_enabled(self.shortcuts.recording.is_none(),egui::Button::new("Save shortcuts").min_size(Vec2::new(140.,40.)));
                #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("save_shortcuts"),save.rect));
                apply=save.clicked();
                cancel=ui.add_sized([100.,40.],egui::Button::new("Cancel")).clicked();
            });
        });
        if cancel {self.shortcuts.open=false;self.shortcuts.recording=None;}
        if apply {
            #[cfg(not(test))]
            let saved=Shortcuts::path().ok_or("Cannot find the shortcut settings folder".to_owned()).and_then(|p|self.shortcuts.draft.save(&p));
            #[cfg(test)] let saved:Result<(),String>=Ok(());
            match saved {
                Ok(())=>{self.shortcuts.bindings=self.shortcuts.draft.clone();self.shortcuts.open=false;self.status="Keyboard shortcuts saved.".into();},
                Err(e)=>self.shortcuts.message=format!("Could not save shortcuts: {e}"),
            }
        }
    }
}

#[cfg(test)] mod tests {
    use super::*;
    fn frame(a:&mut GraphiteApp,c:&egui::Context,events:Vec<egui::Event>)->egui::FullOutput {
        c.run_ui(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1440.,1000.))),events,..Default::default()},|ui|a.interface(ui))
    }
    fn click(a:&mut GraphiteApp,c:&egui::Context,p:Pos2) {for pressed in [true,false] {frame(a,c,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton{pos:p,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]);}}
    fn key(a:&mut GraphiteApp,c:&egui::Context,k:egui::Key) {frame(a,c,vec![egui::Event::Key{key:k,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::NONE}]);}
    #[test] fn editor_records_rejects_conflicts_saves_and_dispatches_new_binding() {
        let mut a=GraphiteApp::with_render_state(None);let c=egui::Context::default();
        a.handle_topbar_action(TopbarAction::Shortcuts);
        for _ in 0..3 {frame(&mut a,&c,vec![]);}
        let r=c.data(|d|d.get_temp::<Rect>(egui::Id::new(("shortcut_assign",Command::Pencil.label()))).unwrap());
        click(&mut a,&c,r.center());assert_eq!(a.shortcuts.recording,Some(Command::Pencil));
        key(&mut a,&c,egui::Key::E);assert!(a.shortcuts.message.contains("already assigned"));
        assert_eq!(a.settings.tool,ToolKind::Pencil,"Recording must not activate Eraser");
        key(&mut a,&c,egui::Key::F2);assert!(a.shortcuts.recording.is_none());
        assert_eq!(a.shortcuts.bindings.label(Command::Pencil),"P");
        for _ in 0..2 {frame(&mut a,&c,vec![]);}
        let r=c.data(|d|d.get_temp::<Rect>(egui::Id::new("save_shortcuts")).unwrap());
        click(&mut a,&c,r.center());assert!(!a.shortcuts.open);
        assert_eq!(a.shortcuts.bindings.label(Command::Pencil),"F2");
        key(&mut a,&c,egui::Key::E);assert_eq!(a.settings.tool,ToolKind::Eraser);
        key(&mut a,&c,egui::Key::P);assert_eq!(a.settings.tool,ToolKind::Eraser);
        key(&mut a,&c,egui::Key::F2);assert_eq!(a.settings.tool,ToolKind::Pencil);
        assert!(!a.history.can_undo());
        let fresh=egui::Context::default();let mut preview=crate::ui::test_render::Preview::default();
        let mut view=GraphiteApp::with_render_state(None);view.open_shortcuts();
        for _ in 0..3 {let out=frame(&mut view,&fresh,vec![]);preview.update(&out);preview.save(&fresh,&out,"../../ui-v245-shortcuts.png",[1440,1000]);}
    }
}
