use super::*;

#[test]
fn a_beat_is_stuck_only_while_busy_past_the_limit() {
    let beat = Beat::default();
    let limit = Duration::from_millis(30);
    assert!(!beat.stuck_after(limit), "idle is never stuck");
    beat.start();
    assert!(!beat.stuck_after(limit));
    std::thread::sleep(limit * 2);
    assert!(beat.stuck_after(limit));
    beat.flagged.store(true, Ordering::SeqCst);
    assert!(beat.end(), "the flagged thread's idle tells the shell");
    assert!(!beat.stuck_after(limit));
    assert!(!beat.end(), "told once");
}
