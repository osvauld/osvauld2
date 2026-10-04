"""e2e_chat_sync.py — docs/design/group-chat-sync.md §0, end to end.

    python3 scripts/e2e_chat_sync.py            # every test
    python3 scripts/e2e_chat_sync.py t1 t5      # just these

One kunki node and two or three offscreen shells per test, each in its own throwaway
directory. Screenshots land in $OSVAULD_SHOTS (default target/e2e-shots) as
`T<n>-<peer>.png`; a step is judged on those as well as the assertions. OSVAULD_KEEP=1 keeps
the node and vault directories for a post-mortem.

Hostile peers (T9, T10, T14, T15) are modified clients: the peer edits its own local copy of
the app's source and reloads, exactly what someone who controls their own desktop can do.
T16 is a Rust test (`kunki` bridge tests) — forging a signature needs no shell.
"""

import json
import sys
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.net import Net, build, wait_until  # noqa: E402

CHAT = Path(__file__).parent.parent / "demo_apps" / "chat"
PUSH = 5.0  # generous: a push is milliseconds; this is process scheduling headroom
DAY = 86400


# ── chat helpers ─────────────────────────────────────────────────────────────


def send(p, item, text):
    p.rpc.type_text(item, "composer", text)
    p.rpc.key(item, "composer", "enter")


def data(p, item) -> dict:
    return p.rpc.read_data(item)


def messages(p, item, channel="general") -> list[dict]:
    """Every message of one channel, across all of its open docs (one per shard once sharded),
    in doc-name then list order."""
    out = []
    for name, d in sorted(data(p, item).items()):
        if name.startswith(f"channel/{channel}/") or name == f"channel:{channel}":
            out += (d or {}).get("messages") or []
    return out


def texts(p, item, channel="general") -> list[str]:
    return [m["text"] for m in messages(p, item, channel)]


def sees(p, item, text, channel="general", timeout=PUSH):
    wait_until(lambda: text in texts(p, item, channel), timeout, f"{p.name} to see {text!r}")


def never_sees(p, item, text, channel="general", settle=1.5):
    time.sleep(settle)
    assert text not in json.dumps(data(p, item)), f"{p.name} saw {text!r}"


def ids_on_screen(p) -> set[str]:
    return {r["id"] for r in p.rpc.rects()}


def patch_local(p, item, path, old, new):
    """A modified client: edit this peer's own copy of the source and reload it."""
    f = p.rpc.read_file_versioned(item, path)
    p.rpc.edit_file(item, path, f["revision"], [{"old_text": old, "new_text": new}])
    p.rpc.reload_item(item)


def shots(label, item, *peers):
    for p in peers:
        p.shot(label, item)


# ── tests ────────────────────────────────────────────────────────────────────


def t1(net):
    """Live push: B, chat open, sees A's message without asking."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(a, i, "hello from alice")
    sees(b, i, "hello from alice")
    send(b, i, "hello from bob")
    sees(a, i, "hello from bob")
    shots("T1", i, a, b)


def t2(net):
    """A late joiner doesn't re-seed: each channel appears once on both sides."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(a, i, "seeded once?")
    sees(b, i, "seeded once?")
    for p in (a, b):
        chans = [c["id"] for c in data(p, i)["chat"]["channels"]]
        assert len(chans) == len(set(chans)), f"{p.name} channels duplicated: {chans}"
        msgs = [m["id"] for m in messages(p, i)]
        assert len(msgs) == len(set(msgs)), f"{p.name} messages duplicated: {msgs}"
    shots("T2", i, a, b)


def t3(net):
    """Offline catch-up: B is down while A sends five; B restarts and has all five."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    b.stop()
    sent = [f"while you were out {n}" for n in range(5)]
    for t in sent:
        send(a, i, t)
    b.restart()
    b.rpc.open_item(i)
    for t in sent:
        sees(b, i, t)
    shots("T3", i, a, b)


def t4(net):
    """Partition: both send while the node is down; after it returns, same messages, same order."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    net.node.stop()
    send(a, i, "alice offline")
    send(b, i, "bob offline")
    net.node.start()
    for p in (a, b):
        p.rpc.request("SyncNow")
    sees(a, i, "bob offline")
    sees(b, i, "alice offline")
    assert texts(a, i) == texts(b, i), f"order differs:\n{texts(a, i)}\n{texts(b, i)}"
    shots("T4", i, a, b)


