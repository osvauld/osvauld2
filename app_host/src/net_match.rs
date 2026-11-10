//! `net.match` — the platform's lockstep loop with rollback (`jetpack-mp.md` §3). A game
//! describes its state, its local input and its rules; every peer runs the same rules on the same
//! inputs and so holds the same state. The platform sends the local input, predicts the ones not
//! yet here (the last one again), keeps a snapshot of `state` before every frame run on a guess,
//! rewinds to the earliest frame a late input corrected and runs `step` forward again, and every second
//! hashes a state all inputs are in for and compares it with the others'.
//!
//! A snapshot is `state` in the wire form of `net` values (`net::encode`): copying it and checking
//! it is plain data are the same act, and its bytes are what the hash reads. `step` may change
//! nothing but `state` — the rule cannot be enforced, so a break shows up as a desync.
//!
//! Matches are per VM: their functions and their state table are the VM's. A reload starts over.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use mlua::{Error, Function, Lua, Table, UserData, UserDataFields, Value};
use sha2::{Digest, Sha256};

use crate::net::{Net, Outgoing, decode_body, encode};

/// Seconds between reading an input and running it, rounded up to whole input frames (at least
/// one): the network's head start, so a nearby peer's input is usually in before its frame runs
/// and nothing is guessed. A one-player match has no network to wait for and runs its input on
/// the frame it was read.
pub const DELAY: f64 = 0.016;
/// Frames the local simulation may run past the last frame every input is in for; past it the
/// match stalls until the slowest peer's input arrives.
pub const WINDOW: u64 = 8;
/// Most a peer that is ahead slows its clock to let the others level with it.
const MOST_SLOW: f64 = 0.1;
/// Input frames run at most per render frame, so a stall's backlog is caught up gradually.
const CATCH_UP: u32 = 8;
/// Seconds stalled on a player before the match says it is waiting for them, and before it ends.
const WAITING: f64 = 1.0;
const DROP_OUT: f64 = 10.0;
/// Checkpoints after a disagreement before its losers' inputs stop counting: every honest peer
/// drops them from the same frame, and none may run past it before the verdict is in.
const DROP_AFTER: u64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Lobby,
    Playing,
    Ended,
    /// Players disagreed and no majority agreed: no result can stand.
    Disputed,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Lobby => "lobby",
            Status::Playing => "playing",
            Status::Ended => "ended",
            Status::Disputed => "disputed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub status: &'static str,
    pub desynced: Vec<String>,
    pub waiting: Vec<String>,
    pub ended: Option<String>,
    /// The next input frame to run.
    pub frame: u64,
    /// Round trip to each other player measured so far, in milliseconds.
    pub ping: Vec<(String, u64)>,
}

/// What a match did, for the player-cap sim and the tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    /// Input frames run forward for the first time.
    pub frames: u64,
    /// Rewinds, and the input frames they ran again.
    pub rollbacks: u64,
    pub rolled_frames: u64,
    /// Render frames on which the match could not advance: an input was too late.
    pub stalls: u64,
    pub steps: u64,
    /// States copied for a rewind or a checkpoint.
    pub snapshots: u64,
    /// Checkpoints where a peer's hash differed from ours.
    pub desyncs: u64,
    /// Render frames on which the clock ran slow, being ahead of a peer.
    pub slowed: u64,
}

struct Core {
    id: String,
    ticks: u32,
    frame_dt: f64,
    players: Vec<String>,
    me: usize,
    players_hash: [u8; 32],
    status: Status,
    hello: BTreeSet<usize>,
    setup: Option<Vec<u8>>,
    /// The starter's setup as it arrives, in pieces that each fit a message: offset → bytes.
    parts: BTreeMap<u64, Vec<u8>>,
    /// The most setup bytes one hello carries.
    chunk: usize,
    delay: u64,
    state: Option<Table>,
    /// The next input frame to run.
    frame: u64,
    /// Inputs in, per player by frame (encoded).
    inputs: Vec<BTreeMap<u64, Vec<u8>>>,
    /// Per player: the first frame whose input is not yet in (all before it are).
    known: Vec<u64>,
    /// What each simulated frame ran on, per player — the predictions to check late inputs by.
    used: BTreeMap<u64, Vec<Vec<u8>>>,
    /// Everything before each frame still open to a rewind.
    snapshots: BTreeMap<u64, Rc<Snap>>,
    /// What the driven worlds reported on the last tick, for the next `step`.
    pending: Vec<(String, world::WorldEvent)>,
    /// The earliest frame a late input proved mispredicted.
    rewind: Option<u64>,
    every: u64,
    checked: u64,
    checks: Vec<(u64, String)>,
    /// Each checkpoint's hashes by player, ours included, until its verdict.
    hashes: BTreeMap<u64, BTreeMap<usize, String>>,
    /// The last checkpoint with a verdict.
    settled: u64,
    desynced: BTreeSet<usize>,
    /// Players out of the match, and the frame from which their inputs no longer count.
    drop_from: BTreeMap<usize, u64>,
    /// Seconds the match has been stalled on a late input, and on whom.
    stalled_for: f64,
    waiting_for: Vec<usize>,
    ended: Option<String>,
    clock: Option<f64>,
    carry: f64,
    since_hello: f64,
    /// When each of our inputs was sent, by frame, until every peer has it: the round trip is
    /// from here to the message that says it arrived.
    sent_at: BTreeMap<u64, f64>,
    /// Per player: the `known` they last told us for us, and the round trip, smoothed (seconds).
    acked: Vec<u64>,
    ping: Vec<Option<f64>>,
    /// Per player: how many frames ahead of us they last said they were.
    their_lead: Vec<Option<i64>>,
    stats: Stats,
    error: Option<String>,
    error_told: bool,
}

