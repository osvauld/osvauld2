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
    // Nothing is guessed alone: only the checkpoints, one a second, are copied.
    assert!(stats.snapshots <= 3, "{stats:?}");
    assert_eq!(sim.messages, 0, "a one-player match sends nothing");
    assert!(!sim.apps[0].match_checks("dots").is_empty());
}

#[test]
fn every_second_paint_is_exactly_one_frame() {
    // 30 input frames a second painted at 60: the clock's rounding must never drop one.
    let mut sim = dots(vec![Link::LAN]);
    let dt = 1.0 / 60.0;
    sim.run(1.0, dt);
    let before = sim.apps[0].match_stats()["dots"].frames;
    for _ in 0..1440 {
        sim.run(dt * 0.5, dt); // one paint
    }
    no_errors(&sim);
    assert_eq!(sim.apps[0].match_stats()["dots"].frames - before, 720);
}

#[test]
fn on_a_lan_inputs_land_in_time_and_little_is_copied() {
    let mut sim = dots(vec![Link::LAN, Link::LAN]);
    sim.run(5.0, 1.0 / 60.0);
    no_errors(&sim);
    for app in &sim.apps {
        let stats = &app.match_stats()["dots"];
        assert!(stats.frames > 120, "{stats:?}");
        assert!(stats.snapshots * 4 < stats.frames, "a LAN should guess little: {stats:?}");
        assert_eq!(stats.desyncs, 0);
    }
}

#[test]
fn ping_is_the_round_trip_through_the_host() {
    // 25 ms each player to the host: 50 ms one way, 100 ms there and back, plus up to an input
    // frame (1/30 s here) before the answer rides on the next input.
    let mut sim = dots(vec![slow(25.0), slow(25.0)]);
    sim.run(3.0, 1.0 / 60.0);
    no_errors(&sim);
    for (i, app) in sim.apps.iter().enumerate() {
        let info = app.match_info("dots").expect("a match");
        let other = crate::net_sim::id_of(1 - i);
        let ping = info.ping.iter().find(|(p, _)| *p == other).map(|(_, ms)| *ms);
        assert!(ping.is_some_and(|ms| (95..=150).contains(&ms)), "{info:?}");
    }
}

