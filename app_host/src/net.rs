//! `net`: an app's ephemeral channels (`jetpack-mp.md` §6 step 4). A message is a plain value sent
//! to every other device with the same app open, never stored. The host signs, relays and
//! delivers; this module is what Lua sees: the channel checked against the app's own
//! `manifest.osv`, a local rate check that says so in the console (the host enforces the same
//! cap), the value encoded, and who is here.
//!
//! The core outlives a reload like the worlds do: who is here, the outbox and the rate buckets
//! are the app's, not one VM's. `net.on` handlers are per VM — a reload's `main.lua` registers
//! its own.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::Instant;

use manifest::Manifest;
use mlua::{Error, Function, Lua, Table, Value};

use crate::host::HostContext;

/// One device here on this app: its device key (hex) and the account that attested it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetPeer {
    pub id: String,
    pub account: String,
}

/// A message on its way out, for the host to sign and route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub channel: String,
    pub body: Vec<u8>,
}

#[derive(Default)]
struct Core {
    me: Option<NetPeer>,
    /// Everyone else here, by id.
    peers: BTreeMap<String, NetPeer>,
    outbox: Vec<Outgoing>,
    /// Console lines for the host to log (`LuaApp::net_take`).
    notes: Vec<String>,
    buckets: HashMap<String, (f64, u64)>,
}

pub type Clock = Rc<dyn Fn() -> u64>;

#[derive(Clone)]
pub struct Net {
    core: Rc<RefCell<Core>>,
    clock: Clock,
}

impl Default for Net {
    fn default() -> Self {
        let start = Instant::now();
        Self {
            core: Rc::default(),
            clock: Rc::new(move || start.elapsed().as_millis() as u64),
        }
    }
}

/// `net.on` handlers by channel, for one VM.
pub type Handlers = Rc<RefCell<HashMap<String, Function>>>;

impl Net {
    pub fn with_clock(clock: Clock) -> Self {
        Self { clock, ..Self::default() }
    }

    pub fn set_me(&self, me: NetPeer) {
        let mut core = self.core.borrow_mut();
        core.peers.remove(&me.id);
        core.me = Some(me);
    }

    pub fn here(&self, peer: NetPeer) {
        let mut core = self.core.borrow_mut();
        if core.me.as_ref().is_some_and(|m| m.id == peer.id) {
            return;
        }
        core.peers.insert(peer.id.clone(), peer);
    }

    pub fn gone(&self, id: &str) {
        self.core.borrow_mut().peers.remove(id);
    }

    /// The route went down: nobody else is here until it is back.
    pub fn alone(&self) {
        self.core.borrow_mut().peers.clear();
    }

    pub fn is_peer(&self, id: &str) -> bool {
        self.core.borrow().peers.contains_key(id)
    }

    /// Onto the outbox, unchecked: a match's own traffic, whose rate was checked at creation.
    pub(crate) fn push(&self, out: Outgoing) {
        self.core.borrow_mut().outbox.push(out);
    }

    pub fn me(&self) -> Option<String> {
        self.core.borrow().me.as_ref().map(|m| m.id.clone())
    }

    pub fn take(&self) -> (Vec<Outgoing>, Vec<String>) {
        let mut core = self.core.borrow_mut();
        (std::mem::take(&mut core.outbox), std::mem::take(&mut core.notes))
    }

    /// The same bucket as the host's (`courier::net::Rate`): a second's burst, refilled at the
    /// channel's rate.
    fn allow(&self, channel: &str, per_sec: Option<u32>) -> bool {
        let now = (self.clock)();
        let cap = f64::from(per_sec.unwrap_or(manifest::DEFAULT_CHANNEL_RATE));
        let mut core = self.core.borrow_mut();
        let (tokens, last) = core.buckets.entry(channel.to_string()).or_insert((cap, now));
        *tokens = (*tokens + now.saturating_sub(*last) as f64 * cap / 1000.0).min(cap);
        *last = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// The app's manifest, as `net` reads it: `Err` says why there is none to send by.
pub type Rules = Rc<Result<Manifest, String>>;

pub fn rules(src: &loro::LoroDoc) -> Rules {
    use loro::{Container, ValueOrContainer};
    Rc::new(match src.get_map("files").get("manifest.osv") {
        Some(ValueOrContainer::Container(Container::Text(t))) => {
            Manifest::parse(&t.to_string()).map_err(|e| format!("manifest.osv: {e}"))
        }
        _ => Err("the app has no manifest.osv to declare channels in".to_string()),
    })
}

fn channel<'m>(rules: &'m Rules, name: &str) -> mlua::Result<&'m manifest::Channel> {
    let m = rules.as_ref().as_ref().map_err(|e| Error::runtime(format!("net: {e}")))?;
    m.channels
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| Error::runtime(format!("net: no channel {name:?} in manifest.osv")))
}