/// A match before frame `f`: `state` in wire form, each driven world, the events waiting for
/// the next `step`, and — on a checkpoint frame — the worlds as bytes for the hash.
struct Snap {
    state: Vec<u8>,
    worlds: Vec<world::WorldSnapshot>,
    pending: Vec<(String, world::WorldEvent)>,
    digest: Vec<u8>,
}

/// One match: its core, and its functions (outside the core so no borrow is held while Lua runs).
#[derive(Clone)]
pub struct Match {
    core: Rc<RefCell<Core>>,
    state_fn: Function,
    input_fn: Function,
    step_fn: Function,
    /// Driven worlds by id, in id order, each with the function describing it from `state`.
    worlds: Rc<Vec<(String, Function)>>,
    world_map: crate::Worlds,
    steps_per_tick: u32,
}

pub type Matches = Rc<RefCell<HashMap<String, Match>>>;

const HELLO: u8 = 0;
const INPUT: u8 = 1;
const CHECK: u8 = 2;

fn varint(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn read_varint(b: &[u8], at: &mut usize) -> Option<u64> {
    let mut n = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*at)?;
        *at += 1;
        n |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(n);
        }
    }
    None
}

fn read_bytes<'b>(b: &'b [u8], at: &mut usize) -> Option<&'b [u8]> {
    let len = read_varint(b, at)? as usize;
    let s = b.get(*at..at.checked_add(len)?)?;
    *at += len;
    Some(s)
}

fn hash_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn empty_input() -> Vec<u8> {
    vec![0] // nil
}

impl Core {
    fn starter(&self) -> usize {
        0
    }

    /// This peer's hello: who it plays with and whether it still lacks the setup — and, from the
    /// starter with `full`, the setup itself in as many pieces as the channel's size needs.
    fn hellos(&self, full: bool) -> Vec<Vec<u8>> {
        let head = |total: usize, offset: usize, bytes: &[u8]| {
            let mut out = vec![HELLO];
            out.extend(self.players_hash);
            out.push(self.setup.is_none() as u8);
            varint(total as u64, &mut out);
            varint(offset as u64, &mut out);
            out.extend(bytes);
            out
        };
        match (&self.setup, full && self.me == self.starter()) {
            (Some(s), true) => {
                let chunks: Vec<&[u8]> = if s.is_empty() { vec![&[][..]] } else { s.chunks(self.chunk).collect() };
                let mut offset = 0;
                chunks
                    .into_iter()
                    .map(|c| {
                        let m = head(s.len(), offset, c);
                        offset += c.len();
                        m
                    })
                    .collect()
            }
            _ => vec![head(0, 0, &[])],
        }
    }

    /// One piece of the starter's setup; the whole once every byte is in.
    fn setup_part(&mut self, total: u64, offset: u64, bytes: &[u8]) {
        if self.setup.is_some() {
            return;
        }
        self.parts.insert(offset, bytes.to_vec());
        let have: u64 = self.parts.values().map(|b| b.len() as u64).sum();
        if have >= total {
            let mut whole = Vec::with_capacity(total as usize);
            for (at, b) in std::mem::take(&mut self.parts) {
                if at as usize == whole.len() {
                    whole.extend(b);
                }
            }
            if whole.len() as u64 == total {
                self.setup = Some(whole);
            }
        }
    }

    /// An input, then per player what we have of theirs and how far ahead of them we think we
    /// are: the first is their round trip, the second our clock's (see [`Self::lead`]).
    fn input_msg(&self, frame: u64, input: &[u8]) -> Vec<u8> {
        let mut out = vec![INPUT];
        varint(frame, &mut out);
        varint(input.len() as u64, &mut out);
        out.extend(input);
        for p in 0..self.players.len() {
            varint(self.known[p], &mut out);
            let lead = if p == self.me { 0 } else { self.local_lead(p) };
            varint(((lead << 1) ^ (lead >> 63)) as u64, &mut out);
        }
        out
    }

    /// Frames we are past player `p`, by the newest input of theirs: they send frame `f + delay`'s
    /// on starting frame `f`. It counts their input's trip too; [`Self::lead`] cancels that out.
    fn local_lead(&self, p: usize) -> i64 {
        self.frame as i64 - (self.known[p] as i64 - self.delay as i64)
    }

    /// Frames we are ahead of the furthest-behind peer. Each side's own estimate is off by the
    /// same one-way trip, in opposite directions, so half their difference is the true lead
    /// (GGPO's frame advantage). A device that started later, or whose clock runs slow, is
    /// behind; the one ahead slows, so neither guesses more than the network makes it.
    fn lead(&self) -> f64 {
        (0..self.players.len())
            .filter(|&p| p != self.me && self.live(p, self.frame))
            .filter_map(|p| Some((self.local_lead(p) - self.their_lead[p]?) as f64 / 2.0))
            .fold(0.0, f64::max)
    }

