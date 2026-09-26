"""demo_sync.py — one kunki node, two shell2 desktops, throwaway state: the "two users, one
node" sync demo (docs/status.md's "sync, subscribe, and push land" entry has the details of
what this is exercising).

    python3 scripts/demo_sync.py

Fully automated over the bridge, start to finish: signup, claim, invite, publish, join, and a
note typed on each side to prove the other one receives it — no manual clicking. Real windows
by default, one per desktop, so the person running it can watch the note lists converge live.
Set OSVAULD_OFFSCREEN=WxH (same convention as scripts/smoke.py) to run both headless instead.

Make sure both windows are actually visible once they open. Under a tabbed/stacked window
manager (sway and similar), a newly opened window can land as a hidden tab behind another —
Wayland only sends frame callbacks to the visible one, and a backgrounded shell2 window can
then take many seconds (not milliseconds) to answer an RPC, purely because its event loop is
waiting on a callback the compositor isn't sending it yet. That's the compositor being correct
(it saves power by not driving invisible surfaces), not a bug in this demo or in shell2 — it
just needs both windows on screen at once to be responsive. Confirmed directly: the exact same
run drops from tens of seconds per step to milliseconds once both are visible.

`join_item` (bob adopting a workspace/item alice already published) is this script's own job
today, not the shell's: there is no node-side "what do you hold" discovery query yet, so the
script hands bob the exact ids straight from alice's own create_workspace/create_item replies.
"""

import atexit
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from tempfile import mkdtemp

ROOT = Path(__file__).parent.parent
sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, offscreen_default, shell_binary  # noqa: E402

# Release, not debug — debug shell2 is noticeably sluggish under the runtime's own render loop,
# and a slow UI thread is exactly what would make the sync timing below look wrong. Same
# convention scripts/upload_app.py already uses via `shell_binary(release=True)`.
KUNKI_BINARY = ROOT / "target" / "release" / "kunki"
APP_FOLDER = ROOT / "demo_apps" / "scratch"
# A local edit reaches the node immediately (the post-flush sync trigger in shell2/src/main.rs)
# and a subscribed peer receives it over its standing Listen connection just as fast — this is
# generous headroom for process/socket scheduling, not a wait for any periodic tick.
PUSH_TIMEOUT = 5.0


def build() -> None:
    cmd = ["cargo", "build", "--release", "-p", "kunki", "-p", "shell2"]
    print("building (release):", " ".join(cmd), flush=True)
    if subprocess.run(cmd, cwd=ROOT).returncode != 0:
        raise SystemExit("build failed")


def spawn_node(demo_dir: Path) -> tuple[subprocess.Popen, Path, str]:
    socket = demo_dir / "kunki.sock"
    env = {
        **os.environ,
        "OSVAULD_KUNKI_DIR": str(demo_dir / "node"),
        "OSVAULD_KUNKI_PASSPHRASE": "demo",
        "OSVAULD_KUNKI_SOCKET": str(socket),
    }
    proc = subprocess.Popen(
        [str(KUNKI_BINARY)], env=env, stdout=subprocess.PIPE, stderr=None, text=True
    )
    # The ticket is the one stdout line kunki ever prints (kunki/src/bridge.rs's own doc
    # comment) — reading it is how this waits for boot, not a guessed sleep.
    ticket = proc.stdout.readline().strip()
    if not ticket:
        raise RuntimeError("kunki exited before printing a ticket")
    return proc, socket, ticket


def add_note(session: Session, item_id: str, text: str) -> None:
    """Drive demo_apps/scratch's own composer (`draft` input, `enter` submits) — an app-level
    write, which only Click/Type/Key can reach; the automation bridge's data sense is
    read-only on purpose (main.rs's own `AppDataGet` doc comment)."""
    session.rpc.type_text(item_id, "draft", text)
    session.rpc.key(item_id, "draft", "enter")


def notes_contain(session: Session, item_id: str, text: str) -> bool:
    return text in json.dumps(session.rpc.read_data(item_id))