def t5(net):
    """A closed item catches up on open, not on the 20 s tick."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    b.rpc.request("CloseItem", item_id=i)
    send(a, i, "sent while closed")
    time.sleep(0.5)
    b.rpc.open_item(i)
    sees(b, i, "sent while closed", timeout=2.0)
    shots("T5", i, a, b)


def t6(net):
    """Day shards: day 1 and day 2 are separate docs; B loads today, scrolls back for day 1."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(a, i, "day one")
    for p in (a, b):
        p.rpc.request("SetClockOffset", secs=DAY)
    send(a, i, "day two")
    sees(b, i, "day two")
    names = [n for n in data(a, i) if n.startswith("channel/general/")]
    assert len(names) == 2, f"expected two day shards on alice, got {names}"
    b.restart()
    b.rpc.request("SetClockOffset", secs=DAY)
    b.rpc.open_item(i)
    sees(b, i, "day two")
    open_now = [n for n in data(b, i) if n.startswith("channel/general/")]
    assert len(open_now) == 1, f"bob loaded more than today: {open_now}"
    x, y = b.rpc.centre_of("messages")
    for _ in range(10):
        b.rpc.wheel(x, y, 0, -400)
    sees(b, i, "day one")
    shots("T6", i, a, b)


def t7(net):
    """Late joiner discovers the workspace, the app, every channel and shard — no hand-fed ids."""
    a, b = net.peer("alice"), net.peer("bob")
    ws, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(a, i, "before carol")
    a.rpc.click(i, "ch:design")
    send(a, i, "design talk")
    c = net.peer("carol")
    c.rpc.claim_node(a.rpc.invite())
    wait_until(lambda: any(w["id"] == ws["id"] for w in c.rpc.list_workspaces()), PUSH,
               "carol to discover the workspace")
    wait_until(lambda: any(x["id"] == i for x in c.rpc.list_items(ws["id"])), PUSH,
               "carol to discover the chat app")
    c.rpc.open_item(i)
    sees(c, i, "before carol")
    c.rpc.click(i, "ch:design")
    sees(c, i, "design talk", channel="design")
    shots("T7", i, a, c)


def t8(net):
    """A member creating a channel is refused and rolled back; the admin never sees it."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    # The UI hides channel creation from members; a modified client calls it anyway.
    patch_local(b, i, "main.lua", "return function()",
                'M.add_channel("bobs-room")\nreturn function()')
    wait_until(lambda: not any(c["id"] == "bobs-room" for c in data(b, i)["chat"]["channels"]),
               PUSH, "bob's channel to roll back")
    never_sees(a, i, "bobs-room")
    assert any("rejected" in line for line in b.rpc.read_console(i)), "no rejection reported"
    shots("T8", i, a, b)


def t9(net):
    """A message forged as A's is refused at the node."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(a, i, "real alice")
    sees(b, i, "real alice")
    alice_did = messages(b, i)[-1]["author"]
    patch_local(b, i, "model.lua", "author = M.me()", f'author = "{alice_did}"')
    send(b, i, "forged as alice")
    never_sees(a, i, "forged as alice")
    wait_until(lambda: "forged as alice" not in texts(b, i), PUSH, "bob's forgery to roll back")
    shots("T9", i, a, b)