pub fn install(
    vm: &Lua,
    net: Net,
    rules: Rules,
    handlers: Handlers,
    matches: crate::net_match::Matches,
    worlds: crate::Worlds,
    context: HostContext,
) -> mlua::Result<()> {
    let api = vm.create_table()?;
    {
        let (net, rules) = (net.clone(), rules.clone());
        api.set(
            "match",
            vm.create_function(move |vm, spec: Table| {
                let id: String = spec.get("id")?;
                let ch = channel(&rules, &id)?;
                let rate = ch.rate_per_sec.unwrap_or(manifest::DEFAULT_CHANNEL_RATE);
                let size = ch.size_bytes.unwrap_or(manifest::CHANNEL_SIZE_MAX);
                crate::net_match::create(vm, spec, (rate, size), net.me(), &net, &matches, &worlds)
            })?,
        )?;
    }
    {
        let (net, rules) = (net.clone(), rules.clone());
        api.set(
            "send",
            vm.create_function(move |_, (name, value): (String, Value)| {
                context.require_handler().map_err(|_| Error::runtime("net.send is only allowed in handlers"))?;
                let ch = channel(&rules, &name)?;
                let mut body = Vec::new();
                encode(&value, &mut body, 0)?;
                let max = ch.size_bytes.unwrap_or(manifest::CHANNEL_SIZE_MAX) as usize;
                if body.len() > max {
                    return Err(Error::runtime(format!("net: {} bytes is over {name:?}'s size {max}", body.len())));
                }
                if !net.allow(&name, ch.rate_per_sec) {
                    net.core.borrow_mut().notes.push(format!("net: over rate on {name:?}"));
                    return Ok(false);
                }
                net.core.borrow_mut().outbox.push(Outgoing { channel: name, body });
                Ok(true)
            })?,
        )?;
    }
    {
        let handlers = handlers.clone();
        api.set(
            "on",
            vm.create_function(move |_, (name, f): (String, Option<Function>)| {
                channel(&rules, &name)?;
                match f {
                    Some(f) => handlers.borrow_mut().insert(name, f),
                    None => handlers.borrow_mut().remove(&name),
                };
                Ok(())
            })?,
        )?;
    }
    api.set(
        "peers",
        vm.create_function(move |vm, ()| {
            let core = net.core.borrow();
            let mut all: Vec<(&NetPeer, bool)> = core.peers.values().map(|p| (p, false)).collect();
            all.extend(core.me.as_ref().map(|m| (m, true)));
            all.sort_by(|a, b| a.0.id.cmp(&b.0.id));
            let out = vm.create_table()?;
            for (p, me) in all {
                let row = vm.create_table()?;
                row.set("id", p.id.as_str())?;
                row.set("account", p.account.as_str())?;
                row.set("me", me)?;
                out.raw_push(row)?;
            }
            Ok(out)
        })?,
    )?;
    api.set_readonly(true);
    vm.globals().set("net", api)
}

/// One received message into Lua: `fn(body, from)`, `from` the sender's id.
pub fn decode_body(vm: &Lua, body: &[u8]) -> mlua::Result<Value> {
    let mut at = 0;
    let v = decode(vm, body, &mut at, 0)?;
    if at != body.len() {
        return Err(Error::runtime("net: trailing bytes in a message"));
    }
    Ok(v)
}

// ── the wire form of a value ────────────────────────────────────────────────
//
// Tagged, little-endian, map keys sorted by their encoding: the same value encodes to the same
// bytes on every device, which a match's input exchange relies on.

const DEPTH: usize = 16;
const NIL: u8 = 0;
const FALSE: u8 = 1;
const TRUE: u8 = 2;
const NUM: u8 = 3;
const STR: u8 = 4;
const TABLE: u8 = 5;