def wait_for_note(session: Session, item_id: str, text: str, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if notes_contain(session, item_id, text):
            return
        time.sleep(0.25)
    raise RuntimeError(f"{text!r} never showed up in {item_id}'s notes within {timeout}s")


class Clock:
    """Prints how long each step actually took, and the running total since the clock was
    started (after the one-time build, which `main` times separately) — the two numbers
    together are what tell "one slow step" apart from "every step adds up"."""

    def __init__(self) -> None:
        self.start = time.monotonic()

    def step(self, label: str, fn):
        t0 = time.monotonic()
        result = fn()
        now = time.monotonic()
        print(f"[+{now - self.start:6.2f}s] {label} ({now - t0:.2f}s)", flush=True)
        return result


def main() -> None:
    build()
    demo_dir = Path(mkdtemp(prefix="osvauld-demo-"))
    print(f"demo state under: {demo_dir} (rm -rf when done)")

    node_proc, kunki_socket, ticket = spawn_node(demo_dir)
    atexit.register(lambda: node_proc.terminate())
    # Both desktops read this from their own environment (`kunki::bridge::socket_path`'s
    # convention, shared by `shell2::node`) — set once here, inherited by both `Session`s below.
    os.environ["OSVAULD_KUNKI_SOCKET"] = str(kunki_socket)

    offscreen = offscreen_default()
    binary = shell_binary(release=True)
    alice = Session(
        shell_binary=binary,
        data_dir=str(demo_dir / "alice"),
        socket_path=str(demo_dir / "alice.sock"),
        offscreen=offscreen,
        show_shell_output=True,
    )
    bob = Session(
        shell_binary=binary,
        data_dir=str(demo_dir / "bob"),
        socket_path=str(demo_dir / "bob.sock"),
        offscreen=offscreen,
        show_shell_output=True,
    )
    clock = Clock()
    clock.step("alice window ready", alice.start)
    clock.step("bob window ready", bob.start)
    # Same reasoning as `node_proc`'s own atexit registration above: the `finally` block below
    # covers the normal exit paths, but a signal or an exception before this point shouldn't
    # leave either desktop process orphaned either.
    atexit.register(alice.close)
    atexit.register(bob.close)

    try:
        clock.step("alice signup", lambda: alice.rpc.signup("alice", "demo"))
        clock.step("alice claim_node", lambda: alice.rpc.claim_node(ticket))
        ws = clock.step(
            "alice create_workspace", lambda: alice.rpc.create_workspace("demo-workspace")
        )
        item = clock.step(
            "alice create_item", lambda: alice.rpc.create_item(ws["id"], "scratch", "app")
        )
        clock.step(
            "alice upload_folder", lambda: alice.rpc.upload_folder(item["id"], APP_FOLDER)
        )
        clock.step("alice open_item", lambda: alice.rpc.open_item(item["id"]))
        clock.step("alice push_src", lambda: alice.rpc.push_src(item["id"]))

        clock.step("bob signup", lambda: bob.rpc.signup("bob", "demo"))
        invite_ticket = clock.step("alice invite", alice.rpc.invite)
        clock.step("bob claim_node", lambda: bob.rpc.claim_node(invite_ticket))

        clock.step("alice publish_all", alice.rpc.publish_all)
        clock.step("bob join_item", lambda: bob.rpc.join_item(ws, item))
        clock.step("bob open_item", lambda: bob.rpc.open_item(item["id"]))

        clock.step(
            "alice note -> bob receives",
            lambda: (
                add_note(alice, item["id"], "hello from alice"),
                wait_for_note(bob, item["id"], "hello from alice", PUSH_TIMEOUT),
            ),
        )
        clock.step(
            "bob note -> alice receives",
            lambda: (
                add_note(bob, item["id"], "hello from bob"),
                wait_for_note(alice, item["id"], "hello from bob", PUSH_TIMEOUT),
            ),
        )

        print("\nsync verified both directions. Both windows stay open — Ctrl-C here to stop.")
    except Exception as e:
        alice.close()
        bob.close()
        node_proc.terminate()
        raise SystemExit(f"demo failed: {e}")

    try:
        while alice.process.poll() is None and bob.process.poll() is None:
            time.sleep(0.5)
    except KeyboardInterrupt:
        pass
    finally:
        alice.close()
        bob.close()
        node_proc.terminate()


if __name__ == "__main__":
    main()
