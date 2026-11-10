//! N apps in one process over a fake network (`jetpack-mp.md` §6 step 5, the player-cap sim).
//! Every message goes through a host, as the real transport does: A → host → B takes A's one-way
//! latency plus B's. Each player's link has a base latency, jitter, and a chance of a spike (a
//! lost packet the stream resends). A stream is ordered, so a pair's messages never overtake
//! each other. Time is virtual; only the cost of each frame is measured in real time.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use loro::{LoroDoc, LoroText};

use crate::net::NetPeer;
use crate::{DocKey, Key, LuaApp, LuaMsg, Shape};

/// One player's link to the host, one way.
#[derive(Debug, Clone, Copy)]
pub struct Link {
    pub base_ms: f64,
    pub jitter_ms: f64,
    /// Chance per message of a resend.
    pub spike: f64,
    pub spike_ms: f64,
}

impl Link {
    pub const LAN: Link = Link { base_ms: 1.0, jitter_ms: 1.0, spike: 0.0, spike_ms: 0.0 };
    pub const WIFI: Link = Link { base_ms: 15.0, jitter_ms: 10.0, spike: 0.01, spike_ms: 60.0 };
    pub const FOUR_G: Link = Link { base_ms: 40.0, jitter_ms: 25.0, spike: 0.02, spike_ms: 120.0 };
    pub const BAD: Link = Link { base_ms: 90.0, jitter_ms: 60.0, spike: 0.05, spike_ms: 250.0 };
}

struct InFlight {
    at: f64,
    to: usize,
    from: String,
    channel: String,
    body: Vec<u8>,
}

pub struct Sim {
    pub apps: Vec<LuaApp<LuaMsg>>,
    links: Vec<Link>,
    flight: Vec<InFlight>,
    pair_last: HashMap<(usize, usize), f64>,
    rng: u64,
    pub t: f64,
    /// Real milliseconds each app spent per render frame, in order.
    pub frame_ms: Vec<Vec<f64>>,
    pub messages: u64,
    /// A muted app is a device gone: it neither runs nor sends, and nothing reaches it.
    pub muted: Vec<bool>,
}

pub fn id_of(i: usize) -> String {
    format!("p{i:02}")
}

impl Sim {
    /// One app per link, from the same source files; each knows the others are here.
    pub fn new(files: &[(&str, &str)], links: Vec<Link>, seed: u64) -> Self {
        let each = vec![files.iter().map(|(p, b)| (p.to_string(), b.to_string())).collect(); links.len()];
        Self::new_each(each, links, seed)
    }

    /// As [`Self::new`], each app from its own files — one peer running modified rules.
    pub fn new_each(sources: Vec<Vec<(String, String)>>, links: Vec<Link>, seed: u64) -> Self {
        let n = links.len();
        let mut apps = Vec::new();
        for (i, files) in sources.iter().enumerate().take(n) {
            let src = LoroDoc::new();
            let map = src.get_map("files");
            for (path, body) in files {
                map.insert_container(path.as_str(), LoroText::new()).unwrap().insert(0, body).unwrap();
            }
            src.commit();
            let app = LuaApp::open(src, Rc::new(|_: &DocKey| Ok(None)), Arc::new(|| {}), Rc::new(|m| m))
                .expect("open");
            app.net().set_me(NetPeer { id: id_of(i), account: format!("did:{i}") });
            for j in (0..n).filter(|j| *j != i) {
                app.net().here(NetPeer { id: id_of(j), account: format!("did:{j}") });
            }
            let _ = app.view();
            apps.push(app);
        }
        Self {
            apps,
            links,
            flight: Vec::new(),
            pair_last: HashMap::new(),
            rng: seed.max(1),
            t: 0.0,
            frame_ms: vec![Vec::new(); n],
            messages: 0,
            muted: vec![false; n],
        }
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn one_way(&mut self, i: usize) -> f64 {
        let l = self.links[i];
        let mut ms = l.base_ms + l.jitter_ms * self.rand();
        if self.rand() < l.spike {
            ms += l.spike_ms;
        }
        ms / 1000.0
    }

    /// `on_click` on element `id`, on every app.
    pub fn click(&mut self, id: &str) {
        for app in &mut self.apps {
            let _ = app.view();
            app.update(LuaMsg::CallAt(Key::new(id, "on_click"), 0.0, 0.0, Shape::default(), None));
        }
    }

    /// `on_click` on element `id`, on app `i` only: a player who acts later than the others.
    pub fn click_on(&mut self, i: usize, id: &str) {
        let app = &mut self.apps[i];
        let _ = app.view();
        app.update(LuaMsg::CallAt(Key::new(id, "on_click"), 0.0, 0.0, Shape::default(), None));
    }

    /// `secs` of virtual time in render frames of `dt`.
    pub fn run(&mut self, secs: f64, dt: f64) {
        let end = self.t + secs;
        while self.t < end {
            self.t += dt;
            for i in 0..self.apps.len() {
                if self.muted[i] {
                    continue;
                }
                let started = Instant::now();
                self.apps[i].advance_simulation(self.t, true);
                self.frame_ms[i].push(started.elapsed().as_secs_f64() * 1000.0);
                let out = self.apps[i].net_take();
                for m in out {
                    for j in (0..self.apps.len()).filter(|j| *j != i) {
                        let at = self.t + self.one_way(i) + self.one_way(j);
                        let last = self.pair_last.entry((i, j)).or_insert(0.0);
                        let at = at.max(*last);
                        *last = at;
                        self.flight.push(InFlight { at, to: j, from: id_of(i), channel: m.channel.clone(), body: m.body.clone() });
                        self.messages += 1;
                    }
                }
            }
            let now = self.t;
            let (due, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.flight).into_iter().partition(|m| m.at <= now);
            self.flight = rest;
            let mut due = due;
            due.sort_by(|a, b| a.at.total_cmp(&b.at));
            for m in due.into_iter().filter(|m| !self.muted[m.to]) {
                self.apps[m.to].net_deliver(&m.channel, &m.from, &m.body);
            }
        }
    }
}
