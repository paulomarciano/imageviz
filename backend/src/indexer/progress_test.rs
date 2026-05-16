use super::*;

#[test]
fn test_progress_tracker_initial_state() {
    let tracker = ProgressTracker::new();
    let snap = tracker.snapshot();
    assert_eq!(snap.status, IndexStatus::Idle);
    assert_eq!(snap.total, 0);
    assert_eq!(snap.processed, 0);
    assert!(snap.errors.is_empty());
}

#[test]
fn test_progress_tracker_set_status() {
    let tracker = ProgressTracker::new();

    tracker.set_status(IndexStatus::Scanning);
    assert_eq!(tracker.snapshot().status, IndexStatus::Scanning);

    tracker.set_status(IndexStatus::Indexing);
    assert_eq!(tracker.snapshot().status, IndexStatus::Indexing);

    tracker.set_status(IndexStatus::Complete);
    assert_eq!(tracker.snapshot().status, IndexStatus::Complete);
}

#[test]
fn test_progress_tracker_totals() {
    let tracker = ProgressTracker::new();

    tracker.set_total(100);
    assert_eq!(tracker.snapshot().total, 100);

    for _ in 0..50 {
        tracker.increment_processed();
    }
    assert_eq!(tracker.snapshot().processed, 50);

    tracker.increment_processed();
    assert_eq!(tracker.snapshot().processed, 51);
}

#[test]
fn test_progress_tracker_errors() {
    let tracker = ProgressTracker::new();

    tracker.add_error("file1.png: Hash error".to_string());
    tracker.add_error("file2.jpg: Detection error".to_string());

    let snap = tracker.snapshot();
    assert_eq!(snap.errors.len(), 2);
    assert!(snap.errors[0].contains("file1.png"));
    assert!(snap.errors[1].contains("file2.jpg"));
}

#[test]
fn test_watch_channel_notifies_on_status_change() {
    let tracker = ProgressTracker::new();
    let rx = tracker.status_rx.clone();

    // Initial value should be Idle
    assert_eq!(*rx.borrow(), IndexStatus::Idle);

    tracker.set_status(IndexStatus::Scanning);
    assert_eq!(*rx.borrow(), IndexStatus::Scanning);

    tracker.set_status(IndexStatus::Complete);
    assert_eq!(*rx.borrow(), IndexStatus::Complete);
}

#[test]
fn test_default_is_idle() {
    let tracker = ProgressTracker::default();
    assert_eq!(tracker.snapshot().status, IndexStatus::Idle);
}
