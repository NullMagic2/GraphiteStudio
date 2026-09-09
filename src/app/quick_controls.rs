use super::*;

#[derive(Default)]
pub(super) struct QuickControls {
    ctrl: bool,
    latched: bool,
    popup_rect: Option<Rect>,
    focus_slider: bool,
    chord: bool,
    button_open: bool,
    collapsed: bool,
    anchor: Option<Pos2>,
    rects: Vec<Rect>,
    pointer_down: bool,
    dismissed_press: bool,
    toolbar_position: Option<Pos2>,
    toolbar_vertical: Option<bool>,
    toolbar_size: Vec2,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_tablet_draws_on_first_contact_after_size_popup_even_after_missing_release() {
        let mut app=GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Tablet resume",40.,50.,120.));
        let ctx=egui::Context::default();
        for _ in 0..3 { frame(&mut app,&ctx,Default::default(),vec![]); }
        let canvas=app.workspace_canvas.unwrap();
        let start=app.viewport.document_to_screen(canvas,Vec2::new(60.,60.));
        app.quick_controls.latched=true;
        app.quick_controls.popup_rect=Some(Rect::from_min_size(Pos2::new(5.,5.),Vec2::new(100.,40.)));
        app.quick_controls.pointer_down=true;
        for step in 0..3 {
            let _=ctx.run_ui(egui::RawInput {screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1440.,920.))),..Default::default()},|ui| {
                let input=PointerFrame {position:Some(start+Vec2::new(step as f32*8.,0.)),
                    primary_pressed:step==0,primary_down:true,primary_released:step==2,
                    pressure:0.7,pressure_from_device:true,over_canvas:true,..Default::default()};
                assert!(!app.quick_controls.block_input(input));
                app.handle_drawing_input(ui,canvas,input);
            });
        }
        assert!(!app.quick_controls.is_open());
        assert!(app.document.surface.graphite_mass.iter().any(|&v|v>0.));
        assert!(app.history.can_undo());
    }
    #[test]
    fn fullscreen_toolbar_drag_keeps_controls_usable_without_drawing() {
        let mut a=GraphiteApp::with_render_state(None);a.fullscreen=true;
        a.replace_document(CanvasSpec::from_physical("Toolbar drag",40.,50.,120.));
        let ctx=egui::Context::default();for _ in 0..3{frame(&mut a,&ctx,Default::default(),vec![]);}
        let before=a.renderer.rgba8(&a.document);
        let original=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_toolbar")).unwrap());
        let grip=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_toolbar_grip")).unwrap().center());
        let button=|p,pressed|egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()};
        frame(&mut a,&ctx,Default::default(),vec![egui::Event::PointerMoved(grip),button(grip,true)]);
        for i in 1..=8 {frame(&mut a,&ctx,Default::default(),vec![egui::Event::PointerMoved(grip+Vec2::new(25.,10.)*i as f32)]);}
        let end=grip+Vec2::new(200.,80.);frame(&mut a,&ctx,Default::default(),vec![button(end,false)]);
        for _ in 0..3 {frame(&mut a,&ctx,Default::default(),vec![]);}
        let moved=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_toolbar")).unwrap());
        assert!((moved.min-original.min).length()>120.);
        let eraser=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("quick_tool",ToolKind::Eraser.label()))).unwrap());
        click(&mut a,&ctx,eraser.center(),Default::default());assert_eq!(a.settings.tool,ToolKind::Eraser);
        a.fullscreen=false;frame(&mut a,&ctx,Default::default(),vec![]);a.fullscreen=true;
        for _ in 0..3{frame(&mut a,&ctx,Default::default(),vec![]);}
        let reopened=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_toolbar")).unwrap());assert!((reopened.min-moved.min).length()<1.);
        assert!(a.renderer.rgba8(&a.document)==before);assert!(!a.history.can_undo());
    }
    fn frame(app:&mut GraphiteApp,ctx:&egui::Context,modifiers:egui::Modifiers,events:Vec<egui::Event>)->egui::FullOutput {
        ctx.run_ui(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1440.,920.))),modifiers,events,..Default::default()},|ui|app.interface(ui))
    }
    fn click(app:&mut GraphiteApp,ctx:&egui::Context,pos:Pos2,modifiers:egui::Modifiers) {
        for pressed in [true,false] {frame(app,ctx,modifiers,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers}]);}
    }
    #[test]
    fn shift_slider_and_fullscreen_tools_change_sizes_without_drawing() {
        let mut app=GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Quick controls",40.,50.,120.));
        let ctx=egui::Context::default();
        for _ in 0..3 {frame(&mut app,&ctx,Default::default(),vec![]);}
        assert!(app.quick_controls.rects.is_empty(),"The toolbar must stay hidden outside fullscreen");
        frame(&mut app,&ctx,egui::Modifiers::SHIFT,vec![egui::Event::PointerMoved(Pos2::new(600.,400.))]);
        for _ in 0..2 {frame(&mut app,&ctx,egui::Modifiers::SHIFT,vec![]);}
        assert!(app.quick_controls.ctrl);
        let slider=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_size_slider")).unwrap());
        let old=app.settings.pencil_core_diameter_mm;
        click(&mut app,&ctx,Pos2::new(slider.left()+155.,slider.center().y),egui::Modifiers::SHIFT);
        assert!(app.settings.pencil_core_diameter_mm>old);
        assert_eq!(app.tip_state.core_diameter_mm,app.settings.pencil_core_diameter_mm);
        frame(&mut app,&ctx,Default::default(),vec![]);assert!(!app.quick_controls.ctrl);
        frame(&mut app,&ctx,egui::Modifiers::CTRL|egui::Modifiers::SHIFT,vec![egui::Event::Key {
            key:egui::Key::Plus,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::CTRL|egui::Modifiers::SHIFT
        }]);
        frame(&mut app,&ctx,egui::Modifiers::SHIFT,vec![]);
        assert!(!app.quick_controls.ctrl,"Releasing Ctrl after Ctrl+Shift+Plus must not open the size slider");
        frame(&mut app,&ctx,Default::default(),vec![]);
        for tool in [ToolKind::Eraser,ToolKind::Smudge,ToolKind::Tissue,ToolKind::Pencil] {
            // Exercise real toolbar clicks and modifier-only frames, followed by a held-Shift drag.
            let icon=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("test_tool_icon",tool.label()))).unwrap());
            click(&mut app,&ctx,icon.center(),Default::default());
            for _ in 0..3 {frame(&mut app,&ctx,egui::Modifiers::SHIFT,vec![egui::Event::PointerMoved(Pos2::new(600.,400.))]);}
            assert!(app.quick_controls.ctrl,"Shift must open size after selecting {tool:?}");
            let r=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_size_slider")).unwrap());
            let old=app.quick_size().unwrap().0;
            click(&mut app,&ctx,Pos2::new(r.left()+80.,r.center().y),egui::Modifiers::SHIFT);
            assert_ne!(app.quick_size().unwrap().0,old);
            frame(&mut app,&ctx,Default::default(),vec![]);
            assert!(!app.quick_controls.ctrl);
        }
        app.fullscreen=true;
        for _ in 0..4 {frame(&mut app,&ctx,Default::default(),vec![]);}
        for tool in [ToolKind::Eraser,ToolKind::Tissue,ToolKind::Smudge,ToolKind::Pencil] {
            let button=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("quick_tool",tool.label()))).unwrap());
            assert!(button.width()>=48. && button.height()>=48.);
            click(&mut app,&ctx,button.center(),Default::default());
            assert_eq!(app.settings.tool,tool);
            let size=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_size_button")).unwrap());
            click(&mut app,&ctx,size.center(),Default::default());
            for _ in 0..2 {frame(&mut app,&ctx,Default::default(),vec![]);}
            assert!(app.quick_controls.button_open);
            let slider=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_size_slider")).unwrap());
            let old=app.quick_size().unwrap().0;
            click(&mut app,&ctx,Pos2::new(slider.left()+70.,slider.center().y),Default::default());
            assert_ne!(app.quick_size().unwrap().0,old);
            click(&mut app,&ctx,size.center(),Default::default());
            assert!(!app.quick_controls.button_open);
        }
        assert!(app.document.surface.graphite_mass.iter().all(|&m|m==0.));
        assert!(!app.history.can_undo());
        let toggle=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_toolbar_toggle")).unwrap());
        app.quick_controls.button_open=true;
        click(&mut app,&ctx,toggle.center(),Default::default());
        assert!(app.quick_controls.collapsed);
        assert!(!app.quick_controls.button_open);
        ctx.data_mut(|d|d.remove::<Rect>(egui::Id::new(("quick_tool",ToolKind::Pencil.label()))));
        for _ in 0..3 {frame(&mut app,&ctx,Default::default(),vec![]);}
        assert!(ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("quick_tool",ToolKind::Pencil.label()))).is_none()));
        let toggle=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_toolbar_toggle")).unwrap());
        click(&mut app,&ctx,toggle.center(),Default::default());
        assert!(!app.quick_controls.collapsed);
        for _ in 0..3 {frame(&mut app,&ctx,Default::default(),vec![]);}
        assert!(ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("quick_tool",ToolKind::Pencil.label()))).is_some()));
        click(&mut app,&ctx,toggle.center(),Default::default());
        assert!(app.quick_controls.collapsed);
        let exit=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("fullscreen_exit_button")).unwrap());
        assert!(exit.width()>=48. && exit.height()>=48.);
        click(&mut app,&ctx,exit.center(),Default::default());
        assert!(!app.fullscreen,"The X button must exit even when the toolbar is collapsed");
        assert!(app.document.surface.graphite_mass.iter().all(|&m|m==0.));
        assert!(!app.history.can_undo());
        app.quick_controls.button_open=true;
        app.fullscreen=false;
        for _ in 0..3 {frame(&mut app,&ctx,Default::default(),vec![]);}
        assert!(!app.quick_controls.button_open,"Leaving fullscreen closes its Size popup");
        assert!(app.quick_controls.rects.is_empty(),"Leaving fullscreen removes the toolbar and its hit targets");
        let mut preview=crate::ui::test_render::Preview::default();
        // A fresh context supplies complete textures for the headless screenshot.
        let fresh=egui::Context::default();
        let mut view_app=GraphiteApp::with_render_state(None);view_app.fullscreen=true;
        for _ in 0..3 {let out=frame(&mut view_app,&fresh,Default::default(),vec![]);preview.update(&out);preview.save(&fresh,&out,"../../ui-v242-fullscreen.png",[1440,920]);}
        let fresh=egui::Context::default();let mut preview=crate::ui::test_render::Preview::default();
        let mut view_app=GraphiteApp::with_render_state(None);
        for _ in 0..3 {let out=frame(&mut view_app,&fresh,egui::Modifiers::SHIFT,vec![egui::Event::PointerMoved(Pos2::new(680.,400.))]);preview.update(&out);preview.save(&fresh,&out,"../../ui-v245-shift-size.png",[1440,920]);}
    }
    #[test]
    fn physical_modifier_key_events_open_size_and_real_chords_stay_blocked() {
        use graphite_studio::shortcuts::SizeModifier;
        let mut app=GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Modifier input",40.,50.,120.));
        let ctx=egui::Context::default();
        let key_event=|key,pressed,modifiers|egui::Event::Key {
            key,physical_key:Some(key),pressed,repeat:false,modifiers,
        };
        for (binding,modifiers,keys) in [
            (SizeModifier::Shift,egui::Modifiers::SHIFT,[egui::Key::ShiftLeft,egui::Key::ShiftRight]),
            (SizeModifier::Ctrl,egui::Modifiers::CTRL,[egui::Key::ControlLeft,egui::Key::ControlRight]),
            (SizeModifier::Alt,egui::Modifiers::ALT,[egui::Key::AltLeft,egui::Key::AltRight]),
        ] {
            app.shortcuts.bindings.size_modifier=binding;
            for key in keys {
                frame(&mut app,&ctx,egui::Modifiers::NONE,vec![]);
                frame(&mut app,&ctx,modifiers,vec![key_event(key,true,modifiers)]);
                assert!(app.quick_controls.ctrl,"Physical {key:?} press must open size");
                frame(&mut app,&ctx,modifiers,vec![]);
                assert!(app.quick_controls.ctrl,"Holding {key:?} must keep size open");
                frame(&mut app,&ctx,egui::Modifiers::NONE,vec![key_event(key,false,egui::Modifiers::NONE)]);
                assert!(!app.quick_controls.ctrl);
                assert!(app.quick_controls.is_open(),"Releasing {key:?} must keep size open");
                click(&mut app,&ctx,Pos2::new(1200.,700.),egui::Modifiers::NONE);
                assert!(!app.quick_controls.is_open(),"Clicking outside must close size");
                frame(&mut app,&ctx,modifiers,vec![key_event(key,true,modifiers),key_event(egui::Key::F6,true,modifiers)]);
                assert!(!app.quick_controls.ctrl,"A real chord must not open size");
                frame(&mut app,&ctx,modifiers,vec![key_event(egui::Key::F6,false,modifiers)]);
                assert!(!app.quick_controls.ctrl,"The rest of a chord hold stays blocked");
                frame(&mut app,&ctx,egui::Modifiers::NONE,vec![key_event(key,false,egui::Modifiers::NONE)]);
            }
        }
        assert!(!app.history.can_undo());
    }
    #[test]
    fn released_shift_slider_adjusts_then_dismisses_without_drawing() {
        let mut app=GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Popup interaction",40.,50.,120.));
        let ctx=egui::Context::default();
        let mut preview=crate::ui::test_render::Preview::default();
        for _ in 0..3 {preview.update(&frame(&mut app,&ctx,egui::Modifiers::NONE,vec![]));}
        let key=|pressed,modifiers|egui::Event::Key {
            key:egui::Key::ShiftLeft,physical_key:Some(egui::Key::ShiftLeft),pressed,repeat:false,modifiers,
        };
        // A quick press and release can both reach the same native input frame.
        preview.update(&frame(&mut app,&ctx,egui::Modifiers::NONE,vec![
            egui::Event::PointerMoved(Pos2::new(600.,400.)),
            key(true,egui::Modifiers::SHIFT),key(false,egui::Modifiers::NONE),
        ]));
        for _ in 0..3 {preview.update(&frame(&mut app,&ctx,egui::Modifiers::NONE,vec![]));}
        assert!(app.quick_controls.is_open());
        let slider=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("quick_size_slider")).unwrap());
        let before=app.quick_size().unwrap().0;
        click(&mut app,&ctx,Pos2::new(slider.left()+150.,slider.center().y),egui::Modifiers::NONE);
        assert_ne!(app.quick_size().unwrap().0,before);
        assert!(app.quick_controls.is_open(),"Adjusting without Shift keeps the popup open");
        let popup=app.quick_controls.popup_rect.unwrap();
        assert!(popup.height()<70.,"Only one slider row should remain");
        let out=frame(&mut app,&ctx,egui::Modifiers::NONE,vec![]);
        preview.update(&out);
        preview.save(&ctx,&out,"../../ui-v247-size-popup.png",[1440,920]);
        let outside=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("test_paper_rect")).unwrap().center())+Vec2::new(0.,140.);
        assert!(!popup.contains(outside));
        click(&mut app,&ctx,outside,egui::Modifiers::NONE);
        assert!(!app.quick_controls.is_open());
        assert!(app.history.can_undo(),"The first canvas press after sizing must draw");
        assert!(app.document.surface.graphite_mass.iter().any(|&m|m>0.));
        // Native pen contacts likewise draw on the first press after sizing.
        let mut controls=QuickControls{latched:true,popup_rect:Some(popup),..Default::default()};
        assert!(!controls.block_input(PointerFrame{position:Some(outside),primary_pressed:true,primary_down:true,..Default::default()}));
        assert!(!controls.is_open());
        assert!(!controls.block_input(PointerFrame{position:Some(outside),primary_down:true,..Default::default()}));
        assert!(!controls.block_input(PointerFrame{position:Some(outside),primary_released:true,..Default::default()}));
        assert!(!controls.block_input(PointerFrame::default()));
    }
    #[test]
    fn held_shift_works_with_button_and_slider_focus_but_not_text_entry() {
        let mut app=GraphiteApp::with_render_state(None);
        let ctx=egui::Context::default();
        let mut text=String::new();
        let mut value=20.;
        for text_focus in [false,true] {
            let _=ctx.run_ui(egui::RawInput{modifiers:egui::Modifiers::SHIFT,..Default::default()},|ui| {
                let r=if text_focus {ui.text_edit_singleline(&mut text)}else{ui.button("Pencil")};
                r.request_focus();
                assert!(ctx.egui_wants_keyboard_input());
                app.prepare_quick_controls(ui);
                assert_eq!(app.quick_controls.ctrl,!text_focus);
            });
        }
        let _=ctx.run_ui(egui::RawInput{modifiers:egui::Modifiers::SHIFT,..Default::default()},|ui| {
            ui.add(egui::Slider::new(&mut value,0.0..=200.0)).request_focus();
            assert!(ctx.egui_wants_keyboard_input());
            app.prepare_quick_controls(ui);
            assert!(app.quick_controls.ctrl,"Dragging a size slider must not dismiss the held-Ctrl popup");
        });
        frame(&mut app,&ctx,Default::default(),vec![]);
        assert!(!app.quick_controls.ctrl);
    }
    #[test]
    fn toolbar_prefers_empty_margins_and_size_drag_keeps_ownership_until_lift() {
        let view=Rect::from_min_max(Pos2::ZERO,Pos2::new(1440.,920.));
        let paper=Rect::from_min_max(Pos2::new(400.,20.),Pos2::new(1040.,900.));
        let (pos,vertical)=toolbar_placement(paper,view,true).unwrap();assert!(vertical);
        assert!(pos.x+68.<paper.left());
        assert!(toolbar_placement(paper,view,false).is_none());
        assert!(toolbar_placement(view,view,false).is_none());
        assert!(toolbar_placement(view,view,true).is_some());
        let mut controls=QuickControls::default();controls.latched=true;
        assert!(controls.block_input(PointerFrame{primary_pressed:true,primary_down:true,..Default::default()}));
        controls.close_size();
        assert!(controls.block_input(PointerFrame{primary_down:true,..Default::default()}));
        assert!(controls.block_input(PointerFrame{primary_released:true,..Default::default()}));
        assert!(!controls.block_input(PointerFrame::default()));
    }
}
impl QuickControls {
    pub(super) fn close_size(&mut self) {
        self.chord |= self.ctrl;
        self.ctrl=false;self.latched=false;self.button_open=false;
        self.popup_rect=None;self.focus_slider=false;
    }
    fn is_open(&self)->bool {self.latched || self.button_open}
    fn dismiss_outside(&mut self,position:Option<Pos2>,pressed:bool) {
        if self.is_open() && pressed && position.is_some_and(|p|self.popup_rect.is_some_and(|r|!r.contains(p))) {
            self.close_size();
            // A fresh canvas press closes Size and begins the next stroke.
            // Contacts begun on a control remain blocked until they lift.
            self.pointer_down=false;
            self.dismissed_press=true;
        }
    }
    pub(super) fn covers(&self,position:Option<Pos2>)->bool {
        self.is_open() || position.is_some_and(|p|self.rects.iter().any(|r|r.contains(p)))
    }
    pub(super) fn block_input(&mut self,input:PointerFrame)->bool {
        // A new contact also clears stale ownership after a driver missed Up.
        if input.primary_pressed { self.pointer_down=false; }
        // Native pen packets can arrive without a promoted egui mouse press.
        self.dismiss_outside(input.position,input.primary_pressed);
        let over=self.covers(input.position);
        if over && (input.primary_pressed || input.primary_down) {self.pointer_down=true;}
        let block=over || self.pointer_down;
        if input.primary_released {self.pointer_down=false;self.dismissed_press=false;}
        block
    }
}