#[test]
fn peers_that_start_apart_level_their_clocks() {
    // B presses start 150 ms after A, and a hello takes 100 ms: one of them starts a round trip
    // after the other, three frames apart at 30 a second. The one ahead slows until they level.
    let mut sim = Sim::new(&[("manifest.osv", MANIFEST), ("main.lua", MAIN)], vec![slow(50.0), slow(50.0)], 7);
    sim.click("script");
    sim.click_on(0, "start");
    sim.run(0.15, 1.0 / 60.0);
    sim.click_on(1, "start");
    sim.run(8.0, 1.0 / 60.0);
    no_errors(&sim);
    let frames: Vec<u64> = sim.apps.iter().map(|a| a.match_info("dots").expect("a match").frame).collect();
    assert!(frames[0].abs_diff(frames[1]) <= 1, "{frames:?}");
    let slowed: u64 = sim.apps.iter().map(|a| a.match_stats()["dots"].slowed).sum();
    assert!(slowed > 0, "the one ahead slowed: {:?}", sim.apps.iter().map(|a| a.match_stats()).collect::<Vec<_>>());
    for app in &sim.apps {
        assert_eq!(app.match_stats()["dots"].desyncs, 0);
    }
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

const HOCKEY_MANIFEST: &str = include_str!("../../../demo_apps/mp_hockey/manifest.osv");
const HOCKEY: &str = include_str!("../../../demo_apps/mp_hockey/main.lua");

fn hockey(links: Vec<Link>) -> Sim {
    let mut sim = Sim::new(&[("manifest.osv", HOCKEY_MANIFEST), ("main.lua", HOCKEY)], links, 3);
    sim.click("script");
    sim.click("start");
    sim
}

/// M6's Rust half: the rink rewinds with the state. A run that rolled back holds the same puck,
/// paddles and score at every checkpoint as a run that never did.
#[test]
fn a_driven_world_rewinds_with_the_match() {
    let mut straight = hockey(vec![slow(0.0), slow(0.0)]);
    straight.run(8.0, 1.0 / 60.0);
    let mut late = hockey(vec![Link::LAN, slow(60.0)]);
    late.run(8.0, 1.0 / 60.0);
    no_errors(&straight);
    no_errors(&late);
    let stats = &late.apps[0].match_stats()["hockey"];
    assert!(stats.rollbacks > 0, "{stats:?}");
    let reference = straight.apps[0].match_checks("hockey");
    assert!(reference.len() >= 6, "{reference:?}");
    let mut compared = 0;
    for sim in [&straight, &late] {
        for app in &sim.apps {
            assert_eq!(app.match_stats()["hockey"].desyncs, 0);
            for (f, h) in app.match_checks("hockey") {
                if let Some((_, want)) = reference.iter().find(|(g, _)| *g == f) {
                    assert_eq!(&h, want, "frame {f}");
                    compared += 1;
                }
            }
        }
    }
    assert!(compared >= 20, "{compared}");
    // Play happened: a goal (the puck is then back on the spot) or a puck off it.
    let score = text_of(&late.apps[0].view().info(), "score").unwrap_or_default();
    let puck = late.apps[0].inspect_worlds()["rink"].entities.iter().find(|e| e.id.starts_with("puck:")).map(|e| e.pos);
    let moved = puck.is_some_and(|p| (p.0 - 434.0).abs() > 1.0 || (p.1 - 234.0).abs() > 1.0);
    let paddles: Vec<_> = late.apps[0].inspect_worlds()["rink"].entities.iter().filter(|e| e.id.starts_with("paddle")).map(|e| (e.id.clone(), e.pos)).collect();
    assert!(moved || score != "0  :  0", "score {score:?}, puck {puck:?}, paddles {paddles:?}, console {:?}", late.apps[0].console(5));
}

fn one_app(main: &str) -> crate::LuaApp<crate::LuaMsg> {
    let sim = Sim::new(&[("manifest.osv", HOCKEY_MANIFEST), ("main.lua", main)], vec![Link::LAN], 1);
    sim.apps.into_iter().next().unwrap()
}

#[test]
fn a_driven_world_takes_no_entities_or_moments_in_the_view() {
    for (extra, says) in [
        ("{ id = 'x', pos = { 0, 0 }, drawing = gfx.drawing({ size = { 1, 1 }, parts = {} }) }", "entities come from the match"),
        ("on_hit = function() end", "no on_hit"),
        ("gravity = { 0, 10 }", "no gravity"),
    ] {
        let main = format!("return function() return ui.world({{ id = 'rink', driven = 'hockey', width = 10, height = 10, {extra} }}) end");
        let app = one_app(&main);
        let _ = app.view();
        let console = app.console(10).join("\n");
        assert!(console.contains(says), "{extra}: {console}");
    }
}

#[test]
fn a_driven_world_is_commanded_only_in_step() {
    let mut sim = hockey(vec![Link::LAN]);
    sim.run(0.5, 1.0 / 60.0);
    let main = HOCKEY.replace(
        "ui.button({ \"start\", id = \"start\", on_click = start }),",
        "ui.button({ \"start\", id = \"start\", on_click = start }),\n\t\t\tui.button({ \"poke\", id = \"poke\", on_click = function() world(\"rink\"):set(\"paddle:1\", { pos = { 0, 0 } }) end }),",
    );
    let mut sim = Sim::new(&[("manifest.osv", HOCKEY_MANIFEST), ("main.lua", main.as_str())], vec![Link::LAN], 1);
    sim.click("start");
    sim.run(0.5, 1.0 / 60.0);
    sim.click("poke");
    let console = sim.apps[0].console(10).join("\n");
    assert!(console.contains("driven by a match"), "{console}");
}

/// A setup bigger than a message (a game's maps and art) arrives in pieces and whole: both
/// peers play on it. mp_dots' channel is 512 bytes; this setup is 12 000.
#[test]
fn a_setup_bigger_than_a_message_arrives_whole() {
    let main = MAIN
        .replace("setup = { size = 400 },", "setup = { size = 400, blob = string.rep(\"ab\", 6000) },")
        .replace("local s = { size = setup.size, t = 0, dots = {} }", "local s = { size = setup.size, t = 0, dots = {}, blob = #setup.blob }");
    assert_ne!(main, MAIN);
    let mut sim = Sim::new(&[("manifest.osv", MANIFEST), ("main.lua", main.as_str())], vec![Link::WIFI, Link::WIFI], 5);
    sim.click("script");
    sim.click("start");
    sim.run(3.0, 1.0 / 60.0);
    no_errors(&sim);
    for app in &sim.apps {
        let stats = &app.match_stats()["dots"];
        assert!(stats.frames > 60, "{stats:?}");
        assert_eq!(stats.desyncs, 0);
    }
    let (a, b) = (sim.apps[0].match_checks("dots"), sim.apps[1].match_checks("dots"));
    assert!(!a.is_empty() && a.iter().any(|c| b.contains(c)), "{a:?} / {b:?}");
}

/// M9's Rust half: one of three runs other rules. The two who agree drop it and play on; it ends
/// its own match, knowing it is the odd one out.
#[test]
fn a_majority_drops_the_player_who_desyncs() {
    let impure = MAIN
        .replace("local SPEED = 2", "local SPEED = 2\nlocal calls = 0")
        .replace("s.t += 1", "calls += 1\n\t\t\ts.t = calls");
    let files = |main: &str| vec![("manifest.osv".to_string(), MANIFEST.to_string()), ("main.lua".to_string(), main.to_string())];
    let mut sim = Sim::new_each(vec![files(MAIN), files(&impure), files(MAIN)], vec![Link::LAN, slow(60.0), Link::LAN], 7);
    sim.click("script");
    sim.click("start");
    sim.run(8.0, 1.0 / 60.0);
    let info: Vec<_> = sim.apps.iter().map(|a| a.match_info("dots").unwrap()).collect();
    for i in [0, 2] {
        assert_eq!(info[i].status, "playing", "{info:?}");
        assert_eq!(info[i].desynced, ["p01"], "{info:?}");
    }
    assert_eq!(info[1].status, "ended", "{info:?}");
    // The two play on past the drop: frames keep coming.
    let before = sim.apps[0].match_stats()["dots"].frames;
    sim.run(2.0, 1.0 / 60.0);
    assert!(sim.apps[0].match_stats()["dots"].frames > before + 40);
}

#[test]
fn two_who_disagree_have_no_result() {
    let impure = MAIN
        .replace("local SPEED = 2", "local SPEED = 2\nlocal calls = 0")
        .replace("s.t += 1", "calls += 1\n\t\t\ts.t = calls");
    let files = |main: &str| vec![("manifest.osv".to_string(), MANIFEST.to_string()), ("main.lua".to_string(), main.to_string())];
    let mut sim = Sim::new_each(vec![files(MAIN), files(&impure)], vec![Link::LAN, slow(60.0)], 7);
    sim.click("script");
    sim.click("start");
    sim.run(8.0, 1.0 / 60.0);
    for app in &sim.apps {
        assert_eq!(app.match_info("dots").unwrap().status, "disputed");
    }
}

/// M10's Rust half: a device vanishes. The others wait, say for whom, and end with no result.
#[test]
fn a_player_gone_is_waited_for_then_the_match_ends() {
    let mut sim = dots(vec![Link::LAN, Link::LAN, Link::LAN]);
    sim.run(3.0, 1.0 / 60.0);
    sim.muted[2] = true;
    sim.run(2.0, 1.0 / 60.0);
    for i in [0, 1] {
        let info = sim.apps[i].match_info("dots").unwrap();
        assert_eq!(info.status, "waiting", "{info:?}");
        assert_eq!(info.waiting, ["p02"], "{info:?}");
    }
    sim.run(10.0, 1.0 / 60.0);
    for i in [0, 1] {
        let info = sim.apps[i].match_info("dots").unwrap();
        assert_eq!(info.status, "ended", "{info:?}");
        assert!(info.ended.as_deref().is_some_and(|e| e.contains("p02")), "{info:?}");
    }
}