def t10(net):
    """An A–B DM is invisible to C: not in C's index, refused when asked for by name."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    c = net.peer("carol")
    net.join(a, c, *_ws_item(a, i))
    c.rpc.open_item(i)
    a.rpc.click(i, f"dm:{_did(b, i)}")
    send(a, i, "just between us")
    wait_until(lambda: "just between us" in json.dumps(data(b, i)), PUSH, "bob to get the DM")
    assert not any(n.startswith("dm/") for n in c.rpc.request("DocNames", item_id=i)), \
        "carol's index lists the DM"
    dm = next(n for n in data(a, i) if n.startswith("dm/"))
    patch_local(c, i, "main.lua", "return function()", f'doc:open("{dm}")\nreturn function()')
    never_sees(c, i, "just between us")
    shots("T10", i, a, c)


def t11(net):
    """Presence: B sees A online in #general; A dies; B sees A offline within 2 s; never stored."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    alice = _did(a, i)
    wait_until(lambda: f"online:{alice}" in ids_on_screen(b), PUSH, "alice online on bob")
    shots("T11-online", i, b)
    a.stop()
    wait_until(lambda: f"online:{alice}" not in ids_on_screen(b), 2.0, "alice offline on bob")
    assert not any("presence" in n for n in data(b, i)), "presence entered a doc"
    shots("T11-offline", i, b)


def t12(net):
    """Typing: B shows A typing; bursts beyond the manifest rate are dropped at the node."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    alice = _did(a, i)
    a.rpc.type_text(i, "composer", "h")
    wait_until(lambda: f"typing:{alice}" in ids_on_screen(b), PUSH, "alice typing on bob")
    shots("T12", i, b)
    for n in range(30):
        a.rpc.type_text(i, "composer", "x" * n)
    time.sleep(1.5)
    got = [l for l in b.rpc.read_console(i, last=200) if l.startswith("typing from")]
    assert len(got) <= 4 * 2 + 1, f"rate limit let {len(got)} through"


def t13(net):
    """Node restarts mid-session; pushes resume without restarting the shells."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(a, i, "before restart")
    sees(b, i, "before restart")
    net.node.stop()
    net.node.start()
    time.sleep(3)  # one Listen retry interval
    send(a, i, "after restart")
    sees(b, i, "after restart")
    # A blip while a fresh peer's subscriptions are still in flight must not lose them.
    c = net.peer("carol")
    net.join(a, c, *_ws_item(a, i))
    net.node.stop()
    net.node.start()
    time.sleep(3)
    send(a, i, "carol hears this")
    sees(c, i, "carol hears this")
    shots("T13", i, a, b, c)


def t14(net):
    """An undeclared doc path in B's index is refused; C never hears of it."""
    a, b = net.peer("alice"), net.peer("bob")
    ws, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    c = net.peer("carol")
    net.join(a, c, ws, item)
    patch_local(b, i, "main.lua", "return function()",
                'doc:open("smuggled/x"):set({"v"}, doc.map({ n = 1 }))\nreturn function()')
    time.sleep(1.5)
    assert "smuggled/x" not in c.rpc.request("DocNames", item_id=i), "carol learned of it"
    assert any("rejected" in line for line in b.rpc.read_console(i)), "no rejection reported"


def t15(net):
    """B can't write under A's DID segment."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    alice = _did(a, i)
    patch_local(b, i, "main.lua", "return function()",
                f'doc:open("user/{alice}"):set({{"me"}}, doc.map({{ name = "mallory" }}))\nreturn function()')
    time.sleep(1.5)
    assert "mallory" not in json.dumps(data(a, i)), "bob renamed alice"
    assert any("rejected" in line for line in b.rpc.read_console(i)), "no rejection reported"


def _did(p, item) -> str:
    return p.rpc.request("Whoami")["did"]


def _ws_item(p, item_id):
    for ws in p.rpc.list_workspaces():
        for it in p.rpc.list_items(ws["id"]):
            if it["id"] == item_id:
                return ws, it
    raise AssertionError(f"{item_id} not found on {p.name}")


def run_once(p, item, lua):
    """Run `lua` once in this peer's app, at load, with `M` (the model) in scope — a modified
    client for hostile tests, a scripted user for the rest."""
    patch_local(p, item, "main.lua", "return function()", f"do\n{lua}\nend\nreturn function()")


def msg_id(p, item, text, channel="general"):
    wait_until(lambda: text in texts(p, item, channel), PUSH, f"{p.name} to have {text!r}")
    return next(m["id"] for m in messages(p, item, channel) if m["text"] == text)


def msg(p, item, mid, channel="general"):
    return next((m for m in messages(p, item, channel) if m["id"] == mid), None)


def rejected(p, item):
    wait_until(lambda: any("rejected" in l for l in p.rpc.read_console(item)), PUSH,
               f"{p.name} to report a rejection")


def t17(net):
    """B edits A's message: rejected and rolled back; A's text unchanged everywhere."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(a, i, "alice wrote this")
    mid = msg_id(b, i, "alice wrote this")
    run_once(b, i, f'M.channel("general"):set({{"messages", "{mid}", "text"}}, "hijacked")')
    rejected(b, i)
    wait_until(lambda: msg(b, i, mid)["text"] == "alice wrote this", PUSH, "bob to roll back")
    never_sees(a, i, "hijacked")
    shots("T17", i, a, b)


