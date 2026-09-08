//! Most-recently-used successful opens/saves, kept in portable app preferences.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentFiles {
    maximum: u8,
    files: Vec<PathBuf>,
}
impl Default for RecentFiles {
    fn default() -> Self {
        Self {
            maximum: 10,
            files: vec![],
        }
    }
}
impl RecentFiles {
    pub fn maximum(&self) -> u8 {
        self.maximum
    }
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
    pub fn set_maximum(&mut self, maximum: u8) {
        self.maximum = maximum.min(10);
        self.normalize();
    }
    pub fn normalize(&mut self) {
        self.maximum = self.maximum.min(10);
        let mut seen = std::collections::HashSet::new();
        self.files
            .retain(|path| path.is_absolute() && supported(path) && seen.insert(key(path)));
        self.files.truncate(self.maximum as usize);
    }
    pub fn remember(&mut self, path: &Path) -> bool {
        if self.maximum == 0 || !supported(path) {
            return false;
        }
        let Some(path) = std::fs::canonicalize(path)
            .ok()
            .or_else(|| std::path::absolute(path).ok())
        else {
            return false;
        };
        let key = key(&path);
        if self.files.first().is_some_and(|p| self::key(p) == key) {
            return false;
        }
        self.files.retain(|p| self::key(p) != key);
        self.files.insert(0, path);
        self.normalize();
        true
    }
}
fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| crate::import::OPEN_EXTENSIONS.contains(&v.to_ascii_lowercase().as_str()))
}
/// Hide Windows' extended-path prefix in the menu while retaining it for reopening.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        if let Some(disk) = text.strip_prefix(r"\\?\") {
            return disk.to_string();
        }
    }
    text.into_owned()
}
fn key(path: &Path) -> String {
    let text = display_path(path);
    #[cfg(windows)]
    {
        text.replace('/', r"\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn most_recent_first_deduplicates_truncates_and_disables() {
        let root = std::env::temp_dir();
        let mut recent = RecentFiles::default();
        for i in 0..12 {
            recent.remember(&root.join(format!("image-{i}.png")));
        }
        assert_eq!(recent.files.len(), 10);
        assert!(recent.files[0].ends_with("image-11.png"));
        recent.remember(&root.join("image-5.png"));
        assert!(recent.files[0].ends_with("image-5.png"));
        assert_eq!(recent.files.len(), 10);
        assert!(!recent.remember(&root.join("image-5.png")));
        recent.set_maximum(3);
        assert_eq!(recent.files.len(), 3);
        recent.set_maximum(0);
        assert!(recent.files.is_empty());
        assert!(!recent.remember(&root.join("disabled.png")));
        recent.set_maximum(255);
        assert_eq!(recent.maximum(), 10);
        assert!(recent.files.is_empty());
        assert!(!recent.remember(&root.join("unsupported.gif")));
    }
    #[cfg(windows)]
    #[test]
    fn windows_paths_ignore_case_and_verbatim_prefix_for_duplicates() {
        let mut recent:RecentFiles=serde_json::from_value(serde_json::json!({"maximum":255,"files":[r"C:\Art\Café.PNG",r"\\?\c:\art\café.png","relative.png"]})).unwrap();
        recent.normalize();
        assert_eq!(recent.maximum(), 10);
        assert_eq!(recent.files.len(), 1);
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\art.psd")),
            r"\\server\share\art.psd"
        );
    }
}
