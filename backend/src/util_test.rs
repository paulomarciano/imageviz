//! Golden tests pinning the exact output of `system_time_to_iso`.
//!
//! The watcher's SSE events and the DB `created_at`/`modified_at` columns
//! depend on this format; consumers would break on any drift.

use crate::util::system_time_to_iso;
use std::time::Duration;

#[test]
fn test_epoch_formats_to_iso() {
    let result = system_time_to_iso(std::time::UNIX_EPOCH);
    assert_eq!(result, "1970-01-01T00:00:00+00:00");
}

#[test]
fn test_known_timestamp_formats_to_iso() {
    let time = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let result = system_time_to_iso(time);
    assert_eq!(result, "2023-11-14T22:13:20+00:00");
}

#[test]
fn test_subsecond_precision_is_preserved() {
    let time = std::time::UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_789);
    let result = system_time_to_iso(time);
    assert_eq!(result, "2023-11-14T22:13:20.123456789+00:00");
}

#[test]
fn test_pre_epoch_time_falls_back_to_epoch() {
    // duration_since(UNIX_EPOCH) fails for pre-epoch times; the shared
    // formatter must fall back to the epoch (same behavior as the deleted
    // walker/handler copies).
    let time = std::time::UNIX_EPOCH - Duration::from_secs(1);
    let result = system_time_to_iso(time);
    assert_eq!(result, "1970-01-01T00:00:00+00:00");
}
