use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::model::License;

#[derive(Default, Serialize, Deserialize)]
pub struct LicenseData {
    #[serde(default)]
    pub licenses: HashMap<String, License>,
}

pub struct LicenseStore {
    path: PathBuf,
    data: Mutex<LicenseData>,
}

impl LicenseStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).expect("unable to create the licence folder");
            }
        }

        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        Self {
            path,
            data: Mutex::new(data),
        }
    }

    pub fn read<R>(&self, f: impl FnOnce(&LicenseData) -> R) -> R {
        f(&self.data.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn write<R>(&self, f: impl FnOnce(&mut LicenseData) -> R) -> R {
        let mut guard = self.data.lock().unwrap_or_else(|e| e.into_inner());
        let result = f(&mut guard);
        if let Err(e) = self.persist(&guard) {
            eprintln!("licence store write failed ({}): {e}", self.path.display());
        }
        result
    }

    fn persist(&self, data: &LicenseData) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(data).expect("licence serialization");
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)
    }
}