def t18(net):
    """Reactions: B's own slot is accepted; removing A's reaction is rejected."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    alice, bob = _did(a, i), _did(b, i)
    send(a, i, "react to me")
    mid = msg_id(b, i, "react to me")
    a.rpc.click(i, f"react:{mid}:👍")
    b.rpc.click(i, f"react:{mid}:👍")
    wait_until(lambda: bob in (msg(a, i, mid).get("reactions") or {}), PUSH, "bob's reaction on alice")
    shots("T18-reacted", i, a, b)
    run_once(b, i, f'M.channel("general"):delete({{"messages", "{mid}", "reactions", "{alice}"}})')
    rejected(b, i)
    time.sleep(1)
    assert alice in msg(a, i, mid)["reactions"], "alice's reaction was removed by bob"


def t19(net):
    """B can't delete A's message; a moderator can remove B's, text kept, hidden in the UI."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(a, i, "keep me")
    mid = msg_id(b, i, "keep me")
    run_once(b, i, f'M.channel("general"):delete({{"messages", "{mid}"}})')
    rejected(b, i)
    assert msg(a, i, mid) is not None, "bob deleted alice's message"
    send(b, i, "spam from bob")
    bmid = msg_id(a, i, "spam from bob")
    a.rpc.click(i, f"remove:{bmid}")  # alice claimed the node: chat admin
    wait_until(lambda: (msg(b, i, bmid) or {}).get("removed"), PUSH, "removal on bob")
    assert msg(b, i, bmid)["text"] == "spam from bob", "removal must keep the text"
    assert f"text:{bmid}" not in ids_on_screen(b), "removed text still rendered"
    shots("T19", i, a, b)


def t20(net):
    """A private group of A and B is invisible to C, even asked for by name."""
    a, b = net.peer("alice"), net.peer("bob")
    ws, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    c = net.peer("carol")
    net.join(a, c, ws, item)
    run_once(a, i, f'M.create_group("secret", {{ "{_did(b, i)}" }})')
    gid = wait_until(lambda: next((n.split("/")[1] for n in data(a, i) if n.startswith("group/")), None),
                     PUSH, "alice's group doc")
    a.rpc.click(i, f"ch:{gid}")
    send(a, i, "for bob only", )
    wait_until(lambda: "for bob only" in json.dumps(data(b, i)), PUSH, "bob to get the group message")
    assert not any(n.startswith("group/") for n in c.rpc.request("DocNames", item_id=i)), \
        "carol's index lists the group"
    run_once(c, i, f'doc:open("group/{gid}/meta")')
    never_sees(c, i, "for bob only")
    shots("T20", i, a, b, c)


def t21(net):
    """Adding C shows C the group and its history; removing C stops new messages reaching C."""
    a, b = net.peer("alice"), net.peer("bob")
    ws, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    c = net.peer("carol")
    net.join(a, c, ws, item)
    run_once(a, i, f'M.create_group("team", {{ "{_did(b, i)}" }})')
    gid = wait_until(lambda: next((n.split("/")[1] for n in data(a, i) if n.startswith("group/")), None),
                     PUSH, "alice's group doc")
    a.rpc.click(i, f"ch:{gid}")
    send(a, i, "before carol joined")
    a.rpc.click(i, f"add-member:{gid}:{_did(c, i)}")
    wait_until(lambda: f"ch:{gid}" in ids_on_screen(c), PUSH, "carol to see the group")
    c.rpc.click(i, f"ch:{gid}")
    sees(c, i, "before carol joined", channel=gid)
    shots("T21-added", i, c)
    a.rpc.click(i, f"remove-member:{gid}:{_did(c, i)}")
    time.sleep(1)
    send(a, i, "after carol left")
    never_sees(c, i, "after carol left", channel=gid)


