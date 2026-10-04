use super::*;
use std::time::Duration;

#[test]
fn phase_measurements_are_deltas_not_cumulative() {
    let start = Instant::now();
    let mut timing = FrameTiming::at(Some(start));
    timing.mark_at(0, start + Duration::from_millis(3));
    timing.mark_at(1, start + Duration::from_millis(8));
    timing.mark_at(2, start + Duration::from_millis(10));
    assert_eq!(&timing.phases[..3], &[3.0, 5.0, 2.0]);
}

#[test]
fn disabled_measurement_stays_disabled() {
    let mut timing = FrameTiming::at(None);
    timing.mark(0);
    timing.mark_at(1, Instant::now());
    assert!(timing.start.is_none() && timing.last.is_none());
    assert_eq!(timing.phases, [0.0; 6]);
}
