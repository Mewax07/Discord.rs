use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{de::DeserializeOwned, Serialize};

use crate::logs;

pub struct JsonStore<T> {
    path: PathBuf,
    data: Mutex<T>,
}

impl<T: Serialize + DeserializeOwned + Default> JsonStore<T> {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).expect("Unable to create the storage folder.");
            }
        }

        let data = Self::load(&path);

        Self {
            path,
            data: Mutex::new(data),
        }
    }

    fn load(path: &Path) -> T {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    logs::error(
                        "storage",
                        format!("unable to read {} ({e}), starting empty", path.display()),
                    );
                }
                return T::default();
            }
        };

        match serde_json::from_str(&raw) {
            Ok(data) => data,
            Err(e) => {
                let backup = path.with_extension("invalid");
                let kept = fs::write(&backup, &raw).is_ok();
                logs::error(
                    "storage",
                    format!(
                        "{} could not be parsed ({e}), starting empty{}",
                        path.display(),
                        if kept {
                            format!(", the original was copied to {}", backup.display())
                        } else {
                            String::new()
                        }
                    ),
                );
                T::default()
            }
        }
    }

    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.data.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn write<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut guard = self.data.lock().unwrap_or_else(|e| e.into_inner());
        let result = f(&mut guard);
        if let Err(e) = self.persist(&guard) {
            logs::error(
                "storage",
                format!("unable to save {} ({e})", self.path.display()),
            );
        }
        result
    }

    fn persist(&self, data: &T) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(data).expect("fail-safe serialization");
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)
    }
}
