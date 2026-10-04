use super::*;

#[test]
fn sixty_hz_frames_plan_exactly_two_steps_without_long_term_drift() {
    let mut clock = FixedClock::default();
    assert_eq!(clock.advance(0.0, true).unwrap(), AdvanceReport::default());
    let mut steps = 0;
    for frame in 1..=1800 {
        let report = clock.advance(f64::from(frame) / 60.0, true).unwrap();
        assert_eq!(report.steps, 2, "frame {frame}, carry {}", clock.carry);
        assert_eq!(report.dropped_seconds, 0.0);
        steps += u64::from(report.steps);
    }
    assert_eq!(steps, 3600);
    assert_eq!(clock.dropped_seconds, 0.0);
}

#[test]
fn fractional_time_is_carried_and_repeated_time_never_steps() {
    let mut clock = FixedClock::default();
    clock.advance(0.0, true).unwrap();
    assert_eq!(clock.advance(STEP * 0.5, true).unwrap().steps, 0);
    let carry = clock.carry;
    assert_eq!(clock.advance(STEP * 0.5, true).unwrap().steps, 0);
    assert_eq!(clock.carry, carry);
    assert_eq!(clock.advance(STEP, true).unwrap().steps, 1);
    assert_eq!(clock.advance(STEP * 1.5, true).unwrap().steps, 0);
    assert_eq!(clock.advance(STEP * 2.0, true).unwrap().steps, 1);
}

#[test]
fn a_stall_is_bounded_and_reports_discarded_time_without_a_backlog() {
    let mut clock = FixedClock::default();
    clock.advance(0.0, true).unwrap();
    let report = clock.advance(1.0, true).unwrap();
    assert_eq!(report.steps, MAX_STEPS);
    assert!((report.dropped_seconds - (1.0 - STEP * 8.0)).abs() < 1e-12);
    assert_eq!(clock.dropped_seconds, report.dropped_seconds);
    assert_eq!(clock.advance(1.0, true).unwrap().steps, 0);
    assert_eq!(clock.advance(1.0 + STEP, true).unwrap().steps, 1);
}

#[test]
fn pause_and_resume_ignore_hidden_time_and_discard_fractional_carry() {
    let mut clock = FixedClock::default();
    clock.advance(0.0, true).unwrap();
    clock.advance(STEP * 0.5, true).unwrap();
    assert!(clock.carry > 0.0);
    assert_eq!(
        clock.advance(10.0, false).unwrap(),
        AdvanceReport::default()
    );
    assert_eq!(clock.carry, 0.0);
    assert_eq!(
        clock.advance(100.0, true).unwrap(),
        AdvanceReport::default()
    );
    assert_eq!(clock.advance(100.0 + STEP, true).unwrap().steps, 1);
    clock.pause();
    assert_eq!(
        clock.advance(200.0, true).unwrap(),
        AdvanceReport::default()
    );
    assert_eq!(clock.dropped_seconds, 0.0);
}

#[test]
fn invalid_time_does_not_change_the_clock_or_its_carry() {
    let mut clock = FixedClock::default();
    clock.advance(1.0, true).unwrap();
    clock.advance(1.0 + STEP * 0.5, true).unwrap();
    let before = (
        clock.last,
        clock.running,
        clock.carry,
        clock.dropped_seconds,
    );
    for elapsed in [f64::NAN, f64::INFINITY, -1.0, 0.5] {
        assert_eq!(clock.advance(elapsed, true), Err(ClockError));
        assert_eq!(
            (
                clock.last,
                clock.running,
                clock.carry,
                clock.dropped_seconds
            ),
            before
        );
        assert_eq!(clock.advance(elapsed, false), Err(ClockError));
        assert_eq!(
            (
                clock.last,
                clock.running,
                clock.carry,
                clock.dropped_seconds
            ),
            before
        );
    }
}

#[test]
fn huge_elapsed_time_cannot_create_an_unbounded_step_count() {
    let mut clock = FixedClock::default();
    clock.advance(0.0, true).unwrap();
    let report = clock.advance(f64::MAX, true).unwrap();
    assert_eq!(report.steps, MAX_STEPS);
    assert!(report.dropped_seconds.is_finite());
    assert_eq!(clock.advance(f64::MAX, true).unwrap().steps, 0);
}
