//! Shared helpers to turn licences into JSON and to format timestamps.

use serde_json::{json, Value};

use licensing::License;

use crate::now_secs;
use crate::visits::today;

/// A full JSON view of a licence, including its bound machines. Used by the
/// admin panel (every licence) and the account space (the viewer's licences).
pub fn license_json(license: &License) -> Value {
    let now = now_secs();
    json!({
        "key_prefix": license.key_prefix,
        "product": license.product,
        "plan": license.plan,
        "status": license.status(now).as_str(),
        "owner_id": license.owner_id,
        "note": license.note,
        "created_at": license.created_at,
        "created_label": human_date(license.created_at),
        "lifetime": license.is_lifetime(),
        "expires_at": license.expires_at,
        "expires_label": license.expires_at.map(human_date),
        "remaining_label": remaining_label(license, now),
        "duration_secs": license.duration_secs,
        "machines_allowed": license.max_activations,
        "machines_used": license.activations.len(),
        "revoked": license.revoked,
        "revoked_reason": license.revoked_reason,
        "issued_by": license.issued_by,
        "last_check": license.last_check,
        "last_check_label": license.last_check.map(human_date),
        "activations": license
            .activations
            .iter()
            .map(|activation| {
                json!({
                    "hwid": activation.hwid,
                    "hwid_short": short_hwid(&activation.hwid),
                    "first_seen": activation.first_seen,
                    "first_seen_label": human_date(activation.first_seen),
                    "last_seen": activation.last_seen,
                    "last_seen_label": human_date(activation.last_seen),
                    "checks": activation.checks
                })
            })
            .collect::<Vec<_>>(),
    })
}

fn remaining_label(license: &License, now: u64) -> String {
    if license.revoked {
        return "revoked".to_string();
    }
    match license.remaining(now) {
        None => "lifetime".to_string(),
        Some(0) => "expired".to_string(),
        Some(secs) => human_duration(secs),
    }
}

/// Format a UNIX timestamp as `YYYY-MM-DD HH:MM` in UTC.
pub fn human_date(secs: u64) -> String {
    let date = today(secs);
    let seconds_of_day = secs % 86_400;
    let hours = seconds_of_day / 3_600;
    let minutes = (seconds_of_day % 3_600) / 60;
    format!("{date} {hours:02}:{minutes:02} UTC")
}

/// A coarse human duration such as `42 days` or `5 hours`.
pub fn human_duration(secs: u64) -> String {
    let days = secs / 86_400;
    if days >= 1 {
        return format!("{days} day{}", plural(days));
    }
    let hours = secs / 3_600;
    if hours >= 1 {
        return format!("{hours} hour{}", plural(hours));
    }
    let minutes = secs / 60;
    format!("{minutes} minute{}", plural(minutes))
}

fn plural(value: u64) -> &'static str {
    if value == 1 {
        ""
    } else {
        "s"
    }
}

/// Show only the tail of a hardware id, keeping enough to tell machines apart.
fn short_hwid(hwid: &str) -> String {
    if hwid.len() <= 12 {
        hwid.to_string()
    } else {
        format!("…{}", &hwid[hwid.len() - 10..])
    }
}
