//! `net` (`jetpack-mp.md` §6 step 4): what an app sees of its channels. The transport is the
//! shell's; here the outbox stands in for it, and `net_deliver` for a message arriving.

use super::*;
use crate::net::{Net, NetPeer};
use std::cell::Cell;

const MANIFEST: &str = "app \"probe\" {\n  roles member\n  channel ping { rate 5/s }\n  channel big { size 16 }\n}\n";

const MAIN: &str = r#"
local got = {}
net.on("ping", function(body, from) table.insert(got, from .. ":" .. tostring(body.text)) end)
return function()
	local kids = {
		ui.button({ "send", id = "send", on_click = function() net.send("ping", { text = "hi", n = 3, list = { 1, 2 } }) end }),
		ui.button({ "flood", id = "flood", on_click = function() for _ = 1, 20 do net.send("ping", { text = "x" }) end end }),
		ui.button({ "bad", id = "bad", on_click = function() net.send("nope", 1) end }),
		ui.button({ "big", id = "big", on_click = function() net.send("big", string.rep("a", 40)) end }),
		ui.text({ "got " .. table.concat(got, ","), id = "got" }),
	}
	for _, p in ipairs(net.peers()) do
		table.insert(kids, ui.text({ p.id .. (p.me and "*" or ""), id = "peer:" .. p.id }))
	end
	return ui.col(kids)
end
"#;

fn probe(clock: Rc<Cell<u64>>) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let map = src.get_map("files");
    for (path, body) in [("manifest.osv", MANIFEST), ("main.lua", MAIN)] {
        map.insert_container(path, LoroText::new()).unwrap().insert(0, body).unwrap();
    }
    src.commit();
    let mut app = LuaApp::open(src, Rc::new(|_: &DocKey| Ok(None)), noop_wake(), identity()).unwrap();
    app.net = Net::with_clock(Rc::new(move || clock.get()));
    // `net.install` captured the old core; rebuild so the VM sees the clocked one.
    app.reload().unwrap();
    assert!(app.error.is_none(), "{:?}", app.error);
    app
}

fn click(app: &mut LuaApp<LuaMsg>, id: &str) {
    let _ = app.view();
    app.update(LuaMsg::CallAt(Key::new(id, "on_click"), 0.0, 0.0, Shape::default(), None));
}

fn texts(app: &LuaApp<LuaMsg>) -> String {
    format!("{:?}", app.view().info())
}

#[test]
fn a_sent_value_comes_out_the_same_on_the_other_side() {
    let mut a = probe(Rc::default());
    click(&mut a, "send");
    let out = a.net_take();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].channel, "ping");
    let mut b = probe(Rc::default());
    b.net_deliver("ping", "aaaa", &out[0].body);
    assert!(texts(&b).contains("got aaaa:hi"), "{}", texts(&b));
    // The same value encodes to the same bytes: map keys are sorted, not hash-ordered.
    click(&mut a, "send");
    assert_eq!(a.net_take()[0].body, out[0].body);
}

#[test]
fn an_undeclared_channel_is_an_error_in_the_console() {
    let mut a = probe(Rc::default());
    click(&mut a, "bad");
    assert!(a.net_take().is_empty());
    let console = a.console(10).join("\n");
    assert!(console.contains("no channel \"nope\""), "{console}");
}

#[test]
fn a_body_over_the_channel_size_is_refused() {
    let mut a = probe(Rc::default());
    click(&mut a, "big");
    assert!(a.net_take().is_empty());
    assert!(a.console(10).join("\n").contains("size 16"), "{:?}", a.console(10));
}

#[test]
fn sending_over_the_rate_drops_and_says_so() {
    let clock = Rc::new(Cell::new(1000));
    let mut a = probe(clock.clone());
    click(&mut a, "flood");
    assert_eq!(a.net_take().len(), 5, "a second's burst at 5/s");
    assert!(a.console(10).iter().any(|l| l.contains("net: over rate on \"ping\"")), "{:?}", a.console(10));
    clock.set(1400);
    click(&mut a, "flood");
    assert_eq!(a.net_take().len(), 2, "0.4 s refills two");
}

#[test]
fn send_is_refused_outside_a_handler() {
    let src = LoroDoc::new();
    let map = src.get_map("files");
    map.insert_container("manifest.osv", LoroText::new()).unwrap().insert(0, MANIFEST).unwrap();
    map.insert_container("main.lua", LoroText::new())
        .unwrap()
        .insert(0, "return function() net.send('ping', 1) return ui.text({'x'}) end")
        .unwrap();
    src.commit();
    let app = LuaApp::open(src, Rc::new(|_: &DocKey| Ok(None)), noop_wake(), identity()).unwrap();
    let _ = app.view();
    assert!(app.net_take().is_empty());
}

#[test]
fn peers_are_who_is_here_with_me_marked() {
    let mut a = probe(Rc::default());
    a.net().set_me(NetPeer { id: "bb".into(), account: "did:me".into() });
    a.net().here(NetPeer { id: "cc".into(), account: "did:c".into() });
    a.net().here(NetPeer { id: "aa".into(), account: "did:a".into() });
    let t = texts(&a);
    let (pa, pb, pc) = (t.find("peer:aa").unwrap(), t.find("peer:bb").unwrap(), t.find("peer:cc").unwrap());
    assert!(pa < pb && pb < pc, "sorted by id: {t}");
    assert!(t.contains("bb*"), "me is marked: {t}");
    a.net().gone("cc");
    assert!(!texts(&a).contains("peer:cc"));
    a.net().alone();
    let t = texts(&a);
    assert!(t.contains("peer:bb") && !t.contains("peer:aa"), "{t}");
    // A reload keeps who is here; it is the app's, not the VM's.
    a.net().here(NetPeer { id: "dd".into(), account: "did:d".into() });
    a.reload().unwrap();
    assert!(texts(&a).contains("peer:dd"));
    let _ = &mut a;
}

#[test]
fn a_malformed_message_is_a_handler_error_not_a_crash() {
    let mut b = probe(Rc::default());
    b.net_deliver("ping", "aa", &[5, 200]);
    assert!(b.console(10).iter().any(|l| l.contains("malformed")), "{:?}", b.console(10));
}