/// Prefer letterboxed margins; a compact overlay stays accessible when zoomed in.
fn toolbar_placement(paper:Rect,view:Rect,fullscreen:bool)->Option<(Pos2,bool)> {
    if !fullscreen {return None;}
    let view=view.shrink(8.);
    let vertical=Vec2::new(68.,370.);
    if view.height()>=vertical.y {
        if paper.left()-view.left()>=vertical.x+8. {
            return Some((Pos2::new(view.left(),view.center().y-vertical.y*0.5),true));
        }
        if view.right()-paper.right()>=vertical.x+8. {
            return Some((Pos2::new(view.right()-vertical.x,view.center().y-vertical.y*0.5),true));
        }
    }
    let horizontal=Vec2::new(382.,68.);
    if view.width()>=horizontal.x {
        if view.bottom()-paper.bottom()>=horizontal.y+8. {
            return Some((Pos2::new(view.center().x-horizontal.x*0.5,view.bottom()-horizontal.y),false));
        }
        if paper.top()-view.top()>=horizontal.y+8. {
            return Some((Pos2::new(view.center().x-horizontal.x*0.5,view.top()),false));
        }
    }
    fullscreen.then_some((view.min,view.height()>=vertical.y))
}

impl GraphiteApp {
    pub(super) fn prepare_quick_controls(&mut self,ui:&egui::Ui) {
        let (held,ctrl,activation_pressed,other_key,focus)=ui.input(|i| (
            self.shortcuts.bindings.size_modifier.held(i.modifiers),
            self.shortcuts.bindings.size_modifier.alone(i.modifiers),
            i.events.iter().any(|e|matches!(e,egui::Event::Key{key,pressed:true,modifiers,..}
                if self.shortcuts.bindings.size_modifier.is_activation_key(*key)
                    && self.shortcuts.bindings.size_modifier.alone(*modifiers))),
            // egui 0.35 emits physical left/right modifier presses as Key events.
            // The activating modifier itself is not an additional shortcut key.
            i.events.iter().any(|e|matches!(e,egui::Event::Key{key,pressed:true,..}
                if !self.shortcuts.bindings.size_modifier.is_activation_key(*key))),
            i.focused,
        ));
        if !held {self.quick_controls.chord=false;}
        if (held || activation_pressed) && other_key {
            self.quick_controls.close_size();self.quick_controls.chord=true;
        }
        let (pressed,position)=ui.input(|i|(i.pointer.any_pressed(),i.pointer.hover_pos()));
        self.quick_controls.dismiss_outside(position,pressed);
        // Buttons and sliders retain keyboard focus after a click. Only text
        // entry should suppress the held modifier, not every focused widget.
        let sizing_tool=matches!(self.settings.tool,ToolKind::Pencil|ToolKind::Eraser|ToolKind::Smudge|ToolKind::Tissue|ToolKind::Brush) || self.editing.transform.is_some();
        let active=(ctrl || activation_pressed) && sizing_tool && !self.rotate_view && !self.canvas_contact_active() && !self.transform_dragging()
            && !self.quick_controls.chord && focus && !ui.ctx().text_edit_focused()
            && !self.gallery.open && !self.paper_settings.open && !self.save_as_open && !self.shortcuts.open;
        if active && !self.quick_controls.ctrl && !self.quick_controls.is_open() {
            self.finish_stroke();
            self.quick_controls.anchor=position.map(|p|p+Vec2::splat(16.));
            self.quick_controls.latched=true;
            self.quick_controls.focus_slider=true;
        }
        self.quick_controls.ctrl=active;
        if !focus || !sizing_tool || self.rotate_view || self.gallery.open || self.paper_settings.open || self.save_as_open || self.shortcuts.open {
            self.quick_controls.close_size();
            if !focus {self.quick_controls.pointer_down=false;}
        }
        if self.quick_controls.is_open() && ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape)) {
            self.quick_controls.close_size();
        }
    }

    fn quick_size(&self)->Option<(f32,f32,f32,&'static str)> {
        if self.liquify.session.is_some(){return Some((self.liquify.settings.size,2.,1200.,"px"));}
        if let Some(scale)=self.transform_scale_percent(){return Some((scale,1.,400.,"%"));}
        let dpi=self.document.spec.dpi;
        match self.settings.tool {
            ToolKind::Pencil=>Some((self.settings.pencil_core_diameter_mm*dpi/25.4,1.5*dpi/25.4,200.,"px")),
            ToolKind::Eraser=>Some((self.settings.eraser_diameter_mm*dpi/25.4,0.8*dpi/25.4,200.,"px")),
            ToolKind::Smudge=>Some((self.settings.smudge_size_px,2.,128.,"px")),
            ToolKind::Tissue=>Some((self.settings.tissue_size_px,2.,200.,"px")),
            ToolKind::Brush=>Some((self.settings.brush_size_px,1.,200.,"px")),
            ToolKind::Shapes=>Some((self.settings.shape_width_px,1.,200.,"px")),
            _=>None,
        }
    }

    fn set_quick_size(&mut self,value:f32) {
        if !value.is_finite(){return;}
        if self.liquify.session.is_some(){self.liquify.settings.size=value.clamp(2.,1200.);return;}
        if self.editing.transform.is_some(){self.set_transform_scale_percent(value);return;}
        let Some((_,min,max,_))=self.quick_size() else{return;};
        let value=value.clamp(min,max);
        self.finish_stroke();
        match self.settings.tool {
            ToolKind::Pencil=>{
                self.settings.pencil_core_diameter_mm=value*25.4/self.document.spec.dpi;
                self.tip_state.sync_core_diameter(self.settings.pencil_core_diameter_mm);
                self.remember_pencil_controls();
            },
            ToolKind::Eraser=>self.settings.eraser_diameter_mm=value*25.4/self.document.spec.dpi,
            ToolKind::Smudge=>self.settings.smudge_size_px=value,
            ToolKind::Tissue=>self.settings.tissue_size_px=value,
            ToolKind::Brush=>self.settings.brush_size_px=value,
            ToolKind::Shapes=>self.settings.shape_width_px=value,
            _=>{},
        }
    }

    pub(super) fn show_quick_controls(&mut self,ui:&egui::Ui,paper:Rect,view:Rect) {
        self.quick_controls.rects.clear();
        self.quick_controls.popup_rect=None;
        if !self.fullscreen {self.quick_controls.button_open=false;}
        if self.gallery.open || self.paper_settings.open || self.save_as_open || self.shortcuts.open{return;}
        if self.fullscreen {
            let exit=egui::Area::new(egui::Id::new("fullscreen_exit"))
                .order(egui::Order::Foreground).fixed_pos(Pos2::new(view.right()-56.,view.top()+8.))
                .movable(false).fade_in(false).show(ui.ctx(),|ui| {
                    let button=ui.add_sized([48.,48.],egui::Button::new(egui::RichText::new("×").size(28.)))
                        .on_hover_text(graphite_studio::shortcuts::hint(ui.ctx(),"Leave fullscreen",Command::Cancel));
                    #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("fullscreen_exit_button"),button.rect));
                    button.clicked()
                });
            self.quick_controls.rects.push(exit.response.rect);
            if exit.inner {
                self.fullscreen=false;
                self.quick_controls.close_size();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ui.ctx().request_repaint();
                return;
            }
        }
        if let Some((pos,vertical))=toolbar_placement(paper,view,self.fullscreen) {
            let vertical=self.quick_controls.toolbar_vertical.unwrap_or(vertical);
            let pos=self.quick_controls.toolbar_position.unwrap_or(pos).clamp(view.min,(view.max-self.quick_controls.toolbar_size).max(view.min));
            let response=egui::Area::new(egui::Id::new("canvas_quick_toolbar"))
                .order(egui::Order::Foreground).fixed_pos(pos).movable(false).fade_in(false)
                .show(ui.ctx(),|ui| {
                    egui::Frame::popup(ui.style()).show(ui,|ui| {
                        let layout=if vertical {egui::Layout::top_down(egui::Align::Center)}else{egui::Layout::left_to_right(egui::Align::Center)};
                        ui.with_layout(layout,|ui| {
                            ui.spacing_mut().item_spacing=Vec2::splat(4.);
                            let (grip,drag)=ui.allocate_exact_size(if vertical {Vec2::new(48.,22.)}else{Vec2::new(22.,48.)},Sense::drag());
                            for x in [-1.,1.] {for y in [-1.,0.,1.] {ui.painter().circle_filled(grip.center()+Vec2::new(x*4.,y*5.),1.6,Color32::from_gray(110));}}
                            #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("quick_toolbar_grip"),grip));
                            if drag.dragged(){self.quick_controls.toolbar_position=Some(pos+ui.input(|i|i.pointer.delta()));self.quick_controls.toolbar_vertical=Some(vertical);self.quick_controls.pointer_down=true;}
                            drag.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text("Drag to move toolbar");
                            let collapsed=self.quick_controls.collapsed;
                            let arrow=match (vertical,collapsed) {(true,true)=>"▶",(true,false)=>"◀",(false,true)=>"▼",(false,false)=>"▲"};
                            let toggle=ui.add_sized([48.,36.],egui::Button::new(arrow))
                                .on_hover_text(if collapsed {"Show fullscreen toolbar"} else {"Hide fullscreen toolbar"});
                            #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("quick_toolbar_toggle"),toggle.rect));
                            if toggle.clicked() {
                                self.quick_controls.collapsed=!collapsed;
                                self.quick_controls.close_size();
                            }
                            if self.quick_controls.collapsed {return;}
                            for tool in [ToolKind::Pencil,ToolKind::Eraser,ToolKind::VectorSelect,ToolKind::Tissue,ToolKind::Smudge] {
                                let transform=tool==ToolKind::VectorSelect;
                                let selected=self.liquify.session.is_none() && if transform {self.editing.transform.is_some() || self.settings.tool==tool}else{self.editing.transform.is_none() && self.settings.tool==tool};
                                let response=if transform {
                                    let (rect,r)=ui.allocate_exact_size(Vec2::splat(48.),Sense::click());
                                    let visuals=ui.style().interact_selectable(&r,selected);
                                    ui.painter().rect(rect.shrink(1.),3.,visuals.bg_fill,visuals.bg_stroke,egui::StrokeKind::Inside);
                                    let bounds=rect.shrink(12.);
                                    ui.painter().rect_stroke(bounds,0.,Stroke::new(1.5,visuals.fg_stroke.color),egui::StrokeKind::Inside);
                                    for p in [bounds.left_top(),bounds.right_top(),bounds.left_bottom(),bounds.right_bottom()] {
                                        ui.painter().rect_filled(Rect::from_center_size(p,Vec2::splat(5.)),0.,visuals.fg_stroke.color);
                                    }
                                    r.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Button,ui.is_enabled(),selected,"Transform"));
                                    r.on_hover_text(graphite_studio::shortcuts::hint(ui.ctx(),"Transform",Command::Transform))
                                }else{crate::ui::tool_icons::tool_button_sized(ui,tool,selected,48.)};
                                #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new(("quick_tool",tool.label())),response.rect));
                                if response.clicked() {
                                    self.quick_controls.close_size();
                                    if transform {
                                        self.begin_transform(ui.ctx());
                                        if self.editing.transform.is_none(){self.select_tool(ToolKind::VectorSelect);}
                                    }else{self.select_tool(tool);}
                                }
                                if transform && crate::ui::tool_icons::liquify_button(ui,self.liquify.session.is_some(),48.).clicked(){self.begin_liquify();}
                            }
                            let response=ui.add_enabled(self.quick_size().is_some(),egui::Button::new("Size").min_size(Vec2::splat(48.)));
                            #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("quick_size_button"),response.rect));
                            if response.clicked() && !self.quick_controls.dismissed_press {
                                self.finish_stroke();
                                self.quick_controls.button_open=!self.quick_controls.button_open;
                                self.quick_controls.focus_slider=self.quick_controls.button_open;
                                self.quick_controls.anchor=Some(if vertical {response.rect.right_top()+Vec2::new(12.,0.)}else{response.rect.left_top()-Vec2::new(0.,64.)});
                            }
                        });
                    });
                });
            self.quick_controls.rects.push(response.response.rect);
            self.quick_controls.toolbar_size=response.response.rect.size();
            #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("quick_toolbar"),response.response.rect));
        }
        if self.quick_controls.is_open() {
            let size=Vec2::new(290.,60.);
            let limit=view.shrink(8.);
            let pos=self.quick_controls.anchor.unwrap_or(view.center()-size*0.5)
                .clamp(limit.min,(limit.max-size).max(limit.min));
            let response=egui::Area::new(egui::Id::new("quick_size_popup"))
                .order(egui::Order::Foreground).fixed_pos(pos).movable(false).fade_in(false)
                .show(ui.ctx(),|ui| {
                    egui::Frame::popup(ui.style()).show(ui,|ui| {
                        ui.set_width(270.);ui.spacing_mut().slider_width=190.;ui.spacing_mut().interact_size.y=44.;
                        if let Some((mut value,min,max,unit))=self.quick_size() {
                            let r=ui.add(egui::Slider::new(&mut value,min..=max).logarithmic(true).suffix(format!(" {unit}")).max_decimals(1));
                            if self.quick_controls.focus_slider {r.request_focus();self.quick_controls.focus_slider=false;}
                            #[cfg(test)] ui.data_mut(|d|d.insert_temp(egui::Id::new("quick_size_slider"),r.rect));
                            if r.changed(){self.set_quick_size(value);}
                        }
                    });
                });
            self.quick_controls.rects.push(response.response.rect);
            self.quick_controls.popup_rect=Some(response.response.rect);
        }
    }
}
