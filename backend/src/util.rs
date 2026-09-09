//! Small shared helpers used across modules.

use std::time::SystemTime;

/// Convert a `SystemTime` to an RFC 3339 / ISO 8601 string with sub-second
/// precision.
///
/// Times before the Unix epoch (where `duration_since` fails) fall back to the
/// epoch itself. This is the single timestamp formatter for the scanner walker
/// and the watcher pipeline, keeping DB timestamps and SSE payloads uniform.
pub fn system_time_to_iso(time: SystemTime) -> String {
    let duration = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let nsecs = duration.subsec_nanos();
    chrono::DateTime::from_timestamp(secs, nsecs).unwrap_or_default().to_rfc3339()
}

#[cfg(test)]
#[path = "util_test.rs"]
mod tests;
