use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn format_clock(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let secs_of_day = unix % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

pub fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{seconds}s");
    }

    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3600;
    let minutes = (seconds % 3600) / 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 && days == 0 {
        parts.push(format!("{minutes}m"));
    }

    parts.join(" ")
}

pub fn parse_duration(input: &str) -> Option<u64> {
    let cleaned = input.trim().to_ascii_lowercase();
    if cleaned.is_empty() {
        return None;
    }

    let mut total: u64 = 0;
    let mut current: u64 = 0;
    let mut has_digit = false;
    let mut has_unit = false;

    for ch in cleaned.chars() {
        match ch {
            '0'..='9' => {
                has_digit = true;
                current = current
                    .saturating_mul(10)
                    .saturating_add(ch as u64 - '0' as u64);
            }
            ' ' => {}
            unit => {
                if !has_digit {
                    return None;
                }
                let factor = match unit {
                    's' => 1,
                    'm' => 60,
                    'h' => 3_600,
                    'd' => 86_400,
                    'w' => 604_800,
                    _ => return None,
                };
                total = total.saturating_add(current.saturating_mul(factor));
                current = 0;
                has_digit = false;
                has_unit = true;
            }
        }
    }

    if has_digit {
        total = total.saturating_add(current.saturating_mul(60));
    } else if !has_unit {
        return None;
    }

    (total > 0).then_some(total)
}

pub fn truncate(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

pub fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn plural(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{count} {singular}")
    } else {
        format!("{count} {plural}")
    }
}
