use std::time::{SystemTime, UNIX_EPOCH};

pub fn epoch_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// "hh:mm:ss" UTC.
pub fn hms(epoch: f64) -> String {
    let s = epoch.max(0.0) as u64 % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Compact age like "12s", "5m", "2h".
pub fn age(since_epoch: f64) -> String {
    let d = (epoch_secs() - since_epoch).max(0.0);
    if d < 60.0 {
        format!("{:.0}s", d)
    } else if d < 3600.0 {
        format!("{:.0}m", d / 60.0)
    } else {
        format!("{:.0}h", d / 3600.0)
    }
}