def t22(net):
    """A message's author, at and id never change after creation."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    send(b, i, "bob's own")
    mid = msg_id(a, i, "bob's own")
    run_once(b, i, f'M.channel("general"):set({{"messages", "{mid}", "at"}}, 1)')
    rejected(b, i)
    assert msg(a, i, mid)["at"] != 1, "bob rewrote at"


def t23(net):
    """role.assign within the cone; a member can't assign; revoking cascades."""
    a, b = net.peer("alice"), net.peer("bob")
    ws, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    c = net.peer("carol")
    net.join(a, c, ws, item)
    bob, carol = _did(b, i), _did(c, i)
    try:
        c.rpc.request("AssignRole", item_id=i, did=bob, role="moderator")
        raise AssertionError("a member assigned a role")
    except Exception as e:  # noqa: BLE001
        assert "AssertionError" not in type(e).__name__, e
    a.rpc.request("AssignRole", item_id=i, did=bob, role="admin")
    b.rpc.request("AssignRole", item_id=i, did=carol, role="moderator")
    send(a, i, "carol may remove this")
    mid = msg_id(c, i, "carol may remove this")
    c.rpc.click(i, f"remove:{mid}")
    wait_until(lambda: (msg(a, i, mid) or {}).get("removed"), PUSH, "carol's removal")
    a.rpc.request("RevokeRole", item_id=i, did=bob, role="admin")
    send(a, i, "carol may not remove this")
    mid = msg_id(c, i, "carol may not remove this")
    run_once(c, i, f'M.remove("{mid}")')
    rejected(c, i)


def t24(net):
    """A ban rejects posts until it expires."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    run_once(a, i, f'M.ban("{_did(b, i)}", 3)')
    time.sleep(1)
    send(b, i, "while banned")
    rejected(b, i)
    never_sees(a, i, "while banned")
    time.sleep(3)
    send(b, i, "after the ban")
    sees(a, i, "after the ban")


def t25(net):
    """@ autocompletes from the people directory; the message stores the DID; B is notified."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item = net.share_app(a, [b], CHAT)
    i = item["id"]
    bob = _did(b, i)
    a.rpc.type_text(i, "composer", "hey @bo")
    wait_until(lambda: f"suggest:{bob}" in ids_on_screen(a), PUSH, "bob suggested")
    shots("T25-suggest", i, a)
    a.rpc.click(i, f"suggest:{bob}")
    a.rpc.key(i, "composer", "enter")
    mid = wait_until(lambda: next((m["id"] for m in messages(b, i) if bob in (m.get("mentions") or {})), None),
                     PUSH, "a message mentioning bob")
    wait_until(lambda: "mention-badge" in ids_on_screen(b), PUSH, "bob's mention badge")
    shots("T25-mention", i, b)


TESTS = {f"t{n}": globals()[f"t{n}"] for n in range(1, 26) if n != 16}


def main() -> int:
    names = [a for a in sys.argv[1:] if not a.startswith("-")] or list(TESTS)
    build()
    failed = []
    for name in names:
        started = time.monotonic()
        try:
            with Net() as net:
                TESTS[name](net)
            print(f"ok      {name}  {TESTS[name].__doc__.splitlines()[0]}  "
                  f"({time.monotonic() - started:.1f}s)", flush=True)
        except Exception as e:  # noqa: BLE001 — every failure is reported, the run goes on
            failed.append(name)
            print(f"FAILED  {name}  {type(e).__name__}: {e}", flush=True)
            if "-v" in sys.argv or len(names) == 1:
                traceback.print_exc()
    print(f"\n{len(names) - len(failed)}/{len(names)} passed" +
          (f"; failed: {' '.join(failed)}" if failed else ""))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
