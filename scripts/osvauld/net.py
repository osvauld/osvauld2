"""Net — N shell2 desktops sharing a throwaway directory, with or without a kunki node.

    with Net() as net:                      # host mode: alice's shell is the host (laptop-host.md)
        a, b = net.peer("alice"), net.peer("bob")
        ws, item, link = net.share_space(a, [b], CHAT)

    with Net(node=True) as net:             # node mode: one kunki process, the pre-pivot path
        ws, item = net.share_app(a, [b], CHAT)

Every peer is a `Session` whose data dir outlives a restart (`peer.restart()`), so offline
scenarios kill and revive a process without losing its vault. In node mode the node can be
stopped and started the same way. `OSVAULD_OFFSCREEN` (default 900x640 here) keeps everything
windowless.

Host mode sets `OSVAULD_NET=local` for every shell: local-network discovery only, no relay —
two shells on one machine find each other by endpoint id without the internet. It unsets
`OSVAULD_KUNKI_SOCKET`: there is no socket, and a shell that still looks for one fails loudly.
`OSVAULD_E2E_HOST=1` makes `Net()` host mode even for scripts written against node mode
(`e2e_chat_sync.py`), and routes their `share_app` through `share_space` — that is H6.
"""

import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from .session import ROOT, Session

KUNKI_BINARY = Path(os.environ.get("OSVAULD_KUNKI_BINARY") or ROOT / "target" / "debug" / "kunki")
SHELL_BINARY = ROOT / "target" / "debug" / "shell2"
PASSPHRASE = "demo"


def build() -> None:
    if os.environ.get("OSVAULD_SHELL_BINARY"):
        return
    cmd = ["cargo", "build", "-p", "kunki", "-p", "shell2"]
    # Builds go through the task manager (AGENTS.md § Commands); a bare cargo here would put a
    # target/ on the home partition.
    if shutil.which("loadtable"):
        cmd = ["loadtable", "run", "--who", "e2e-net", "--pin", "--", *cmd, "-j", "2"]
    print("building:", " ".join(cmd), flush=True)
    if subprocess.run(cmd, cwd=ROOT).returncode != 0:
        raise SystemExit("build failed")


def wait_until(pred, timeout: float, what: str, every: float = 0.1):
    deadline = time.monotonic() + timeout
    while True:
        got = pred()
        if got:
            return got
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out after {timeout}s waiting for {what}")
        time.sleep(every)


class Node:
    """The pre-pivot kunki: a trust root with a boot ticket, reached over a unix socket."""

    def __init__(self, root: Path):
        self.dir = root / "node"
        self.socket = root / "kunki.sock"
        self.ticket: str | None = None
        self.proc: subprocess.Popen | None = None

    def start(self) -> None:
        env = {
            **os.environ,
            "OSVAULD_KUNKI_DIR": str(self.dir),
            "OSVAULD_KUNKI_PASSPHRASE": PASSPHRASE,
            "OSVAULD_KUNKI_SOCKET": str(self.socket),
        }
        self.proc = subprocess.Popen(
            [str(KUNKI_BINARY)], env=env, stdout=subprocess.PIPE, text=True
        )
        # The boot ticket is the one stdout line kunki prints; reading it is the boot wait.
        line = self.proc.stdout.readline().strip()
        if not line:
            raise RuntimeError("kunki exited before printing a ticket")
        self.ticket = self.ticket or line

    def stop(self) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            self.proc.wait(timeout=5)
        self.proc = None


class Headless:
    """A headless host (laptop-host.md step 7): `kunki --data DIR host --join LINK --accept WS`,
    a member identity that joins a space by link and accepts appointment as its host. Prints
    `did: <did>` on stdout once it has joined; that line is the boot wait."""

    def __init__(self, root: Path, name: str, link: str, ws_id: str):
        self.dir = root / name
        self.link = link
        self.ws_id = ws_id
        self.did: str | None = None
        self.proc: subprocess.Popen | None = None

    def start(self) -> None:
        env = {**os.environ, "OSVAULD_KUNKI_PASSPHRASE": PASSPHRASE}
        argv = [str(KUNKI_BINARY), "--data", str(self.dir), "host",
                "--join", self.link, "--accept", self.ws_id]
        self.proc = subprocess.Popen(argv, env=env, stdout=subprocess.PIPE, text=True)
        line = self.proc.stdout.readline().strip()
        if not line.startswith("did: "):
            raise RuntimeError(f"headless host did not join: {line!r}")
        self.did = line[len("did: "):]

    def stop(self) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            self.proc.wait(timeout=5)
        self.proc = None


class Peer:
    def __init__(self, net: "Net", name: str):
        self.net = net
        self.name = name
        self.session = Session(
            shell_binary=SHELL_BINARY,
            data_dir=str(net.root / name),
            socket_path=str(net.root / f"{name}.sock"),
            offscreen=net.offscreen,
            show_shell_output=net.verbose,
        )
        self.did: str | None = None

    @property
    def rpc(self):
        return self.session.rpc

    def start(self) -> None:
        self.session.start()

    def stop(self) -> None:
        # `Session.close` removes its own tmp dir, never the data dir it was handed.
        self.session.close()

    def restart(self) -> None:
        self.stop()
        self.start()
        self.rpc.unlock(self.name, PASSPHRASE)

    def shot(self, label: str, item_id: str) -> Path:
        self.rpc.frame(2)
        path = self.net.shots / f"{label}-{self.name}.png"
        self.rpc.save_screenshot(item_id, path)
        return path


