//! Small per-pencil preferences, separate from the gallery's sampled textures.
use crate::core::pencil::{PencilGrade, ToolSettings};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{BufReader, BufWriter, Write},
    path::Path,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PencilControls {
    pub grade: PencilGrade,
    pub core_diameter_mm: f32,
    pub geometry_scale: f32,
    pub width_response: f32,
    pub line_smoothing: f32,
    pub tip_sharpness: f32,
    pub color: [u8; 3],
    pub shade_variation: f32,
    pub flow: f32,
    pub mouse_pressure: f32,
    pub pressure_feel: f32,
    pub hold_to_straighten: bool,
    pub tilt: f32,
    pub azimuth: f32,
    pub auto_azimuth: bool,
    pub tip_rotation: f32,
}
impl PencilControls {
    pub fn capture(s: &ToolSettings, tip_rotation: f32) -> Self {
        Self {
            grade: s.grade,
            core_diameter_mm: s.pencil_core_diameter_mm,
            geometry_scale: s.pencil_geometry_scale,
            width_response: s.pressure_width,
            line_smoothing: s.line_smoothing,
            tip_sharpness: s.tip_sharpness,
            color: s.pencil_color_rgb,
            shade_variation: s.particle_variation,
            flow: s.flow,
            mouse_pressure: s.mouse_pressure,
            pressure_feel: s.pen_pressure_gamma,
            hold_to_straighten: s.hold_to_straighten,
            tilt: s.tilt_deg,
            azimuth: s.azimuth_deg,
            auto_azimuth: s.auto_azimuth,
            tip_rotation,
        }
    }
    pub fn apply(&self, s: &mut ToolSettings) {
        s.grade = self.grade;
        s.pencil_core_diameter_mm = self.core_diameter_mm;
        s.pencil_geometry_scale = self.geometry_scale;
        s.pressure_width = self.width_response;
        s.line_smoothing = self.line_smoothing;
        s.tip_sharpness = self.tip_sharpness;
        s.pencil_color_rgb = self.color;
        s.particle_variation = self.shade_variation;
        s.flow = self.flow;
        s.mouse_pressure = self.mouse_pressure;
        s.pen_pressure_gamma = self.pressure_feel;
        s.hold_to_straighten = self.hold_to_straighten;
        s.tilt_deg = self.tilt;
        s.azimuth_deg = self.azimuth;
        s.auto_azimuth = self.auto_azimuth;
    }
    fn valid(&self) -> bool {
        let mut s = ToolSettings::default();
        self.apply(&mut s);
        crate::project::settings_valid(&s)
            && (0. ..=1.).contains(&self.width_response)
            && (0. ..=1.).contains(&self.line_smoothing)
            && (0. ..=360.).contains(&self.tip_rotation)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct PencilSettings {
    pub pencils: BTreeMap<String, PencilControls>,
    pub last_pencil: Option<String>,
}
impl PencilSettings {
    fn valid(&self) -> bool {
        self.pencils.len() <= 4096
            && self
                .pencils
                .iter()
                .all(|(k, v)| !k.is_empty() && k.len() <= 128 && v.valid())
            && self.last_pencil.as_ref().is_none_or(|id| id.len() <= 128)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if !self.valid() {
            return Err("Invalid pencil settings.".into());
        }
        let temporary = path.with_extension(format!("settings.{}.tmp", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, self).map_err(|e| e.to_string())?;
            writer.flush().map_err(|e| e.to_string())?;
            writer.get_ref().sync_all().map_err(|e| e.to_string())?;
            drop(writer);
            std::fs::rename(&temporary, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
            return Err("Pencil settings file is too large.".into());
        }
        let result: Self =
            serde_json::from_reader(BufReader::new(file)).map_err(|e| e.to_string())?;
        if !result.valid() {
            return Err("Invalid pencil settings.".into());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_controls_round_trip_without_changing_other_tools_or_texture() {
        let mut source = ToolSettings::default();
        source.grade = PencilGrade::B4;
        source.pencil_core_diameter_mm = 3.2;
        source.pencil_geometry_scale = 1.2;
        source.pressure_width = 0.83;
        source.line_smoothing = 0.57;
        source.tip_sharpness = 0.74;
        source.pencil_color_rgb = [15, 120, 185];
        source.particle_variation = 0.19;
        source.flow = 1.4;
        source.mouse_pressure = 0.7;
        source.pen_pressure_gamma = 1.3;
        source.hold_to_straighten = false;
        source.tilt_deg = 60.;
        source.azimuth_deg = 120.;
        source.auto_azimuth = false;
        let control = PencilControls::capture(&source, 135.);
        let encoded = serde_json::to_vec(&control).unwrap();
        let restored: PencilControls = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(control, restored);
        let mut target = ToolSettings::default();
        target.use_wintab = true;
        target.eraser_strength = 0.42;
        target.pencil_texture = Some(std::sync::Arc::new(crate::core::brush::BrushTip {
            name: "Texture".into(),
            width: 1,
            height: 1,
            mask: vec![255],
        }));
        restored.apply(&mut target);
        assert_eq!(
            PencilControls::capture(&target, restored.tip_rotation),
            control
        );
        assert!(target.use_wintab);
        assert_eq!(target.eraser_strength, 0.42);
        assert!(target.pencil_texture.is_some());
        let mut bad = restored;
        bad.width_response = f32::NAN;
        assert!(!bad.valid());
    }
    #[test]
    fn legacy_gallery_ids_migrate_once_and_survive_rename_and_removal() {
        let mut gallery = crate::pencil_gallery::Gallery::default();
        for _ in 0..2 {
            gallery
                .add("Identical", "Folder", &ToolSettings::default())
                .unwrap();
        }
        for p in &mut gallery.pencils {
            p.id.clear();
        }
        assert!(gallery.ensure_ids());
        assert!(!gallery.ensure_ids());
        let id = gallery.pencils[1].id.clone();
        assert_ne!(gallery.pencils[0].id, id);
        gallery.pencils.remove(0);
        gallery.pencils[0].name = "Renamed".into();
        gallery.pencils[0].folder = "Moved".into();
        let restored: crate::pencil_gallery::Gallery =
            serde_json::from_slice(&serde_json::to_vec(&gallery).unwrap()).unwrap();
        assert_eq!(restored.pencils[0].id, id);
    }
}
