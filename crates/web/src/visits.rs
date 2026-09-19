//! Lightweight visit counters persisted to a JSON file.
//!
//! No per-visitor tracking and no IP storage: only aggregate totals, per-page
//! counts, per-download counts, and a per-day breakdown. Writes are best effort
//! and never block a response for long (a short mutex, then an atomic rename).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::now_secs;

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct VisitData {
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub pages: BTreeMap<String, u64>,
    #[serde(default)]
    pub downloads: BTreeMap<String, u64>,
    #[serde(default)]
    pub days: BTreeMap<String, u64>,
    #[serde(default)]
    pub first_seen: u64,
    #[serde(default)]
    pub last_seen: u64,
}

pub struct VisitStore {
    path: PathBuf,
    data: Mutex<VisitData>,
}

impl VisitStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
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

    /// Count a page view for `label` (a small, fixed set of labels).
    pub fn record_page(&self, label: &str) {
        self.bump(|data| {
            *data.pages.entry(label.to_string()).or_insert(0) += 1;
        });
    }

    /// Count a download for a manifest item id.
    pub fn record_download(&self, id: &str) {
        self.bump(|data| {
            *data.downloads.entry(id.to_string()).or_insert(0) += 1;
        });
    }

    fn bump(&self, f: impl FnOnce(&mut VisitData)) {
        let now = now_secs();
        let mut guard = self.data.lock().unwrap_or_else(|e| e.into_inner());
        if guard.first_seen == 0 {
            guard.first_seen = now;
        }
        guard.last_seen = now;
        guard.total += 1;
        *guard.days.entry(today(now)).or_insert(0) += 1;
        // Keep the per-day map bounded to the most recent stretch of activity.
        while guard.days.len() > 400 {
            if let Some(oldest) = guard.days.keys().next().cloned() {
                guard.days.remove(&oldest);
            } else {
                break;
            }
        }
        f(&mut guard);
        if let Err(e) = self.persist(&guard) {
            eprintln!("visit store write failed ({}): {e}", self.path.display());
        }
    }

    pub fn snapshot(&self) -> VisitData {
        self.data.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn persist(&self, data: &VisitData) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string());
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)
    }
}

/// Format a UNIX timestamp as `YYYY-MM-DD` in UTC without pulling in a date crate.
pub fn today(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

// Howard Hinnant's civil-from-days algorithm (days since 1970-01-01).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