    /// Player `p` says what it has of ours, and how far ahead of us it is.
    fn their_view(&mut self, p: usize, known_of_me: u64, lead: i64) {
        self.their_lead[p] = Some(lead);
        if known_of_me > self.acked[p] {
            self.acked[p] = known_of_me;
            if let (Some(&sent), Some(now)) = (self.sent_at.get(&(known_of_me - 1)), self.clock) {
                let rtt = (now - sent).max(0.0);
                self.ping[p] = Some(self.ping[p].map_or(rtt, |was| was * 0.8 + rtt * 0.2));
            }
        }
        let all = (0..self.players.len()).filter(|&q| q != self.me).map(|q| self.acked[q]).min().unwrap_or(0);
        self.sent_at = self.sent_at.split_off(&all.saturating_sub(1));
        while self.sent_at.len() > 512 {
            self.sent_at.pop_first();
        }
    }

    fn check_msg(frame: u64, hash: &str) -> Vec<u8> {
        let mut out = vec![CHECK];
        varint(frame, &mut out);
        varint(hash.len() as u64, &mut out);
        out.extend(hash.as_bytes());
        out
    }

    fn alone(&self) -> bool {
        self.players.len() == 1
    }

    /// Whether player `p`'s input counts at frame `f`.
    fn live(&self, p: usize, f: u64) -> bool {
        self.drop_from.get(&p).is_none_or(|&d| f < d)
    }

    /// The first frame some counted input is still missing for. A dropped player holds nothing up
    /// once every input of theirs that counts is in.
    fn confirmed(&self) -> u64 {
        (0..self.players.len())
            .map(|p| match self.drop_from.get(&p) {
                Some(&d) if self.known[p] >= d => u64::MAX,
                _ => self.known[p],
            })
            .min()
            .unwrap_or(0)
    }

    fn put_input(&mut self, p: usize, frame: u64, bytes: Vec<u8>) {
        if !self.live(p, frame) {
            return;
        }
        if frame < self.known[p] && self.inputs[p].get(&frame).is_none() {
            return; // pruned: older than anything still open
        }
        if let Some(used) = self.used.get(&frame) {
            if used[p] != bytes {
                self.rewind = Some(self.rewind.map_or(frame, |r| r.min(frame)));
            }
        }
        self.inputs[p].insert(frame, bytes);
        while self.inputs[p].contains_key(&self.known[p]) {
            self.known[p] += 1;
        }
    }

    /// The input player `p` ran frame `f` on: theirs if in, else their last one before it.
    fn input_for(&self, p: usize, f: u64) -> Vec<u8> {
        if !self.live(p, f) {
            return empty_input();
        }
        self.inputs[p]
            .range(..=f)
            .next_back()
            .map(|(_, v)| v.clone())
            .unwrap_or_else(empty_input)
    }

    fn their_check(&mut self, p: usize, frame: u64, hash: String) {
        if frame > self.settled && self.live(p, frame) {
            self.hashes.entry(frame).or_default().insert(p, hash);
            self.settle();
        }
    }

    /// Verdicts, checkpoint by checkpoint, once every counted player's hash is in. A strict
    /// majority stands: the others are dropped `DROP_AFTER` checkpoints on — or, if we are the
    /// odd one out, our match ends. With no majority the match is disputed.
    fn settle(&mut self) {
        while self.status == Status::Playing {
            let f = self.settled + self.every;
            let counted: Vec<usize> = (0..self.players.len()).filter(|&p| self.live(p, f)).collect();
            let Some(hashes) = self.hashes.get(&f) else { return };
            if !counted.iter().all(|p| hashes.contains_key(p)) {
                return;
            }
            let hashes = self.hashes.remove(&f).expect("checked");
            self.settled = f;
            let mut groups: BTreeMap<&String, Vec<usize>> = BTreeMap::new();
            for &p in &counted {
                groups.entry(&hashes[&p]).or_default().push(p);
            }
            if groups.len() == 1 {
                continue;
            }
            self.stats.desyncs += 1;
            let majority = groups.values().find(|g| g.len() * 2 > counted.len()).cloned();
            let Some(majority) = majority else {
                eprintln!("net.match {}: no majority at frame {f}", self.id);
                self.status = Status::Disputed;
                self.ended = Some(format!("disputed at frame {f}"));
                return;
            };
            let from = f + DROP_AFTER * self.every;
            if !majority.contains(&self.me) {
                // Out from `from`, as the others see it: play to there so they have every input of
                // ours that still counts, then stop (`advance_inner`).
                eprintln!("net.match {}: this device differs from the others at frame {f}", self.id);
                let me = self.me;
                if !self.drop_from.contains_key(&me) {
                    self.drop_from.insert(me, from);
                    self.ended = Some(format!("desynced at frame {f}"));
                }
                return;
            }
            // Once dropped, a player's frame stays: its own device stops there.
            let losers: Vec<usize> =
                counted.iter().copied().filter(|p| !majority.contains(p) && !self.drop_from.contains_key(p)).collect();
            for p in losers {
                eprintln!("net.match {}: {} differs at frame {f}; out from frame {from}", self.id, self.players[p]);
                self.desynced.insert(p);
                self.drop_from.insert(p, from);
                if self.frame > from {
                    self.rewind = Some(self.rewind.map_or(from, |r| r.min(from)));
                }
            }
        }
    }

