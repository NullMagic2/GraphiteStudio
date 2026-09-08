//! Portable keyboard preferences, independent of drawings and pencil presets.
use eframe::egui::{Event, InputState, Key, Modifiers};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::{Path, PathBuf}};

pub fn hint(ctx:&eframe::egui::Context,text:&str,c:Command)->String {
    let s=ctx.data(|d|d.get_temp::<Shortcuts>(eframe::egui::Id::new("active_shortcuts"))).unwrap_or_default();
    s.binding(c).map(|b|format!("{text} · {}",b.label())).unwrap_or_else(||text.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command { Pencil, PencilAlternate, Eraser, VectorSelect, RotateView, Transform, RotateSelection, Deselect, SelectAll, Cancel, Confirm, Delete, Open, Save, SaveAs, Undo, Redo, ZoomIn, ZoomInAlternate, ZoomOut, Recover, Pan, Fit, Fullscreen }
impl Command {
    pub const ALL: [Self;24]=[Self::Pencil,Self::PencilAlternate,Self::Eraser,Self::VectorSelect,Self::RotateView,Self::Transform,Self::RotateSelection,Self::Deselect,Self::SelectAll,Self::Cancel,Self::Confirm,Self::Delete,Self::Open,Self::Save,Self::SaveAs,Self::Undo,Self::Redo,Self::ZoomIn,Self::ZoomInAlternate,Self::ZoomOut,Self::Recover,Self::Pan,Self::Fit,Self::Fullscreen];
    pub fn label(self)->&'static str { match self {
        Self::Pencil=>"Pencil",Self::PencilAlternate=>"Pencil (alternate / EasyCanvas)",Self::Eraser=>"Eraser",Self::VectorSelect=>"Vector selection",Self::RotateView=>"Rotate paper",Self::Transform=>"Transform selection",Self::RotateSelection=>"Rotate selection",Self::Deselect=>"Deselect",Self::SelectAll=>"Select all",Self::Cancel=>"Cancel / leave fullscreen",Self::Confirm=>"Confirm transform",Self::Delete=>"Delete selected marks",Self::Open=>"Open project",Self::Save=>"Save project",Self::SaveAs=>"Save project as",Self::Undo=>"Undo",Self::Redo=>"Redo",Self::ZoomIn=>"Zoom drawing in",Self::ZoomInAlternate=>"Zoom drawing in (alternate)",Self::ZoomOut=>"Zoom drawing out",Self::Recover=>"Move to main screen",Self::Pan=>"Pan paper (hold + drag)",Self::Fit=>"Fit to screen",Self::Fullscreen=>"Toggle fullscreen"
    }}
    fn id(self)->String {format!("{self:?}")}
    fn default_binding(self)->Option<Binding> {
        use Command::*;
        let (key,ctrl,shift)=match self {
            Pencil=>(Key::P,false,false),PencilAlternate=>(Key::B,false,false),Eraser=>(Key::E,false,false),VectorSelect=>(Key::V,false,false),RotateView=>(Key::R,false,false),Transform=>(Key::T,true,false),RotateSelection=>(Key::R,true,false),Deselect=>(Key::D,true,false),SelectAll=>(Key::A,true,false),Cancel=>(Key::Escape,false,false),Confirm=>(Key::Enter,false,false),Delete=>(Key::Delete,false,false),Open=>(Key::O,true,false),Save=>(Key::S,true,false),SaveAs=>(Key::S,true,true),Undo=>(Key::Z,true,false),Redo=>(Key::Y,true,false),ZoomIn=>(Key::Plus,true,false),ZoomInAlternate=>(Key::Equals,true,false),ZoomOut=>(Key::Minus,true,false),Recover=>(Key::Home,true,true),Pan=>(Key::Space,false,false),Fit|Fullscreen=>return None
        };
        Some(Binding{key:key.name().into(),ctrl,shift,alt:false})
    }
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub struct Binding {pub key:String,pub ctrl:bool,pub shift:bool,pub alt:bool}
impl Binding {
    pub fn capture(key:Key,m:Modifiers)->Self {Self{key:key.name().into(),ctrl:m.ctrl||m.command,shift:m.shift,alt:m.alt}}
    pub fn label(&self)->String {
        format!("{}{}{}{}",if self.ctrl {"Ctrl+"} else {""},if self.alt {"Alt+"} else {""},if self.shift {"Shift+"} else {""},self.key)
    }
    fn matches(&self,key:Key,m:Modifiers)->bool {
        Key::from_name(&self.key)==Some(key) && self.ctrl==(m.ctrl||m.command) && self.alt==m.alt
            && (self.shift==m.shift || (key==Key::Plus && !self.shift))
    }
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
pub enum SizeModifier {Ctrl, #[default] Shift, Alt, Disabled}
impl SizeModifier {
    pub fn is_activation_key(self,key:Key)->bool {
        match self {
            Self::Shift=>matches!(key,Key::ShiftLeft|Key::ShiftRight),
            Self::Ctrl=>matches!(key,Key::ControlLeft|Key::ControlRight),
            Self::Alt=>matches!(key,Key::AltLeft|Key::AltRight),
            Self::Disabled=>false,
        }
    }
    pub fn held(self,m:Modifiers)->bool {match self {Self::Ctrl=>m.ctrl,Self::Shift=>m.shift,Self::Alt=>m.alt,Self::Disabled=>false}}
    pub fn alone(self,m:Modifiers)->bool {self.held(m) && match self {Self::Ctrl=>!m.alt&&!m.shift,Self::Shift=>!m.ctrl&&!m.alt,Self::Alt=>!m.ctrl&&!m.shift,Self::Disabled=>false}}
}
#[derive(Clone,Default,Serialize,Deserialize)]
#[serde(default)]
pub struct Shortcuts {overrides:BTreeMap<String,Option<Binding>>,pub size_modifier:SizeModifier}
impl Shortcuts {
    pub fn binding(&self,c:Command)->Option<Binding> {self.overrides.get(&c.id()).cloned().unwrap_or_else(||c.default_binding())}
    pub fn label(&self,c:Command)->String {self.binding(c).map(|b|b.label()).unwrap_or_else(||"Unassigned".into())}
    pub fn matches(&self,c:Command,key:Key,m:Modifiers)->bool {self.binding(c).is_some_and(|b|b.matches(key,m))}
    pub fn consume(&self,i:&mut InputState,c:Command)->bool {
        let mut found=false;
        i.events.retain(|e| {
            if let Event::Key{key,pressed:true,modifiers,..}=e {if self.matches(c,*key,*modifiers) {found=true;return false;}}
            true
        });found
    }
    pub fn down(&self,i:&InputState,c:Command)->bool {self.binding(c).is_some_and(|b|Key::from_name(&b.key).is_some_and(|key|i.key_down(key)&&b.matches(key,i.modifiers)))}
    pub fn set(&mut self,c:Command,b:Option<Binding>)->Result<(),String> {
        if let Some(b)=&b {
            let key=Key::from_name(&b.key).ok_or("Unknown key")?;
            for other in Command::ALL {if other!=c {if let Some(old)=self.binding(other) {
                for mask in 0..8 {let m=Modifiers{ctrl:mask&1!=0,alt:mask&2!=0,shift:mask&4!=0,..Modifiers::NONE};
                    if old.matches(key,m)&&b.matches(key,m) {return Err(format!("{} is already assigned to {}. Clear that assignment first.",b.label(),other.label()));}
                }
            }}}
        }
        self.overrides.insert(c.id(),b);Ok(())
    }
    pub fn path()->Option<PathBuf> {Some(std::env::current_exe().ok()?.parent()?.join("shortcuts.settings"))}
    pub fn load(path:&Path)->Result<Self,String> {
        if std::fs::metadata(path).map_err(|e|e.to_string())?.len()>65536 {return Err("Shortcut settings are too large".into());}
        let config:Self=serde_json::from_slice(&std::fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let mut validated=config.clone();
        for c in Command::ALL {validated.set(c,config.binding(c))?;}
        Ok(config)
    }
    pub fn save(&self,path:&Path)->Result<(),String> {
        use std::io::Write;
        let temp=path.with_extension(format!("{}.tmp",std::process::id()));
        let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e|e.to_string())?;
        let result=(||->Result<(),String>{f.write_all(&serde_json::to_vec_pretty(self).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;f.sync_all().map_err(|e|e.to_string())?;drop(f);std::fs::rename(&temp,path).map_err(|e|e.to_string())})();
        if result.is_err(){let _=std::fs::remove_file(temp);} result
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn conflicts_exact_modifiers_and_roundtrip() {
        let mut s=Shortcuts::default();
        assert!(s.set(Command::Pencil,Some(Binding::capture(Key::E,Modifiers::NONE))).is_err());
        s.set(Command::Pencil,Some(Binding::capture(Key::F2,Modifiers::CTRL))).unwrap();
        assert!(!s.matches(Command::Pencil,Key::P,Modifiers::NONE));
        assert!(s.matches(Command::Pencil,Key::F2,Modifiers::CTRL));
        assert!(!s.matches(Command::Pencil,Key::F2,Modifiers::CTRL|Modifiers::SHIFT));
        let path=std::env::temp_dir().join(format!("graphite-shortcuts-test-{}.settings",std::process::id()));
        s.save(&path).unwrap();let loaded=Shortcuts::load(&path).unwrap();std::fs::remove_file(path).unwrap();
        assert_eq!(loaded.binding(Command::Pencil),s.binding(Command::Pencil));
        assert!(Shortcuts::default().matches(Command::ZoomIn,Key::Plus,Modifiers::CTRL|Modifiers::SHIFT));
    }
}
