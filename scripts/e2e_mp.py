"""e2e_mp.py — docs/design/jetpack-mp.md §0, end to end.

    python3 scripts/e2e_mp.py            # every test built so far
    python3 scripts/e2e_mp.py m1 m3      # just these

Three offscreen shells in host mode, as e2e_host.py runs them: alice's shell hosts the space and
the others dial it over iroh on loopback. Screenshots land in $OSVAULD_SHOTS (default
target/e2e-shots) as `M<n>-<peer>.png`.

Messages cross real sockets, so their arrival is waited for in wall-clock time, not counted
in virtual frames: the offscreen clock only moves when asked, the network does not wait for it.
"""

import base64
import sys
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.net import Net, build, wait_until  # noqa: E402

PROBE = Path(__file__).parent / "e2e_apps" / "net_probe"
SOON = 5.0


def find(tree, el_id):
    if tree.get("id") == el_id:
        return tree
    for child in tree.get("children", []):
        found = find(child, el_id)
        if found:
            return found
    return None


def shown(p, item, el_id) -> str:
    p.rpc.frame(1)
    node = find(p.rpc.dump_tree(item), el_id)
    return (node or {}).get("text") or ""


def count(p, item) -> int:
    return int(shown(p, item, "count").split()[-1])


def say(p, item, text):
    p.rpc.type_text(item, "say", text)
    p.rpc.key(item, "say", "enter")


def short(p) -> str:
    """The first 8 hex digits of p's device key, as the probe shows a peer."""
    device = p.rpc.whoami()["device"]
    return base64.urlsafe_b64decode(device + "=" * (-len(device) % 4)).hex()[:8]


def shots(label, item, *peers):
    for p in peers:
        p.shot(label, item)


def setup(net, names):
    peers = [net.peer(n) for n in names]
    _, item, _ = net.share_space(peers[0], peers[1:], PROBE)
    i = item["id"]
    # Everyone hears everyone before a test starts.
    for p in peers:
        wait_until(lambda p=p: shown(p, i, "peers").count(",") == len(peers) - 1, SOON,
                   f"{p.name} to see {len(peers)} peers")
    return peers, i


# ── tests ────────────────────────────────────────────────────────────────────


def m1(net):
    """A channel message reaches the others, from A, and leaves nothing behind."""
    (a, b, c), i = setup(net, ["alice", "bob", "carol"])
    before = [p.rpc.read_data(i) for p in (a, b, c)]
    items_before = [p.rpc.list_items(p.rpc.list_workspaces()[0]["id"]) for p in (a, b, c)]
    say(a, i, "hello")
    for p in (b, c):
        wait_until(lambda p=p: f"{short(a)}:hello" in shown(p, i, "got"), SOON, f"{p.name} to hear alice")
    assert count(a, i) == 0, "a message is not sent back to its sender"
    say(b, i, "back")
    wait_until(lambda: f"{short(b)}:back" in shown(a, i, "got"), SOON, "alice (the host) to hear bob")
    for p, d, its in zip((a, b, c), before, items_before):
        assert p.rpc.read_data(i) == d, f"a channel message changed {p.name}'s docs"
        assert p.rpc.list_items(p.rpc.list_workspaces()[0]["id"]) == its, f"{p.name}'s items changed"
    b.rpc.click(i, "bad")
    assert any('no channel "nope"' in line for line in b.rpc.read_console(i)), b.rpc.read_console(i)
    shots("M1", i, a, b, c)


def m2(net):
    """Over the manifest's rate: the others get at most the cap; the sender's console says so."""
    (a, b), i = setup(net, ["alice", "bob"])
    b.rpc.click(i, "flood")  # 50 at once on a 10/s channel
    wait_until(lambda: count(a, i) >= 10, SOON, "alice to hear the burst")
    time.sleep(1.0)
    got = count(a, i)
    assert got <= 11, f"alice heard {got} of a 10/s burst"
    assert any("net: over rate" in line for line in b.rpc.read_console(i)), b.rpc.read_console(i)
    shots("M2", i, a, b)


def m3(net):
    """Presence: B and C see A; C is killed and A and B drop it within 2 s."""
    (a, b, c), i = setup(net, ["alice", "bob", "carol"])
    for p in (b, c):
        assert short(a) in shown(p, i, "peers"), f"{p.name} does not see alice"
    gone = short(c)
    c.session.process.kill()  # SIGKILL: no FIN, only the missing beats tell
    started = time.monotonic()
    for p in (a, b):
        wait_until(lambda p=p: gone not in shown(p, i, "peers"), 4.0, f"{p.name} to drop carol")
    took = time.monotonic() - started
    assert took < 3.0, f"carol took {took:.1f}s to drop (2 s silence + a beat)"
    shots("M3", i, a, b)


TESTS = {"m1": m1, "m2": m2, "m3": m3}


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