    /// Drop what no rewind can reach: frames before the last one every input is in for.
    fn prune(&mut self) {
        let keep = self.confirmed().min(self.frame).min(self.checked + self.every);
        self.snapshots = self.snapshots.split_off(&keep);
        self.used = self.used.split_off(&keep);
        for inputs in self.inputs.iter_mut() {
            // Keep the last one before `keep`: it is the prediction for what follows.
            let last = inputs.range(..keep).next_back().map(|(f, _)| *f);
            let mut rest = inputs.split_off(&keep);
            if let Some(f) = last {
                if let Some(v) = inputs.remove(&f) {
                    rest.insert(f, v);
                }
            }
            *inputs = rest;
        }
    }
}

impl Match {
    pub fn status(&self) -> Status {
        self.core.borrow().status
    }

    pub fn stats(&self) -> Stats {
        self.core.borrow().stats.clone()
    }

    /// What the match says of itself: status (as Lua reads it), who was dropped as desynced, who
    /// it waits for, why it ended.
    pub fn info(&self) -> Info {
        let core = self.core.borrow();
        let names = |ps: &mut dyn Iterator<Item = usize>| ps.map(|p| core.players[p].clone()).collect();
        Info {
            status: if core.status == Status::Playing && core.stalled_for >= WAITING { "waiting" } else { core.status.name() },
            desynced: names(&mut core.desynced.iter().copied()),
            waiting: names(&mut core.waiting_for.iter().copied()),
            ended: core.ended.clone(),
            frame: core.frame,
            ping: (0..core.players.len())
                .filter_map(|p| Some((core.players[p].clone(), (core.ping[p]? * 1000.0).round() as u64)))
                .collect(),
        }
    }

    /// Why the match ended in error, once — for the console.
    pub fn take_error(&self) -> Option<String> {
        let mut core = self.core.borrow_mut();
        if core.error_told {
            return None;
        }
        core.error_told = core.error.is_some();
        core.error.clone()
    }

    pub fn checks(&self) -> Vec<(u64, String)> {
        self.core.borrow().checks.clone()
    }

    fn send(&self, net: &Net, body: Vec<u8>) {
        let core = self.core.borrow();
        if core.alone() {
            return;
        }
        net.push(Outgoing { channel: core.id.clone(), body });
    }

    /// A message on the match's channel from `from`. `false` if `from` is not a player.
    pub fn feed(&self, net: &Net, from: &str, body: &[u8]) -> bool {
        let reply = {
            let mut core = self.core.borrow_mut();
            let Some(p) = core.players.iter().position(|id| id == from) else {
                return false;
            };
            let mut at = 1;
            match body.first() {
                Some(&HELLO) => {
                    let Some(hash) = body.get(1..33) else { return true };
                    if hash != core.players_hash {
                        eprintln!("net.match {}: {from} plays with other players", core.id);
                        return true;
                    }
                    let needs = body.get(33).copied() == Some(1);
                    at = 34;
                    let (Some(total), Some(offset)) = (read_varint(body, &mut at), read_varint(body, &mut at)) else {
                        return true;
                    };
                    if p == core.starter() && (total > 0 || core.setup.is_none()) {
                        let bytes = body.get(at..).unwrap_or_default();
                        core.setup_part(total, offset, bytes);
                    }
                    // Answer a first hello, so a peer that made its match after ours hears us; and
                    // a peer still short of the setup, with it.
                    let first = core.hello.insert(p);
                    (first || needs).then(|| core.hellos(needs))
                }
                Some(&INPUT) => {
                    let (Some(frame), Some(input)) = (read_varint(body, &mut at), read_bytes(body, &mut at).map(<[u8]>::to_vec)) else {
                        return true;
                    };
                    core.put_input(p, frame, input);
                    let me = core.me;
                    for q in 0..core.players.len() {
                        let (Some(known), Some(lead)) = (read_varint(body, &mut at), read_varint(body, &mut at)) else {
                            break;
                        };
                        if q == me {
                            core.their_view(p, known, ((lead >> 1) as i64) ^ -((lead & 1) as i64));
                        }
                    }
                    None
                }
                Some(&CHECK) => {
                    let (Some(frame), Some(hash)) = (read_varint(body, &mut at), read_bytes(body, &mut at)) else {
                        return true;
                    };
                    let hash = String::from_utf8_lossy(hash).into_owned();
                    core.their_check(p, frame, hash);
                    None
                }
                _ => None,
            }
        };
        for hello in reply.into_iter().flatten() {
            self.send(net, hello);
        }
        true
    }

    /// Run the match up to `now` (absolute seconds). `true` while it wants frames.
    pub fn advance(&self, vm: &Lua, net: &Net, now: f64, active: bool) -> bool {
        if let Err(e) = self.advance_inner(vm, net, now, active) {
            let mut core = self.core.borrow_mut();
            eprintln!("net.match {}: {e}", core.id);
            core.error = Some(e.to_string());
            core.status = Status::Ended;
        }
        matches!(self.core.borrow().status, Status::Lobby | Status::Playing)
    }

