//! `net.match` (`jetpack-mp.md` §3, step 5), on `demo_apps/mp_dots` over `net_sim`'s fake network.

use crate::net_sim::{Link, Sim};

const MANIFEST: &str = include_str!("../../../demo_apps/mp_dots/manifest.osv");
const MAIN: &str = include_str!("../../../demo_apps/mp_dots/main.lua");

fn dots(links: Vec<Link>) -> Sim {
    let mut sim = Sim::new(&[("manifest.osv", MANIFEST), ("main.lua", MAIN)], links, 7);
    sim.click("script");
    sim.click("start");
    sim
}

fn slow(ms: f64) -> Link {
    Link { base_ms: ms, jitter_ms: 0.0, spike: 0.0, spike_ms: 0.0 }
}

fn no_errors(sim: &Sim) {
    for app in &sim.apps {
        assert!(app.console(20).is_empty(), "{:?}", app.console(20));
    }
}

#[test]
fn a_one_player_match_runs_alone() {
    let mut sim = dots(vec![Link::LAN]);
    sim.run(2.0, 1.0 / 60.0);
    no_errors(&sim);
    let stats = &sim.apps[0].match_stats()["dots"];
    assert!((58..=61).contains(&stats.frames), "{stats:?}");
    assert_eq!(stats.rollbacks, 0);
    assert_eq!(sim.messages, 0, "a one-player match sends nothing");
    assert!(!sim.apps[0].match_checks("dots").is_empty());
}

#[test]
fn peers_agree_and_a_rolled_back_run_equals_a_straight_one() {
    let mut straight = dots(vec![slow(0.0), slow(0.0)]);
    straight.run(5.0, 1.0 / 60.0);
    let mut late = dots(vec![Link::LAN, slow(60.0)]);
    late.run(5.0, 1.0 / 60.0);
    no_errors(&straight);
    no_errors(&late);
    let stats = &late.apps[0].match_stats()["dots"];
    assert!(stats.rollbacks > 0, "120 ms each way should rewind: {stats:?}");
    let reference = straight.apps[0].match_checks("dots");
    assert!(reference.len() >= 3, "{reference:?}");
    for sim in [&straight, &late] {
        for app in &sim.apps {
            let checks = app.match_checks("dots");
            assert!(checks.len() >= 3, "{checks:?}");
            for (f, h) in &checks {
                if let Some((_, want)) = reference.iter().find(|(g, _)| g == f) {
                    assert_eq!(h, want, "frame {f}");
                }
            }
            assert_eq!(app.match_stats()["dots"].desyncs, 0);
        }
    }
}

#[test]
fn a_step_that_keeps_state_outside_state_is_caught() {
    let impure = MAIN
        .replace("local SPEED = 2", "local SPEED = 2\nlocal calls = 0")
        .replace("s.t += 1", "calls += 1\n\t\t\ts.t = calls");
    let files = |main: &str| vec![("manifest.osv".to_string(), MANIFEST.to_string()), ("main.lua".to_string(), main.to_string())];
    let mut sim = Sim::new_each(vec![files(&impure), files(MAIN)], vec![Link::LAN, slow(60.0)], 7);
    sim.click("script");
    sim.click("start");
    sim.run(5.0, 1.0 / 60.0);
    let caught: u64 = sim.apps.iter().map(|a| a.match_stats()["dots"].desyncs).sum();
    assert!(caught > 0, "{:?}", sim.apps.iter().map(|a| a.match_stats()).collect::<Vec<_>>());
}

fn text_of(info: &runtime::ElInfo, id: &str) -> Option<String> {
    if info.id.as_deref() == Some(id) {
        return info.text.clone();
    }
    info.children.iter().find_map(|c| text_of(c, id))
}

/// `(frame, what player 2's own dot reads)` for player 2, after each render frame.
fn own_dot_by_frame(sim: &mut Sim, secs: f64) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for _ in 0..(secs * 60.0) as usize {
        sim.run(1.0 / 60.0, 1.0 / 60.0);
        let info = sim.apps[1].view().info();
        if let (Some(status), Some(dot)) = (text_of(&info, "status"), text_of(&info, "dot:2")) {
            out.insert(status, dot);
        }
    }
    out
}

/// M5's Rust half: a slow link delays only the others' dots. Player 2's own dot reads the same at
/// every frame as on a link with no latency at all — its input is never predicted.
#[test]
fn rollback_hides_latency_for_my_own_dot() {
    let straight = own_dot_by_frame(&mut dots(vec![slow(0.0), slow(0.0)]), 5.0);
    let late = own_dot_by_frame(&mut dots(vec![Link::LAN, slow(60.0)]), 5.0);
    let mut compared = 0;
    for (status, dot) in &late {
        if let Some(want) = straight.get(status) {
            assert_eq!(dot, want, "at {status}");
            compared += 1;
        }
    }
    assert!(compared > 100, "only {compared} frames in common");
}