class Net:
    def __init__(self, shots: str | None = None, verbose: bool = False, node: bool = False):
        self.root = Path(tempfile.mkdtemp(prefix="osvauld-net-"))
        self.shots = Path(shots or os.environ.get("OSVAULD_SHOTS") or ROOT / "target" / "e2e-shots")
        self.shots.mkdir(parents=True, exist_ok=True)
        self.verbose = verbose or bool(os.environ.get("OSVAULD_VERBOSE"))
        spec = os.environ.get("OSVAULD_OFFSCREEN", "900x640").lower().split("x")
        self.offscreen = (int(spec[0]), int(spec[1]))
        self.host_mode = bool(os.environ.get("OSVAULD_E2E_HOST")) or not node
        self.node = None if self.host_mode else Node(self.root)
        self.peers: list[Peer] = []
        self.headless: list[Headless] = []

    def __enter__(self) -> "Net":
        if self.node:
            # Every desktop reads the node socket from its own environment.
            os.environ["OSVAULD_KUNKI_SOCKET"] = str(self.node.socket)
            self.node.start()
        else:
            os.environ.pop("OSVAULD_KUNKI_SOCKET", None)
            os.environ["OSVAULD_NET"] = "local"
        return self

    def __exit__(self, *_exc) -> None:
        for p in self.peers:
            p.stop()
        for h in self.headless:
            h.stop()
        if self.node:
            self.node.stop()
        if not os.environ.get("OSVAULD_KEEP"):
            shutil.rmtree(self.root, ignore_errors=True)

    def peer(self, name: str) -> Peer:
        p = Peer(self, name)
        self.peers.append(p)
        p.start()
        p.rpc.signup(name, PASSPHRASE)
        return p

    # ── host mode ────────────────────────────────────────────────────────────

    def share_space(self, owner: Peer, others: list[Peer], folder: Path) -> tuple[dict, dict, str]:
        """`owner` makes a space with an app from `folder` and shares it; each of `others` joins
        by the link and opens the app it finds there — no ids cross the test. Returns the
        workspace, the item (as the owner sees them) and the link."""
        ws = owner.rpc.create_workspace("team")
        item = owner.rpc.create_item(ws["id"], folder.name, "app")
        owner.rpc.upload_folder(item["id"], folder)
        owner.rpc.open_item(item["id"])
        link = owner.rpc.share_space(ws["id"])
        for p in others:
            self.join_space(p, link, item["name"])
        return ws, item, link

    def join_space(self, p: Peer, link: str, item_name: str) -> dict:
        """Join by link and open the item called `item_name` — the item the space lists, not an
        id the test knew. Returns the joined item as this peer sees it."""
        joined = p.rpc.join_space(link)
        found = [it for it in joined["items"] if it["name"] == item_name]
        assert found, f"{p.name} joined but the space lists no {item_name!r}: {joined}"
        p.rpc.open_item(found[0]["id"])
        return found[0]

    def stop_host(self, owner: Peer) -> None:
        """Take the space's host down: the node in node mode, the owner's shell in host mode."""
        if self.node:
            self.node.stop()
        else:
            owner.stop()

    def start_host(self, owner: Peer, item_id: str) -> None:
        """Bring it back. In host mode the owner's shell restarts and reopens the item, which is
        what serves the space again."""
        if self.node:
            self.node.start()
        else:
            owner.restart()
            owner.rpc.open_item(item_id)

    def headless_host(self, name: str, link: str, ws_id: str) -> Headless:
        h = Headless(self.root, name, link, ws_id)
        self.headless.append(h)
        h.start()
        return h

    # ── node mode (pre-pivot), kept for e2e_chat_sync.py ─────────────────────

    def share_app(self, owner: Peer, others: list[Peer], folder: Path) -> tuple[dict, dict]:
        """`owner` claims the node and publishes an app from `folder`; each of `others` joins
        by invite and adopts it. Returns the workspace and item records. In host mode this is
        `share_space` with the link dropped, so the chat tests run unchanged over sessions."""
        if self.host_mode:
            ws, item, _link = self.share_space(owner, others, folder)
            return ws, item
        owner.rpc.claim_node(self.node.ticket)
        ws = owner.rpc.create_workspace("team")
        item = owner.rpc.create_item(ws["id"], folder.name, "app")
        owner.rpc.upload_folder(item["id"], folder)
        owner.rpc.open_item(item["id"])
        owner.rpc.push_src(item["id"])
        owner.rpc.publish_all()
        for p in others:
            self.join(owner, p, ws, item)
        return ws, item

    def join(self, owner: Peer, p: Peer, ws: dict, item: dict) -> None:
        if self.host_mode:
            self.join_space(p, owner.rpc.share_space(ws["id"]), item["name"])
            return
        p.rpc.claim_node(owner.rpc.invite())
        p.rpc.join_item(ws, item)
        p.rpc.open_item(item["id"])