    fn advance_inner(&self, vm: &Lua, net: &Net, now: f64, active: bool) -> mlua::Result<()> {
        if matches!(self.status(), Status::Ended | Status::Disputed) {
            return Ok(());
        }
        let elapsed = {
            let mut core = self.core.borrow_mut();
            let last = core.clock.replace(now);
            match last {
                Some(last) if active => (now - last).max(0.0),
                _ => 0.0,
            }
        };
        if self.status() == Status::Lobby {
            let (ready, resend) = {
                let mut core = self.core.borrow_mut();
                core.since_hello += elapsed;
                let ready = core.hello.len() == core.players.len() && core.setup.is_some();
                let resend = !ready && core.since_hello >= 0.5;
                if resend {
                    core.since_hello = 0.0;
                }
                (ready, resend.then(|| core.hellos(false)))
            };
            for hello in resend.into_iter().flatten() {
                self.send(net, hello);
            }
            if !ready {
                return Ok(());
            }
            self.start(vm)?;
        }
        self.rewind(vm)?;
        {
            let mut core = self.core.borrow_mut();
            // Ahead by more than half a frame: run up to a tenth slow until level.
            let slow = ((core.lead() - 0.5) * 0.04).clamp(0.0, MOST_SLOW);
            if slow > 0.0 && elapsed > 0.0 {
                core.stats.slowed += 1;
            }
            core.carry = (core.carry + elapsed * (1.0 - slow)).min(core.frame_dt * f64::from(CATCH_UP));
        }
        let mut ran = 0;
        let mut waiting: Option<bool> = None; // Some(true): on an input, Some(false): on a verdict
        loop {
            let (late, verdict) = {
                let core = self.core.borrow();
                if core.carry < core.frame_dt || ran >= CATCH_UP || core.status != Status::Playing {
                    break;
                }
                if core.drop_from.get(&core.me).is_some_and(|&d| core.frame >= d) {
                    break;
                }
                let verdict = !core.alone() && core.frame >= core.settled + (DROP_AFTER + 1) * core.every;
                (core.frame >= core.confirmed() + WINDOW, verdict)
            };
            if late || verdict {
                self.core.borrow_mut().stats.stalls += 1;
                waiting = Some(late);
                break;
            }
            self.read_input(net)?;
            let f = self.core.borrow().frame;
            self.run_frame(vm, f)?;
            let mut core = self.core.borrow_mut();
            core.frame += 1;
            core.stats.frames += 1;
            core.carry -= core.frame_dt;
            ran += 1;
        }
        self.check(net)?;
        let mut core = self.core.borrow_mut();
        let me = core.me;
        if core.drop_from.get(&me).is_some_and(|&d| core.frame >= d) && core.status == Status::Playing {
            core.status = Status::Ended;
        }
        if let Some(on_input) = waiting {
            core.stalled_for += elapsed;
            let me = core.me;
            core.waiting_for = if on_input {
                let at = core.confirmed();
                (0..core.players.len()).filter(|&p| p != me && core.known[p] == at && core.live(p, at)).collect()
            } else {
                let f = core.settled + core.every;
                let have = core.hashes.get(&f);
                (0..core.players.len())
                    .filter(|&p| p != me && core.live(p, f) && !have.is_some_and(|h| h.contains_key(&p)))
                    .collect()
            };
            if core.stalled_for >= DROP_OUT && core.status == Status::Playing {
                let who: Vec<String> = core.waiting_for.iter().map(|&p| core.players[p].clone()).collect();
                eprintln!("net.match {}: {} gone for {DROP_OUT} s; the match ends", core.id, who.join(", "));
                core.status = Status::Ended;
                core.ended = Some(format!("dropped out: {}", who.join(", ")));
            }
        } else {
            core.stalled_for = 0.0;
            core.waiting_for.clear();
        }
        core.prune();
        Ok(())
    }

    fn start(&self, vm: &Lua) -> mlua::Result<()> {
        let (setup, players) = {
            let core = self.core.borrow();
            let setup = match core.setup.as_deref() {
                Some(s) => decode_body(vm, s)?,
                None => Value::Nil,
            };
            (setup, core.players.clone())
        };
        let ids = vm.create_sequence_from(players)?;
        let state: Table = self.state_fn.call((setup, ids))?;
        for (id, _) in self.worlds.iter() {
            let mut w = world::World2d::default();
            w.set_driven(true);
            self.world_map.borrow_mut().insert(id.clone(), w);
        }
        let mut core = self.core.borrow_mut();
        // Everyone ran frames before the delay on no input; nothing to wait for there.
        let delay = core.delay;
        for p in 0..core.players.len() {
            for f in 0..delay {
                core.inputs[p].insert(f, empty_input());
            }
            core.known[p] = core.known[p].max(delay);
            while core.inputs[p].contains_key(&core.known[p]) {
                let k = core.known[p];
                core.known[p] = k + 1;
            }
        }
        core.state = Some(state);
        core.status = Status::Playing;
        Ok(())
    }

