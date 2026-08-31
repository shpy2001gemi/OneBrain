//! Shared helper / formatting utilities for CLI output.

/// Format an epoch timestamp as a human-readable relative time string.
pub(crate) fn format_timestamp(epoch_secs: u64) -> String {
    if epoch_secs == 0 {
        return "--".to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.saturating_sub(epoch_secs);
    if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

/// Format a Duration as a human-readable relative time string (e.g. "5m ago").
pub(crate) fn format_elapsed(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Format milliOBT into a human-readable OBT string with proper grouping.
///
/// Fixed: correctly handles values > 999,999 milliOBT by recursively
/// grouping the integer part (e.g. 1,000,000,000 → "1,000,000.000 OBT").
///
/// Note: Currently unused — `format_obt_short` is preferred for compact output.
/// Kept for future verbose/detailed display (e.g. `wallet --verbose`).
#[allow(dead_code)]
pub(crate) fn format_obt(milliobt: u64) -> String {
    let whole = milliobt / 1000;
    let frac = milliobt % 1000;
    format!("{}.{:03} OBT", group_thousands(whole), frac)
}

/// Insert comma separators into a u64 for display (e.g. 1234567 → "1,234,567").
fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

/// Format milliOBT into a shorter form (no leading group).
pub(crate) fn format_obt_short(milliobt: u64) -> String {
    let whole = milliobt / 1000;
    let frac = milliobt % 1000;
    format!("{}.{:03} OBT", whole, frac)
}

/// Format a signed milliOBT amount (for transaction history).
pub(crate) fn format_obt_signed(milliobt: i64) -> String {
    let sign = if milliobt >= 0 { "+" } else { "-" };
    let abs = milliobt.unsigned_abs();
    let whole = abs / 1000;
    let frac = abs % 1000;
    format!("{}{}.{:03} OBT", sign, whole, frac)
}

/// Generate a simple horizontal bar chart string.
pub(crate) fn bar_chart(value: u64, max: u64, width: usize) -> String {
    let filled = if max > 0 {
        (value as f64 / max as f64 * width as f64) as usize
    } else {
        0
    };
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Truncate CID hex to a short display form.
pub(crate) fn short_cid(cid_hex: &str) -> &str {
    if cid_hex.len() >= 8 {
        &cid_hex[..8]
    } else {
        cid_hex
    }
}

/// Generate a timestamp string for backup filenames.
///
/// Fixed: uses YYYYMMDD_HHMMSS format instead of raw epoch seconds.
pub(crate) fn chrono_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert epoch seconds to date/time components (UTC).
    // Using manual calculation to avoid pulling in the `chrono` crate.
    let secs_per_minute = 60u64;
    let secs_per_hour = 3600u64;
    let secs_per_day = 86400u64;

    let total_days = now / secs_per_day;
    let remaining = now % secs_per_day;
    let hour = remaining / secs_per_hour;
    let minute = (remaining % secs_per_hour) / secs_per_minute;
    let second = remaining % secs_per_minute;

    // Days since 1970-01-01 → calendar date (Gregorian).
    let (year, month, day) = days_to_ymd(total_days);

    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        year, month, day, hour, minute, second
    )
}

/// Convert day-count since Unix epoch to (year, month, day).
fn days_to_ymd(total_days: u64) -> (u64, u64, u64) {
    // Algorithm adapted from Howard Hinnant's `civil_from_days`.
    let z = total_days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as u64, m, d)
}

/// Format bytes as human-readable size.
pub(crate) fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Truncate a string for display (UTF-8 safe).
pub(crate) fn truncate_str(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Count total items (nodes or edges) in a neighbor tree recursively.
///
/// Replaces the identical `count_nodes` and `count_edges` helpers.
pub(crate) fn count_graph_items(neighbors: &[onebrain_node::types::NeighborInfo]) -> usize {
    let mut count = neighbors.len();
    for n in neighbors {
        count += count_graph_items(&n.children);
    }
    count
}
