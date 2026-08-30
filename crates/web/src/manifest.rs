use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    pub file: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub changelog_url: Option<String>,
    #[serde(default)]
    pub hidden: bool,
}

impl DownloadItem {
    pub fn is_safe(&self) -> bool {
        !self.id.is_empty()
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && !self.file.is_empty()
            && !self.file.contains("..")
            && !self.file.contains('/')
            && !self.file.contains('\\')
            && !self.file.contains(':')
    }

    pub fn resolve(&self, root: &Path) -> Option<PathBuf> {
        if !self.is_safe() {
            return None;
        }

        let candidate = root.join(&self.file);
        let real_root = fs::canonicalize(root).ok()?;
        let real_file = fs::canonicalize(&candidate).ok()?;

        real_file
            .starts_with(&real_root)
            .then_some(real_file)
            .filter(|path| path.is_file())
    }

    pub fn size(&self, root: &Path) -> Option<u64> {
        let path = self.resolve(root)?;
        fs::metadata(path).ok().map(|meta| meta.len())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub items: Vec<DownloadItem>,
}

impl Manifest {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn visible(&self) -> Vec<&DownloadItem> {
        self.items
            .iter()
            .filter(|item| !item.hidden && item.is_safe())
            .collect()
    }

    pub fn find(&self, id: &str) -> Option<&DownloadItem> {
        self.items
            .iter()
            .find(|item| item.id == id && item.is_safe())
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