    fn read_input(&self, net: &Net) -> mlua::Result<()> {
        let v: Value = self.input_fn.call(())?;
        let mut bytes = Vec::new();
        encode(&v, &mut bytes, 0)?;
        let (frame, me) = {
            let core = self.core.borrow();
            (core.frame + core.delay, core.me)
        };
        let msg = {
            let mut core = self.core.borrow_mut();
            core.put_input(me, frame, bytes.clone());
            if let Some(now) = core.clock {
                core.sent_at.insert(frame, now);
            }
            core.input_msg(frame, &bytes)
        };
        self.send(net, msg);
        Ok(())
    }

    /// Each driven world brought to what `state` describes.
    fn describe_worlds(&self, state: &Table) -> mlua::Result<()> {
        for (id, describe) in self.worlds.iter() {
            let desc: Table = describe.call(state.clone())?;
            let mut map = self.world_map.borrow_mut();
            let w = map.get_mut(id).ok_or_else(|| Error::runtime(format!("world {id:?} is gone")))?;
            crate::describe_world(w, &desc, id)?;
        }
        Ok(())
    }

    fn snap(&self, state: &Table, digest: bool) -> mlua::Result<Snap> {
        let mut bytes = Vec::new();
        encode(&Value::Table(state.clone()), &mut bytes, 0)?;
        let map = self.world_map.borrow();
        let mut worlds = Vec::new();
        let mut world_bytes = Vec::new();
        for (id, _) in self.worlds.iter() {
            let w = map.get(id).ok_or_else(|| Error::runtime(format!("world {id:?} is gone")))?;
            worlds.push(w.snapshot());
            if digest {
                world_bytes.extend(w.digest());
            }
        }
        Ok(Snap { state: bytes, worlds, pending: self.core.borrow().pending.clone(), digest: world_bytes })
    }

    fn events_table(&self, vm: &Lua) -> mlua::Result<Table> {
        let pending = std::mem::take(&mut self.core.borrow_mut().pending);
        let out = vm.create_table()?;
        for (w, e) in pending {
            let t = vm.create_table()?;
            t.set("world", w)?;
            match e {
                world::WorldEvent::Hit { id, who, speed } => {
                    (t.set("kind", "hit")?, t.set("id", id)?, t.set("who", who)?, t.set("speed", speed)?);
                }
                world::WorldEvent::Enter { id, who } => {
                    (t.set("kind", "zone")?, t.set("phase", "enter")?, t.set("id", id)?, t.set("who", who)?);
                }
                world::WorldEvent::Exit { id, who } => {
                    (t.set("kind", "zone")?, t.set("phase", "leave")?, t.set("id", id)?, t.set("who", who)?);
                }
                world::WorldEvent::Move { id, dx, dy } => {
                    (t.set("kind", "move")?, t.set("id", id)?, t.set("dx", dx)?, t.set("dy", dy)?);
                }
                other => {
                    t.set("kind", format!("{other:?}"))?;
                }
            }
            out.raw_push(t)?;
        }
        Ok(out)
    }

    fn open_worlds(&self, open: bool) {
        let mut map = self.world_map.borrow_mut();
        for (id, _) in self.worlds.iter() {
            if let Some(w) = map.get_mut(id) {
                w.open_commands(open);
            }
        }
    }

    /// Frame `f` from the state now held: worlds described, snapshot, then `ticks` steps on its
    /// inputs, each followed by the worlds' own steps.
    fn run_frame(&self, vm: &Lua, f: u64) -> mlua::Result<()> {
        let (state, used, ticks, checkpoint, guessed) = {
            let core = self.core.borrow();
            let state = core.state.clone().expect("playing");
            let used: Vec<Vec<u8>> = (0..core.players.len()).map(|p| core.input_for(p, f)).collect();
            (state, used, core.ticks, f % core.every == 0, f >= core.confirmed())
        };
        self.describe_worlds(&state)?;
        // Only a frame run on a guessed input can be rewound to (a drop-out rewinds to a
        // checkpoint), so only those and checkpoints are kept: alone, or on a LAN where inputs
        // land in time, that is nearly none.
        let snap = (checkpoint || guessed).then(|| self.snap(&state, checkpoint)).transpose()?;
        let inputs = vm.create_table()?;
        for (i, bytes) in used.iter().enumerate() {
            inputs.raw_set(i + 1, decode_body(vm, bytes)?)?;
        }
        for _ in 0..ticks {
            let events = self.events_table(vm)?;
            self.open_worlds(true);
            let stepped = self.step_fn.call::<()>((state.clone(), inputs.clone(), events));
            self.open_worlds(false);
            stepped?;
            let mut map = self.world_map.borrow_mut();
            let mut happened = Vec::new();
            for (id, _) in self.worlds.iter() {
                if let Some(w) = map.get_mut(id) {
                    w.step_driven(self.steps_per_tick);
                    happened.extend(w.drain_events().into_iter().map(|e| (id.clone(), e)));
                }
            }
            drop(map);
            self.core.borrow_mut().pending = happened;
        }
        let mut core = self.core.borrow_mut();
        match snap {
            Some(snap) => core.snapshots.insert(f, Rc::new(snap)),
            // A rerun may leave a stale one from when this frame was guessed.
            None => core.snapshots.remove(&f),
        };
        if checkpoint || guessed {
            core.stats.snapshots += 1;
        }
        core.used.insert(f, used);
        core.stats.steps += u64::from(ticks);
        Ok(())
    }

