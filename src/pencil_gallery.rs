use crate::core::pencil::{ToolKind, ToolSettings};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct PencilPreset {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub folder: String,
    pub settings: ToolSettings,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FolderCollection {
    pub name: String,
    pub source: std::path::PathBuf,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Gallery {
    pub pencils: Vec<PencilPreset>,
    #[serde(default)]
    pub collections: Vec<FolderCollection>,
}
impl Gallery {
    pub fn ensure_ids(&mut self) -> bool {
        let mut changed=false;
        let mut used=std::collections::HashSet::new();
        for pencil in &mut self.pencils {
            if pencil.id.is_empty() || pencil.id=="standard" || !used.insert(pencil.id.clone()) {
                pencil.id=new_pencil_id();used.insert(pencil.id.clone());changed=true;
            }
        }
        changed
    }
    /// Import files from the selected directory once. Collections embed their
    /// tips; removal never changes the user's source directory or files.
    pub fn add_folder(&mut self, directory: &Path, settings: &ToolSettings) -> Result<String,String> {
        let source=directory.canonicalize().map_err(|e|e.to_string())?;
        let same_path=|a:&Path,b:&Path| if cfg!(windows) {a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())}else{a==b};
        if self.collections.iter().any(|c|same_path(&c.source,&source)) {return Err("This folder collection is already added.".into());}
        if self.collections.len()>=256{return Err("The gallery holds up to 256 folder collections.".into());}
        let base:String=source.file_name().and_then(|s|s.to_str()).unwrap_or("Collection").chars().take(80).collect();
        let mut name=base.clone();let mut suffix=2;
        while self.collections.iter().any(|c|c.name==name) || self.pencils.iter().any(|p|p.folder==name) {
            name=format!("{base} ({suffix})");suffix+=1;
        }
        let mut files=Vec::new();
        for entry in std::fs::read_dir(&source).map_err(|e|e.to_string())? {
            let entry=entry.map_err(|e|e.to_string())?;
            if entry.file_type().map_err(|e|e.to_string())?.is_file() && entry.path().extension().is_some_and(|e|e.eq_ignore_ascii_case("abr")) {
                files.push(entry.path());
                if files.len()>256{return Err("The folder has more than 256 ABR files.".into());}
            }
        }
        files.sort();
        let mut next=self.clone();let mut total_bytes=0u64;
        for file in files {
            let size=file.metadata().map_err(|e|e.to_string())?.len();total_bytes+=size;
            if total_bytes>128*1024*1024{return Err("Folder ABR files exceed the 128 MB import limit.".into());}
            let bytes=std::fs::read(&file).map_err(|e|e.to_string())?;
            let tips=crate::core::brush::import_abr(&bytes,file.file_stem().and_then(|s|s.to_str()).unwrap_or("Pencil"))
                .map_err(|e|format!("{}: {e}",file.display()))?;
            for tip in tips {let mut s=settings.clone();s.pencil_texture=Some(tip.clone());next.add(&tip.name,&name,&s)?;}
        }
        next.collections.push(FolderCollection{name:name.clone(),source});
        *self=next;Ok(name)
    }
    pub fn delete_folder(&mut self, name:&str) {
        if name.is_empty(){return;}
        self.pencils.retain(|p|p.folder!=name);
        self.collections.retain(|c|c.name!=name);
    }
    pub fn add(&mut self, name: &str, folder: &str, settings: &ToolSettings) -> Result<(), String> {
        if self.pencils.len() >= 256 {
            return Err("The gallery holds up to 256 pencils.".into());
        }
        let mut settings = settings.clone();
        settings.tool = ToolKind::Pencil;
        settings.brush_tip = None;
        let preset = PencilPreset {
            id: new_pencil_id(),
            name: name.trim().chars().take(100).collect(),
            folder: folder.trim().chars().take(100).collect(),
            settings,
        };
        if preset.name.is_empty() {
            return Err("Enter a pencil name.".into());
        }
        self.pencils.push(preset);
        if !self.valid() {
            self.pencils.pop();
            return Err("Pencil gallery exceeds its supported size.".into());
        }
        Ok(())
    }
    fn valid(&self) -> bool {
        self.pencils.len() <= 256
            && self.collections.len()<=256
            && self.collections.iter().all(|c|!c.name.is_empty() && c.name.len()<=400 && c.source.as_os_str().len()<=32768)
            && self.pencils.iter().all(|p| {
                p.id.len()<=128 && !p.name.is_empty()
                    && p.name.len() <= 400
                    && p.folder.len() <= 400
                    && crate::project::settings_valid(&p.settings)
            })
            && self
                .pencils
                .iter()
                .filter_map(|p| p.settings.pencil_texture.as_ref())
                .map(|t| t.mask.len())
                .sum::<usize>()
                <= 64 * 1024 * 1024
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if !self.valid() {
            return Err("Invalid pencil gallery.".into());
        }
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let result = (|| -> Result<(), String> {
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            let mut writer = BufWriter::with_capacity(256 * 1024, encoder);
            serde_json::to_writer(&mut writer, self).map_err(|e| e.to_string())?;
            writer.flush().map_err(|e| e.to_string())?;
            let encoder = writer.into_inner().map_err(|e| e.to_string())?;
            let file = encoder.finish().map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            std::fs::rename(&temp, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
            return Err("Gallery is too large.".into());
        }
        let decoder = flate2::read::GzDecoder::new(BufReader::new(file));
        let gallery: Self = serde_json::from_reader(BufReader::with_capacity(
            256 * 1024,
            decoder.take(320 * 1024 * 1024),
        ))
        .map_err(|e| e.to_string())?;
        if !gallery.valid() {
            return Err("Invalid pencil gallery.".into());
        }
        Ok(gallery)
    }
}
impl PencilPreset {
    pub fn apply(&self, target: &mut ToolSettings) {
        let s = &self.settings;
        target.tool = ToolKind::Pencil;
        target.pencil_id = self.id.clone();
        target.grade = s.grade;
        target.pencil_core_diameter_mm = s.pencil_core_diameter_mm;
        target.pencil_geometry_scale = s.pencil_geometry_scale;
        target.pressure_width = s.pressure_width;
        target.tip_sharpness = s.tip_sharpness;
        target.flow = s.flow;
        target.pencil_color_rgb = s.pencil_color_rgb;
        target.particle_variation = s.particle_variation;
        target.line_smoothing = s.line_smoothing;
        target.hold_to_straighten = s.hold_to_straighten;
        target.tilt_deg = s.tilt_deg;
        target.azimuth_deg = s.azimuth_deg;
        target.auto_azimuth = s.auto_azimuth;
        target.pencil_texture = s.pencil_texture.clone();
        target.pencil_tip_shape = true;
    }
}

fn new_pencil_id() -> String {
    static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(1);
    format!("{:x}-{:x}-{:x}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_directory_collections_import_persist_and_delete_without_touching_sources() {
        let root=std::env::temp_dir().join(format!("graphite-collections-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let first=root.join("one").join("Pencils");let second=root.join("two").join("Pencils");let empty=root.join("Empty");
        for p in [&first,&second,&empty]{std::fs::create_dir_all(p).unwrap();}
        let mut record=vec![0u8;15];
        for v in [0i32,0,2,3]{record.extend(v.to_be_bytes());}
        record.extend(8u16.to_be_bytes());record.push(0);record.extend([0,128,255,90,90,90]);
        let mut abr=vec![0,1,0,1,0,2];abr.extend((record.len() as u32).to_be_bytes());abr.extend(record);
        std::fs::write(first.join("One.abr"),&abr).unwrap();std::fs::write(second.join("Two.ABR"),&abr).unwrap();
        let mut gallery=Gallery::default();let settings=ToolSettings::default();
        let a=gallery.add_folder(&first,&settings).unwrap();let b=gallery.add_folder(&second,&settings).unwrap();
        assert_ne!(a,b);assert_eq!(gallery.pencils.len(),2);
        assert!(gallery.add_folder(&first,&settings).is_err());
        gallery.add_folder(&empty,&settings).unwrap();
        std::fs::write(empty.join("Bad.abr"),[0,99]).unwrap();
        let before=serde_json::to_vec(&gallery).unwrap();
        let invalid=root.join("Invalid");std::fs::create_dir(&invalid).unwrap();std::fs::write(invalid.join("Bad.abr"),[0,99]).unwrap();
        assert!(gallery.add_folder(&invalid,&settings).is_err());assert_eq!(serde_json::to_vec(&gallery).unwrap(),before);
        gallery.delete_folder(&a);assert_eq!(gallery.pencils.len(),1);
        assert_eq!(std::fs::read(first.join("One.abr")).unwrap(),abr);
        let saved=root.join("Pencils.gallery");gallery.save(&saved).unwrap();let restored=Gallery::load(&saved).unwrap();
        assert_eq!(restored.collections.len(),2);assert_eq!(restored.pencils[0].folder,b);
        assert_eq!(restored.pencils[0].settings.pencil_texture.as_ref().unwrap().mask,[0,128,255,90,90,90]);
        let legacy:Gallery=serde_json::from_str("{\"pencils\":[]}").unwrap();assert!(legacy.collections.is_empty());
        for p in [first.join("One.abr"),second.join("Two.ABR"),empty.join("Bad.abr"),invalid.join("Bad.abr"),saved]{std::fs::remove_file(p).unwrap();}
        for p in [first,second,empty,invalid,root.join("one"),root.join("two"),root]{std::fs::remove_dir(p).unwrap();}
    }

    #[test]
    fn gallery_persists_folders_embedded_tips_and_preserves_device_preferences() {
        let mut g = Gallery::default();
        let mut s = ToolSettings::default();
        s.pencil_texture = Some(std::sync::Arc::new(crate::core::brush::BrushTip {
            name: "Test".into(),
            width: 2,
            height: 2,
            mask: vec![0, 128, 200, 255],
        }));
        g.add("My pencil", "Sketches", &s).unwrap();
        let path = std::env::temp_dir().join(format!(
            "graphite-gallery-test-{}.gallery",
            std::process::id()
        ));
        g.save(&path).unwrap();
        let loaded = Gallery::load(&path).unwrap();
        assert_eq!(loaded.pencils[0].folder, "Sketches");
        assert_eq!(
            loaded.pencils[0]
                .settings
                .pencil_texture
                .as_ref()
                .unwrap()
                .mask,
            [0, 128, 200, 255]
        );
        let mut current = ToolSettings::default();
        current.use_wintab = true;
        current.pen_pressure_gamma = 1.7;
        current.tissue_size_px = 700.;
        loaded.pencils[0].apply(&mut current);
        assert!(current.use_wintab);
        assert_eq!(current.pen_pressure_gamma, 1.7);
        assert_eq!(current.tissue_size_px, 700.);
        g.pencils[0].name.clear();
        assert!(g.save(&path).is_err());
        assert_eq!(Gallery::load(&path).unwrap().pencils[0].name, "My pencil");
        std::fs::remove_file(path).unwrap();
    }
}
