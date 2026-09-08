fn main() {
    let path = std::path::PathBuf::from(std::env::args_os().nth(1).expect("ABR path"));
    let bytes = std::fs::read(&path).unwrap();
    match graphite_studio::core::brush::import_abr(&bytes, path.file_stem().unwrap().to_str().unwrap()) {
        Ok(tips) => {
            println!("Imported {} tips; {} decoded bytes", tips.len(), tips.iter().map(|t| t.mask.len()).sum::<usize>());
            let mut gallery = graphite_studio::pencil_gallery::Gallery::default();
            for tip in &tips {
                let settings = graphite_studio::core::pencil::ToolSettings {
                    pencil_texture: Some(tip.clone()), ..Default::default()
                };
                gallery.add(&tip.name,"Imported collection",&settings).unwrap();
            }
            let saved = std::env::temp_dir().join(format!("graphite-abr-check-{}-{}.gallery",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            gallery.save(&saved).unwrap();
            let restored = graphite_studio::pencil_gallery::Gallery::load(&saved).unwrap();
            assert_eq!(tips.len(), restored.pencils.len());
            for (tip,preset) in tips.iter().zip(&restored.pencils) {
                let actual = preset.settings.pencil_texture.as_ref().unwrap();
                assert_eq!((&tip.name,tip.width,tip.height,&tip.mask),(&actual.name,actual.width,actual.height,&actual.mask));
                assert_eq!(preset.folder,"Imported collection");
            }
            std::fs::remove_file(saved).unwrap();
            println!("All textures and collection labels survived gallery save/reopen exactly.");
        }
        Err(error) => { eprintln!("Import failed: {error}"); std::process::exit(1); }
    }
}
