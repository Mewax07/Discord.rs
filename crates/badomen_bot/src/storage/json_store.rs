use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{de::DeserializeOwned, Serialize};

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

        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        Self {
            path,
            data: Mutex::new(data),
        }
    }

    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.data.lock().unwrap())
    }

    pub fn write<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut guard = self.data.lock().unwrap();
        let result = f(&mut guard);
        if let Err(e) = self.persist(&guard) {
            eprintln!(
                "failed to write JSON store ({}): {e}",
                self.path.display()
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
