//! Portable display preferences and optional recent-file history, separate from drawing material.
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccelerationMode {
    #[default]
    General,
    Radeon7900Xtx,
    IntelHd,
}

impl AccelerationMode {
    pub const ALL: [Self; 3] = [Self::General, Self::Radeon7900Xtx, Self::IntelHd];
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Radeon7900Xtx => "AMD Radeon 7900 XTX",
            Self::IntelHd => "Intel HD Graphics",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::General => "Balanced hardware selection, full display filtering, up to 60 canvas updates/s while drawing.",
            Self::Radeon7900Xtx => "Prefer a discrete AMD GPU and Vulkan, low frame latency, up to 120 canvas updates/s while drawing.",
            Self::IntelHd => "OpenGL compatibility for HD Graphics 2000. No display pyramid, window shadows or interface animations; up to 30 canvas updates/s while drawing.",
        }
    }
    pub fn update_interval(self) -> Duration {
        Duration::from_secs_f64(
            1. / match self {
                Self::General => 60.,
                Self::Radeon7900Xtx => 120.,
                Self::IntelHd => 30.,
            },
        )
    }
    pub fn renderer(self) -> eframe::Renderer {
        if self == Self::IntelHd {
            eframe::Renderer::Glow
        } else {
            eframe::Renderer::Wgpu
        }
    }
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Option<Self> {
        args.into_iter()
            .filter_map(|s| match s.as_str() {
                "--intel-hd" => Some(Self::IntelHd),
                "--general" => Some(Self::General),
                "--xtx" => Some(Self::Radeon7900Xtx),
                _ => None,
            })
            .last()
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Preferences {
    pub acceleration: AccelerationMode,
    #[serde(default)]
    pub recent_files: crate::recent_files::RecentFiles,
}
impl Preferences {
    // Portable builds keep display settings and the optional recent-file list beside the executable.
    pub fn path() -> Option<PathBuf> {
        Some(
            std::env::current_exe()
                .ok()?
                .parent()?
                .join("Graphite-Studio.settings.json"),
        )
    }
    pub fn load_from(path: &std::path::Path) -> Option<Self> {
        if std::fs::metadata(path).ok()?.len() > 2 * 1024 * 1024 {
            return None;
        }
        let mut preferences:Self=serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        preferences.recent_files.normalize();Some(preferences)
    }
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        use std::io::Write;
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result.map_err(|e| e.to_string())
    }
    pub fn startup() -> AccelerationMode {
        let requested = AccelerationMode::from_args(std::env::args())
            .or_else(|| {
                Self::path()
                    .and_then(|p| Self::load_from(&p))
                    .map(|p| p.acceleration)
            })
            .unwrap_or_default();
        compatible_mode(requested, legacy_intel_only())
    }
}

fn compatible_mode(requested: AccelerationMode, legacy_only: bool) -> AccelerationMode {
    // A preference copied from a modern machine must never lock a legacy PC out of the menu.
    if legacy_only {
        AccelerationMode::IntelHd
    } else {
        requested
    }
}

fn is_legacy_intel(id: &str) -> bool {
    let id = id.to_ascii_uppercase();
    id.contains("VEN_8086")
        && ["0102", "0106", "010A", "0112", "0116", "0122", "0126"]
            .iter()
            .any(|dev| id.contains(&format!("DEV_{dev}")))
}

#[cfg(windows)]
fn legacy_intel_only() -> bool {
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP,
    };
    let mut found = false;
    for index in 0..32 {
        let mut device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut device, 0) } == 0 {
            break;
        }
        if device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
            continue;
        }
        let end = device
            .DeviceID
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(device.DeviceID.len());
        let id = String::from_utf16_lossy(&device.DeviceID[..end]);
        // Ignore virtual display adapters (including EasyCanvas); a real modern GPU wins.
        if !id.to_ascii_uppercase().starts_with("PCI\\") {
            continue;
        }
        if !is_legacy_intel(&id) {
            return false;
        }
        found = true;
    }
    found
}
#[cfg(not(windows))]
fn legacy_intel_only() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_intel_and_startup_overrides() {
        assert!(is_legacy_intel(
            "PCI\\VEN_8086&DEV_0102&SUBSYS_21118086&REV_09"
        ));
        assert!(!is_legacy_intel("PCI\\VEN_8086&DEV_1912"));
        assert!(!is_legacy_intel("PCI\\VEN_1002&DEV_744C"));
        assert_eq!(
            AccelerationMode::from_args(["--xtx".into(), "--intel-hd".into()]),
            Some(AccelerationMode::IntelHd)
        );
        assert_eq!(AccelerationMode::IntelHd.renderer(), eframe::Renderer::Glow);
        for mode in AccelerationMode::ALL {
            assert_eq!(compatible_mode(mode, true), AccelerationMode::IntelHd);
            assert_eq!(compatible_mode(mode, false), mode);
        }
    }
    #[test]
    fn preferences_replace_and_reject_invalid_files() {
        let path = std::env::temp_dir().join(format!(
            "graphite-preferences-test-{}.json",
            std::process::id()
        ));
        for acceleration in AccelerationMode::ALL {
            Preferences { acceleration, ..Default::default() }.save_to(&path).unwrap();
            assert_eq!(
                Preferences::load_from(&path).unwrap().acceleration,
                acceleration
            );
        }
        std::fs::write(&path, b"{bad}").unwrap();
        assert!(Preferences::load_from(&path).is_none());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn recent_preferences_preserve_legacy_display_settings_and_disabled_state() {
        let path=std::env::temp_dir().join(format!("graphite-recent-preferences-{}.json",std::process::id()));
        std::fs::write(&path,br#"{"acceleration":"IntelHd"}"#).unwrap();
        let mut p=Preferences::load_from(&path).unwrap();
        assert_eq!(p.acceleration,AccelerationMode::IntelHd);assert_eq!(p.recent_files.maximum(),10);
        p.recent_files.remember(&std::env::temp_dir().join("Café drawing.psd"));p.save_to(&path).unwrap();
        let mut p=Preferences::load_from(&path).unwrap();assert_eq!(p.recent_files.files().len(),1);
        p.recent_files.set_maximum(0);p.save_to(&path).unwrap();
        let p=Preferences::load_from(&path).unwrap();assert_eq!(p.recent_files.maximum(),0);assert!(p.recent_files.files().is_empty());assert_eq!(p.acceleration,AccelerationMode::IntelHd);
        std::fs::remove_file(path).unwrap();
    }
}
