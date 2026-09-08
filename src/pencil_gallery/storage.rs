//! Portable, detected brush resources. The index never contains imported masks.
use super::{Gallery, PencilPreset};
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::BTreeMap, io::{BufReader, BufWriter, Read, Write}, path::{Path, PathBuf}};

fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
        return Err("Pencil file exceeds 128 MB.".into());
    }
    let decoder = flate2::read::GzDecoder::new(BufReader::new(file));
    serde_json::from_reader(BufReader::with_capacity(256 * 1024, decoder.take(320 * 1024 * 1024)))
        .map_err(|e| e.to_string())
}

fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", super::new_pencil_id()));
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| e.to_string())?;
    let result = (|| {
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut writer = BufWriter::with_capacity(256 * 1024, encoder);
        serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        let file = writer.into_inner().map_err(|e| e.to_string())?.finish().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() { let _ = std::fs::remove_file(temp); }
    result
}

fn regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

impl Gallery {
    pub fn imports_path(index: &Path) -> PathBuf {
        index.with_file_name("imported_brushes")
    }

    /// Save numeric presets in the index and each sampled pencil in its own file.
    /// Files removed outside the app are never reconstructed from cached masks.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        if !self.valid() { return Err("Invalid pencil gallery.".into()); }
        let root = Self::imports_path(path);
        let mut next = self.clone();
        next.ensure_ids();
        next.pencils.retain(|p| self.files.get(&p.id).is_none_or(|file| regular_file(file)));
        let mut files = BTreeMap::new();
        let mut created = Vec::new();
        let result = (|| {
            for pencil in next.pencils.iter().filter(|p| p.settings.pencil_texture.is_some()) {
                std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                // Never use a brush name or imported ID as a filesystem path.
                let filename = format!("{}.pencil", super::new_pencil_id());
                let file = self.files.get(&pencil.id).filter(|p| p.parent() == Some(root.as_path())).cloned().unwrap_or_else(|| root.join(filename));
                let existed = file.exists();
                write(&file, pencil)?;
                if !existed { created.push(file.clone()); }
                files.insert(pencil.id.clone(), file);
            }
            let mut index = next.clone();
            index.storage_version = 2;
            index.order = next.pencils.iter().map(|p| p.id.clone()).collect();
            index.pencils.retain(|p| p.settings.pencil_texture.is_none());
            // Legacy custom-tool masks must not leak into an otherwise numeric preset.
            for p in &mut index.pencils { p.settings.brush_tip = None; }
            write(path, &index)
        })();
        if let Err(error) = result {
            for file in created { let _ = std::fs::remove_file(file); }
            return Err(error);
        }
        // Only remove known managed files, never the original ABRs or unknown files.
        for (id, file) in &self.files {
            if !files.contains_key(id) && file.parent() == Some(root.as_path()) && regular_file(file) {
                std::fs::remove_file(file).map_err(|e| format!("Cannot remove imported pencil: {e}"))?;
            }
        }
        next.files = files;
        next.storage_version = 2;
        *self = next;
        Ok(())
    }

    /// Detect only files beside this executable/index. No roaming profile or
    /// project-embedded brush is searched or added to the global gallery.
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut gallery: Self = if path.exists() { read(path)? } else { Self::default() };
        if !gallery.valid() || gallery.storage_version > 2 { return Err("Invalid pencil gallery.".into()); }
        let ids_changed=gallery.ensure_ids();
        let legacy = gallery.storage_version < 2 && gallery.pencils.iter().any(|p| p.settings.pencil_texture.is_some());
        if gallery.storage_version >= 2 {
            gallery.pencils.retain(|p| p.settings.pencil_texture.is_none());
        }
        let root = Self::imports_path(path);
        if root.exists() {
            let mut paths = Vec::new();
            for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_file()
                    && entry.path().extension().is_some_and(|s| s.eq_ignore_ascii_case("pencil")) {
                    paths.push(entry.path());
                    if paths.len() > 256 { return Err("imported_brushes contains more than 256 pencil files.".into()); }
                }
            }
            paths.sort();
            for file in paths {
                let result = (|| -> Result<(), String> {
                    let pencil: PencilPreset = read(&file)?;
                    if pencil.id.is_empty() || pencil.id == "standard" || pencil.settings.pencil_texture.is_none() {
                        return Err("Invalid imported pencil.".into());
                    }
                    if gallery.pencils.iter().any(|p| p.id == pencil.id) {
                        // A completed resource from an interrupted legacy migration.
                        if legacy { gallery.files.insert(pencil.id, file.clone()); return Ok(()); }
                        return Err("Duplicate pencil ID.".into());
                    }
                    gallery.pencils.push(pencil);
                    if !gallery.valid() { gallery.pencils.pop(); return Err("Invalid pencil or gallery size limit exceeded.".into()); }
                    gallery.files.insert(gallery.pencils.last().unwrap().id.clone(), file.clone());
                    Ok(())
                })();
                if let Err(e) = result { gallery.warnings.push(format!("Skipped {}: {e}", file.file_name().unwrap_or_default().to_string_lossy())); }
            }
        }
        gallery.pencils.sort_by_key(|p| gallery.order.iter().position(|id| id == &p.id).unwrap_or(usize::MAX));
        if legacy || ids_changed {
            // Commit the mask-free index only after every legacy pencil is saved.
            gallery.save(path)?;
        }
        Ok(gallery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{brush::BrushTip, pencil::ToolSettings};
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path=std::env::temp_dir().join(format!("graphite-import-storage-{}",crate::pencil_gallery::new_pencil_id()));
            std::fs::create_dir(&path).unwrap(); Self(path)
        }
        fn index(&self)->PathBuf {self.0.join("Pencils.gallery")}
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            assert_eq!(self.0.parent(),Some(std::env::temp_dir().as_path()));
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn pencils()->Gallery {
        let mut gallery=Gallery::default();
        for (name,mask) in [("Grain",vec![0,80,160,255]),("Soft",vec![20,60,150,230])] {
            let settings=ToolSettings {pencil_texture:Some(std::sync::Arc::new(BrushTip{name:name.into(),width:2,height:2,mask})),..Default::default()};
            gallery.add(name,"Imported",&settings).unwrap();
        }
        gallery
    }
    #[test]
    fn portable_brush_files_are_detected_without_a_gallery_and_absent_files_do_not_reappear() {
        let original=Directory::new();let copy=Directory::new();let mut gallery=pencils();
        gallery.save(&original.index()).unwrap();
        let index:Gallery=read(&original.index()).unwrap();
        assert!(index.pencils.is_empty(),"Index must not contain imported shapes");
        std::fs::copy(original.index(),copy.index()).unwrap();
        assert!(Gallery::load(&copy.index()).unwrap().pencils.is_empty(),"Index alone must not import brushes");
        std::fs::remove_file(copy.index()).unwrap();
        std::fs::create_dir(Gallery::imports_path(&copy.index())).unwrap();
        let source=gallery.files.values().next().unwrap();
        let destination=Gallery::imports_path(&copy.index()).join("Shared pencil.pencil");
        std::fs::copy(source,&destination).unwrap();
        let mut detected=Gallery::load(&copy.index()).unwrap();
        assert_eq!(detected.pencils.len(),1);
        assert_eq!(detected.pencils[0].folder,"Imported");
        assert_eq!(detected.pencils[0].settings.pencil_texture.as_ref().unwrap().mask,read::<PencilPreset>(source).unwrap().settings.pencil_texture.as_ref().unwrap().mask);
        std::fs::remove_file(&destination).unwrap();
        detected.save(&copy.index()).unwrap();
        assert!(detected.pencils.is_empty());assert!(!destination.exists());
        assert!(Gallery::load(&copy.index()).unwrap().pencils.is_empty());
    }
    #[test]
    fn legacy_gallery_migrates_once_and_removing_import_directory_is_respected() {
        let root=Directory::new();let gallery=pencils();
        write(&root.index(),&gallery).unwrap();
        let migrated=Gallery::load(&root.index()).unwrap();
        assert_eq!(migrated.pencils.len(),2);assert_eq!(migrated.pencils[0].id,gallery.pencils[0].id);
        assert_eq!(migrated.pencils[0].settings.flow,gallery.pencils[0].settings.flow);
        assert!(read::<Gallery>(&root.index()).unwrap().pencils.is_empty());
        for file in migrated.files.values(){std::fs::remove_file(file).unwrap();}
        std::fs::remove_dir(Gallery::imports_path(&root.index())).unwrap();
        assert!(Gallery::load(&root.index()).unwrap().pencils.is_empty());
        assert!(!Gallery::imports_path(&root.index()).exists());
    }
    #[test]
    fn removal_only_deletes_managed_brushes_and_bad_brush_does_not_hide_good_brushes() {
        let root=Directory::new();let mut gallery=pencils();gallery.save(&root.index()).unwrap();
        let folder=Gallery::imports_path(&root.index());
        std::fs::write(folder.join("Original.abr"),b"source ABR kept").unwrap();
        std::fs::write(folder.join("Invalid.pencil"),b"broken").unwrap();
        let mut loaded=Gallery::load(&root.index()).unwrap();
        assert_eq!(loaded.pencils.len(),2);assert_eq!(loaded.warnings.len(),1);
        let removed=loaded.pencils.remove(0);let removed_file=loaded.files[&removed.id].clone();
        loaded.save(&root.index()).unwrap();assert!(!removed_file.exists());
        assert_eq!(Gallery::load(&root.index()).unwrap().pencils.len(),1);
        loaded.delete_folder("Imported");loaded.save(&root.index()).unwrap();
        assert!(Gallery::load(&root.index()).unwrap().pencils.is_empty());
        assert_eq!(std::fs::read(folder.join("Original.abr")).unwrap(),b"source ABR kept");
        assert_eq!(std::fs::read(folder.join("Invalid.pencil")).unwrap(),b"broken");
    }
    #[test]
    fn failed_resource_save_preserves_the_existing_index() {
        let root=Directory::new();let mut plain=Gallery::default();plain.add("Native","Sketches",&ToolSettings::default()).unwrap();
        plain.save(&root.index()).unwrap();let before=std::fs::read(root.index()).unwrap();
        std::fs::write(Gallery::imports_path(&root.index()),b"not a directory").unwrap();
        assert!(pencils().save(&root.index()).is_err());
        assert_eq!(std::fs::read(root.index()).unwrap(),before);
    }
}
