use super::*;

#[test]
fn third_wrong_code_on_a_connection_closes_it() {
    let mut codes = Codes::default();
    assert_eq!(codes.wrong(1, 0), Verdict::Delay(WRONG_DELAY));
    assert_eq!(codes.wrong(1, 10), Verdict::Delay(WRONG_DELAY));
    assert_eq!(codes.wrong(1, 20), Verdict::Close);
    // Another connection starts from zero.
    assert_eq!(codes.wrong(2, 30), Verdict::Delay(WRONG_DELAY));
}

#[test]
fn thirty_wrong_codes_a_minute_shut_the_host_until_the_window_moves() {
    let mut codes = Codes::default();
    for i in 0..PER_MINUTE as u64 {
        assert!(codes.allowed(i));
        codes.wrong(i, i); // a fresh connection each, so none closes
    }
    assert!(!codes.allowed(PER_MINUTE as u64));
    assert!(!codes.allowed(MINUTE_MS - 1));
    // The first wrong code was at t=0; a minute after it, one slot frees.
    assert!(codes.allowed(MINUTE_MS));
}

#[test]
fn forgetting_a_connection_resets_its_count_only() {
    let mut codes = Codes::default();
    codes.wrong(1, 0);
    codes.wrong(1, 1);
    codes.forget(1);
    assert_eq!(codes.wrong(1, 2), Verdict::Delay(WRONG_DELAY));
    assert_eq!(codes.wrong_times_ms.len(), 3, "the per-minute window keeps every wrong code");
}
