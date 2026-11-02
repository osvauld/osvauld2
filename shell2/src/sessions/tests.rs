//! The failover arithmetic (laptop-host §2 "Failover"): one host is the old retry ladder,
//! more hosts are tried in turn with no pause until the whole list has failed.

use net::wire::retry_after;

use super::{Failover, Redial};

fn at_once(index: usize) -> Failover {
    Failover { index, pause: None }
}

fn paused(index: usize, attempt: usize) -> Failover {
    Failover {
        index,
        pause: Some(retry_after(attempt)),
    }
}

#[test]
fn one_host_is_the_retry_ladder() {
    let mut r = Redial::default();
    assert_eq!(r.after_drop(0, 1), paused(0, 0));
    assert_eq!(r.after_drop(0, 1), paused(0, 1));
    assert_eq!(r.after_drop(0, 1), paused(0, 2));
    assert_eq!(r.after_drop(0, 1), paused(0, 3));
    assert_eq!(r.after_drop(0, 1), paused(0, 4));
    // The ladder's last rung repeats.
    assert_eq!(r.after_drop(0, 1), paused(0, 5));
    assert_eq!(r.round, 6);
}

#[test]
fn two_hosts_alternate_and_pause_only_on_the_wrap() {
    let mut r = Redial::default();
    assert_eq!(r.after_drop(0, 2), at_once(1));
    assert_eq!(r.round, 0);
    assert_eq!(r.after_drop(1, 2), paused(0, 0));
    assert_eq!(r.after_drop(0, 2), at_once(1));
    assert_eq!(r.after_drop(1, 2), paused(0, 1));
    assert_eq!(r.round, 2);
}

#[test]
fn three_hosts_each_get_a_turn_before_the_line() {
    let mut r = Redial::default();
    assert_eq!(r.after_drop(0, 3), at_once(1));
    assert_eq!(r.after_drop(1, 3), at_once(2));
    assert_eq!(r.after_drop(2, 3), paused(0, 0));
}

#[test]
fn a_session_that_came_up_resets_the_round() {
    let mut r = Redial::default();
    r.after_drop(0, 1);
    r.after_drop(0, 1);
    // The shell drops the entry on `SessionUp`; a fresh one starts the ladder over.
    let mut r = Redial::default();
    assert_eq!(r.after_drop(0, 1), paused(0, 0));
}

#[test]
fn a_list_that_shrank_wraps_rather_than_indexing_past_it() {
    let mut r = Redial::default();
    assert_eq!(r.after_drop(3, 2), at_once(0));
    assert_eq!(r.after_drop(0, 0), paused(0, 0));
}

#[test]
fn a_drop_after_a_session_on_the_last_host_tries_the_owner_at_once() {
    // The shell dropped the entry when the session on host 1 of 2 came up; its drop is the
    // first try of a new round, not the end of one — no pause, no offline line.
    let mut r = Redial::default();
    assert_eq!(r.after_drop(1, 2), at_once(0));
    assert_eq!(r.round, 0);
    assert_eq!(r.after_drop(0, 2), paused(0, 0));
}
