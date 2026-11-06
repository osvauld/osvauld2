"""e2e_owned.py — docs/design/owned-entries.md §0, the shell-driven half (O1, O2, O6, O7).

    python3 scripts/e2e_owned.py            # every test
    python3 scripts/e2e_owned.py o2         # just this one

Two offscreen shells: alice hosts the space from her own device, bob joins by link. The chat
app is the artifact: its manifest owns `messages/*` with a moderator override, so "an entry"
is a message and "its creator" is whoever posted it. The host-side half (O3–O5, O8) is in
`courier/src/sync/tests.rs` — Python cannot forge a Loro change.
"""

import sys
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.net import Net, build, wait_until  # noqa: E402
from e2e_chat_sync import CHAT, PUSH, messages, send, sees, texts, shots, patch_local  # noqa: E402


def message_by(p, item, text) -> dict:
    found = [m for m in messages(p, item) if m["text"] == text]
    assert found, f"{p.name} has no message {text!r}: {texts(p, item)}"
    return found[0]


def buttons(p, prefix) -> set[str]:
    return {r["id"] for r in p.rpc.rects() if r["id"].startswith(prefix)}


def gone(p, item, text, timeout=PUSH):
    wait_until(lambda: text not in texts(p, item), timeout, f"{p.name} to see {text!r} gone")


def o1(net):
    """Each sees the other's real name, and edit/delete only on their own messages."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item, _ = net.share_space(a, [b], CHAT)
    i = item["id"]
    send(a, i, "alice posts")
    sees(b, i, "alice posts")
    send(b, i, "bob posts")
    sees(a, i, "bob posts")
    assert message_by(b, i, "alice posts")["author"] == "alice"
    assert message_by(a, i, "bob posts")["author"] == "bob"
    assert "me" not in {m["author"] for m in messages(a, i)}

    alices, bobs = message_by(b, i, "alice posts")["id"], message_by(b, i, "bob posts")["id"]
    on_bob = buttons(b, "del:")
    assert f"del:{bobs}" in on_bob, f"bob has no delete on his own message: {on_bob}"
    assert f"del:{alices}" not in on_bob, f"bob has a delete on alice's message: {on_bob}"
    on_alice = buttons(a, "del:")
    # The owner moderates: she may delete anyone's.
    assert {f"del:{bobs}", f"del:{alices}"} <= on_alice, on_alice
    shots("O1", i, a, b)


def o2(net):
    """A modified client that tries anyway is refused inside its own shell; nothing is pushed."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item, _ = net.share_space(a, [b], CHAT)
    i = item["id"]
    send(a, i, "alice posts")
    sees(b, i, "alice posts")
    # Bob's copy of the app draws a delete button on every message.
    patch_local(b, i, "main.lua", "if M.can_delete(m) then", "if true then")
    alices = message_by(b, i, "alice posts")["id"]
    wait_until(lambda: f"del:{alices}" in buttons(b, "del:"), PUSH, "bob's patched delete button")
    b.rpc.click(i, f"del:{alices}")
    wait_until(
        lambda: any("may not delete" in line for line in b.rpc.read_console(i)),
        PUSH, "bob's console to report the refusal",
    )
    time.sleep(1.0)
    assert "alice posts" in texts(b, i), "the refused delete must not touch bob's own copy"
    assert "alice posts" in texts(a, i), "nothing may have reached alice"
    shots("O2", i, a, b)


def o6(net):
    """Owners still can: bob deletes his own message and both converge."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item, _ = net.share_space(a, [b], CHAT)
    i = item["id"]
    send(b, i, "bob posts")
    sees(a, i, "bob posts")
    send(b, i, "bob regrets")
    sees(a, i, "bob regrets")
    regret = message_by(b, i, "bob regrets")["id"]
    b.rpc.click(i, f"del:{regret}")
    gone(b, i, "bob regrets")
    gone(a, i, "bob regrets")
    assert texts(a, i) == texts(b, i)
    shots("O6", i, a, b)


def o7(net):
    """The override: the owner deletes bob's message and it syncs."""
    a, b = net.peer("alice"), net.peer("bob")
    _, item, _ = net.share_space(a, [b], CHAT)
    i = item["id"]
    send(b, i, "bob posts")
    sees(a, i, "bob posts")
    bobs = message_by(a, i, "bob posts")["id"]
    a.rpc.click(i, f"del:{bobs}")
    gone(a, i, "bob posts")
    gone(b, i, "bob posts")
    shots("O7", i, a, b)


TESTS = {"o1": o1, "o2": o2, "o6": o6, "o7": o7}


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