    fn rewind(&self, vm: &Lua) -> mlua::Result<()> {
        let (from, to, snap) = {
            let mut core = self.core.borrow_mut();
            let Some(from) = core.rewind.take() else {
                return Ok(());
            };
            let Some(snap) = core.snapshots.get(&from).cloned() else {
                return Err(Error::runtime(format!("rewind to frame {from}, which is gone")));
            };
            (from, core.frame, snap)
        };
        let Value::Table(state) = decode_body(vm, &snap.state)? else {
            return Err(Error::runtime("a snapshot that is not a table"));
        };
        // The worlds take the frame's shape from its state, then its motion from the snapshot.
        self.describe_worlds(&state)?;
        {
            let mut map = self.world_map.borrow_mut();
            for ((id, _), saved) in self.worlds.iter().zip(&snap.worlds) {
                let w = map.get_mut(id).ok_or_else(|| Error::runtime(format!("world {id:?} is gone")))?;
                w.restore(saved).map_err(|e| Error::runtime(format!("world {id:?}: {e}")))?;
            }
        }
        let mut core = self.core.borrow_mut();
        core.state = Some(state);
        core.pending = snap.pending.clone();
        drop(core);
        for f in from..to {
            self.run_frame(vm, f)?;
        }
        let mut core = self.core.borrow_mut();
        core.stats.rollbacks += 1;
        core.stats.rolled_frames += to - from;
        Ok(())
    }

    /// Hash each checkpoint every input is in for, and tell the others.
    fn check(&self, net: &Net) -> mlua::Result<()> {
        let sent = {
            let mut core = self.core.borrow_mut();
            let mut sent = Vec::new();
            let ready = core.confirmed().min(core.frame.saturating_sub(1));
            while core.checked + core.every <= ready {
                let f = core.checked + core.every;
                core.checked = f;
                let Some(snap) = core.snapshots.get(&f) else { continue };
                let h = hash_hex(&[snap.state.as_slice(), &snap.digest].concat());
                core.checks.push((f, h.clone()));
                if core.checks.len() > 64 {
                    core.checks.remove(0);
                }
                let me = core.me;
                core.hashes.entry(f).or_default().insert(me, h.clone());
                core.settle();
                sent.push(Core::check_msg(f, &h));
            }
            sent
        };
        for m in sent {
            self.send(net, m);
        }
        Ok(())
    }
}

impl UserData for Match {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("status", |_, m| {
            let core = m.core.borrow();
            Ok(if core.status == Status::Playing && core.stalled_for >= WAITING { "waiting" } else { core.status.name() })
        });
        fields.add_field_method_get("waiting", |vm, m| {
            let core = m.core.borrow();
            vm.create_sequence_from(core.waiting_for.iter().map(|p| core.players[*p].clone()).collect::<Vec<_>>())
        });
        fields.add_field_method_get("ended", |_, m| Ok(m.core.borrow().ended.clone()));
        fields.add_field_method_get("state", |_, m| Ok(m.core.borrow().state.clone()));
        fields.add_field_method_get("frame", |_, m| Ok(m.core.borrow().frame));
        // Clock not yet stepped, under one frame: the view draws moving things this far ahead, or a
        // display rate that isn't a multiple of the frame rate judders (1, 2, 1, 2 steps a paint).
        fields.add_field_method_get("ahead", |_, m| {
            let core = m.core.borrow();
            Ok(if core.status == Status::Playing { core.carry.clamp(0.0, core.frame_dt) } else { 0.0 })
        });
        // Round trip to each other player, in whole milliseconds, once one has been measured.
        fields.add_field_method_get("ping", |vm, m| {
            let core = m.core.borrow();
            let out = vm.create_table()?;
            for (p, ping) in core.ping.iter().enumerate() {
                if let Some(s) = ping {
                    out.set(core.players[p].as_str(), (s * 1000.0).round() as i64)?;
                }
            }
            Ok(out)
        });
        fields.add_field_method_get("stats", |vm, m| {
            let s = m.core.borrow().stats.clone();
            let out = vm.create_table()?;
            out.set("frames", s.frames)?;
            out.set("rollbacks", s.rollbacks)?;
            out.set("rolled_frames", s.rolled_frames)?;
            out.set("stalls", s.stalls)?;
            out.set("snapshots", s.snapshots)?;
            out.set("slowed", s.slowed)?;
            Ok(out)
        });
        fields.add_field_method_get("me", |_, m| Ok(m.core.borrow().me + 1));
        fields.add_field_method_get("players", |vm, m| vm.create_sequence_from(m.core.borrow().players.clone()));
        fields.add_field_method_get("error", |_, m| Ok(m.core.borrow().error.clone()));
        fields.add_field_method_get("checks", |vm, m| {
            let out = vm.create_table()?;
            for (f, h) in &m.core.borrow().checks {
                let row = vm.create_table()?;
                row.set("frame", *f)?;
                row.set("hash", h.as_str())?;
                out.raw_push(row)?;
            }
            Ok(out)
        });
        fields.add_field_method_get("desynced", |vm, m| {
            let core = m.core.borrow();
            vm.create_sequence_from(core.desynced.iter().map(|p| core.players[*p].clone()).collect::<Vec<_>>())
        });
    }
}