fn varint(mut n: usize, out: &mut Vec<u8>) {
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

pub fn encode(v: &Value, out: &mut Vec<u8>, depth: usize) -> mlua::Result<()> {
    if depth > DEPTH {
        return Err(Error::runtime("net: a message nests deeper than 16 tables"));
    }
    match v {
        Value::Nil => out.push(NIL),
        Value::Boolean(false) => out.push(FALSE),
        Value::Boolean(true) => out.push(TRUE),
        Value::Integer(i) => {
            out.push(NUM);
            out.extend((*i as f64).to_le_bytes());
        }
        Value::Number(n) => {
            out.push(NUM);
            out.extend(n.to_le_bytes());
        }
        Value::String(s) => {
            out.push(STR);
            let b = s.as_bytes();
            varint(b.len(), out);
            out.extend(&*b);
        }
        Value::Table(t) => table(t, out, depth)?,
        other => {
            return Err(Error::runtime(format!(
                "net: a message holds numbers, strings, booleans and tables, not a {}",
                other.type_name()
            )));
        }
    }
    Ok(())
}

fn table(t: &Table, out: &mut Vec<u8>, depth: usize) -> mlua::Result<()> {
    let n = t.raw_len();
    let mut keyed = Vec::new();
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, v) = pair?;
        let index = match &k {
            Value::Integer(i) => Some(*i as f64),
            Value::Number(x) => Some(*x),
            _ => None,
        };
        if index.is_some_and(|i| i.fract() == 0.0 && i >= 1.0 && i <= n as f64) {
            continue;
        }
        if !matches!(k, Value::String(_) | Value::Integer(_) | Value::Number(_)) {
            return Err(Error::runtime("net: a message's keys are strings or numbers"));
        }
        let (mut kb, mut vb) = (Vec::new(), Vec::new());
        encode(&k, &mut kb, depth + 1)?;
        encode(&v, &mut vb, depth + 1)?;
        keyed.push((kb, vb));
    }
    keyed.sort();
    out.push(TABLE);
    varint(n, out);
    for i in 1..=n {
        encode(&t.raw_get::<Value>(i)?, out, depth + 1)?;
    }
    varint(keyed.len(), out);
    for (k, v) in keyed {
        out.extend(k);
        out.extend(v);
    }
    Ok(())
}

fn bad() -> Error {
    Error::runtime("net: a malformed message")
}

fn read_varint(b: &[u8], at: &mut usize) -> mlua::Result<usize> {
    let mut n = 0usize;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*at).ok_or_else(bad)?;
        *at += 1;
        n |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            return Ok(n);
        }
    }
    Err(bad())
}

fn decode(vm: &Lua, b: &[u8], at: &mut usize, depth: usize) -> mlua::Result<Value> {
    if depth > DEPTH {
        return Err(bad());
    }
    let tag = *b.get(*at).ok_or_else(bad)?;
    *at += 1;
    Ok(match tag {
        NIL => Value::Nil,
        FALSE => Value::Boolean(false),
        TRUE => Value::Boolean(true),
        NUM => {
            let bytes: [u8; 8] = b.get(*at..*at + 8).ok_or_else(bad)?.try_into().map_err(|_| bad())?;
            *at += 8;
            Value::Number(f64::from_le_bytes(bytes))
        }
        STR => {
            let len = read_varint(b, at)?;
            let s = b.get(*at..at.checked_add(len).ok_or_else(bad)?).ok_or_else(bad)?;
            *at += len;
            Value::String(vm.create_string(s)?)
        }
        TABLE => {
            let t = vm.create_table()?;
            let n = read_varint(b, at)?;
            if n > b.len() {
                return Err(bad());
            }
            for i in 1..=n {
                t.raw_set(i, decode(vm, b, at, depth + 1)?)?;
            }
            let keyed = read_varint(b, at)?;
            if keyed > b.len() {
                return Err(bad());
            }
            for _ in 0..keyed {
                let k = decode(vm, b, at, depth + 1)?;
                if !matches!(k, Value::String(_) | Value::Number(_)) {
                    return Err(bad());
                }
                t.raw_set(k, decode(vm, b, at, depth + 1)?)?;
            }
            Value::Table(t)
        }
        _ => return Err(bad()),
    })
}
