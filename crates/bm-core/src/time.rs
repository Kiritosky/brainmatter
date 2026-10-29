use std::time::{SystemTime, UNIX_EPOCH};

/// Unix epoch milliseconds, the timestamp unit used throughout the database.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