/// `net.match(spec)`, its channel's `rate` read from the manifest: checked, registered under its id (a second match with the id replaces the
/// first), hello sent. `me` is this device's id; `None` plays alone as `"local"`.
pub fn create(
    vm: &Lua,
    spec: Table,
    (rate, size): (u32, u32),
    me: Option<String>,
    net: &Net,
    matches: &Matches,
    world_map: &crate::Worlds,
) -> mlua::Result<Match> {
    let id: String = spec.get("id")?;
    let tick: f64 = spec.get::<Option<f64>>("tick")?.unwrap_or(1.0 / 60.0);
    let ticks: u32 = spec.get::<Option<u32>>("frame")?.unwrap_or(1);
    if !(tick > 0.0 && tick <= 1.0) || ticks == 0 || ticks > 16 {
        return Err(Error::runtime("net.match: tick is in (0, 1] seconds, frame is 1..16 ticks"));
    }
    let me = me.unwrap_or_else(|| "local".to_string());
    let mut players: Vec<String> = match spec.get::<Option<Table>>("players")? {
        None => vec![me.clone()],
        Some(t) => t
            .sequence_values::<Value>()
            .map(|v| match v? {
                Value::String(s) => Ok(s.to_str()?.to_string()),
                Value::Table(row) => row.get::<String>("id"),
                _ => Err(Error::runtime("net.match: players are ids or net.peers() rows")),
            })
            .collect::<mlua::Result<_>>()?,
    };
    players.sort();
    players.dedup();
    let Some(me_at) = players.iter().position(|p| *p == me) else {
        return Err(Error::runtime("net.match: this device is not one of the players"));
    };
    let setup_arg: Value = spec.get("setup")?;
    let needed = (1.0 / (tick * f64::from(ticks))).ceil() as u32 + 2;
    // Alone, nothing is sent: a single-player game may step its input every tick.
    if players.len() > 1 && rate < needed {
        return Err(Error::runtime(format!(
            "net.match: channel {id:?} sends {needed}/s at this tick and frame; declare `rate {needed}/s` or more"
        )));
    }
    let state_fn: Function = spec.get("state")?;
    let input_fn: Function = spec.get("input")?;
    let step_fn: Function = spec.get("step")?;
    let mut worlds: Vec<(String, Function)> = match spec.get::<Option<Table>>("worlds")? {
        None => Vec::new(),
        Some(t) => t.pairs::<String, Function>().collect::<mlua::Result<_>>()?,
    };
    worlds.sort_by(|a, b| a.0.cmp(&b.0));
    let steps_per_tick = (tick / world::STEP).round() as u32;
    if !worlds.is_empty() && (steps_per_tick == 0 || (tick - f64::from(steps_per_tick) * world::STEP).abs() > 1e-9) {
        return Err(Error::runtime(format!(
            "net.match: a match with worlds ticks in whole world steps of 1/{} s",
            (1.0 / world::STEP).round()
        )));
    }
    let mut setup = None;
    if me_at == 0 {
        let mut s = Vec::new();
        encode(&setup_arg, &mut s, 0)?;
        setup = Some(s);
    }
    let players_hash: [u8; 32] = Sha256::digest(players.join("\n").as_bytes()).into();
    let n = players.len();
    let frame_dt = tick * f64::from(ticks);
    let core = Core {
        id: id.clone(),
        ticks,
        frame_dt,
        players,
        me: me_at,
        players_hash,
        status: Status::Lobby,
        hello: BTreeSet::from([me_at]),
        parts: BTreeMap::new(),
        // Room for the hello's own fields: tag, players hash, flag, two varints.
        chunk: (size as usize).saturating_sub(64).max(64),
        delay: if n == 1 { 0 } else { ((DELAY / frame_dt) - 1e-9).ceil().max(1.0) as u64 },
        setup,
        state: None,
        frame: 0,
        inputs: vec![BTreeMap::new(); n],
        known: vec![0; n],
        used: BTreeMap::new(),
        snapshots: BTreeMap::new(),
        pending: Vec::new(),
        rewind: None,
        every: (1.0 / frame_dt).round().max(1.0) as u64,
        checked: 0,
        checks: Vec::new(),
        hashes: BTreeMap::new(),
        settled: 0,
        desynced: BTreeSet::new(),
        drop_from: BTreeMap::new(),
        stalled_for: 0.0,
        waiting_for: Vec::new(),
        ended: None,
        clock: None,
        carry: 0.0,
        since_hello: 0.0,
        sent_at: BTreeMap::new(),
        acked: vec![0; n],
        ping: vec![None; n],
        their_lead: vec![None; n],
        stats: Stats::default(),
        error: None,
        error_told: false,
    };
    let m = Match {
        core: Rc::new(RefCell::new(core)),
        state_fn,
        input_fn,
        step_fn,
        worlds: Rc::new(worlds),
        world_map: world_map.clone(),
        steps_per_tick,
    };
    let hellos = m.core.borrow().hellos(true);
    for hello in hellos {
        m.send(net, hello);
    }
    // Alone, there is nobody to wait for: playing from the first frame, its state readable now.
    if n == 1 {
        m.start(vm)?;
    }
    matches.borrow_mut().insert(id, m.clone());
    Ok(m)
}
