use super::*;

#[test]
fn fps_uses_completed_presentations_not_virtual_time_or_work_cost() {
    let start = Instant::now();
    let mut monitor = Monitor::new("windowed", (900.0, 700.0), 1.0, "test adapter".into());
    for i in 0..=120 {
        monitor.record(
            start + Duration::from_secs_f64(i as f64 / 60.0),
            2.0,
            true,
            false,
        );
    }
    let snapshot = monitor.snapshot();
    assert!((snapshot.fps_at(start + Duration::from_secs(2)).unwrap() - 60.0).abs() < 0.01);
    assert_eq!(snapshot.work_p95_ms, 2.0);
    assert_eq!(snapshot.presented_frames, 121);
    assert!(snapshot.fps_at(start + Duration::from_secs(4)).is_none());
    let before = monitor.snapshot();
    monitor.record(start + Duration::from_secs(3), 999.0, true, true);
    assert_eq!(monitor.snapshot().normal_frames, before.normal_frames);
}

#[test]
fn history_is_bounded_and_offscreen_work_never_becomes_display_fps() {
    let start = Instant::now();
    let mut monitor = Monitor::new("offscreen", (900.0, 700.0), 1.0, "test adapter".into());
    for i in 0..1000 {
        monitor.record(
            start + Duration::from_millis(i * 10),
            if i % 10 == 0 { 20.0 } else { 4.0 },
            false,
            false,
        );
    }
    let snapshot = monitor.snapshot();
    assert_eq!(monitor.costs.len(), 240);
    assert_eq!(snapshot.samples, 240);
    assert_eq!(snapshot.work_p50_ms, 4.0);
    assert_eq!(snapshot.work_p95_ms, 20.0);
    assert_eq!(snapshot.over_budget, 24);
    assert!(snapshot.fps_at(start + Duration::from_secs(10)).is_none());
    assert_eq!(snapshot.presented_frames, 0);
    assert!(snapshot.label.contains("offscreen"));
}
