"""Net — one kunki node and N shell2 desktops sharing a throwaway directory.

    with Net() as net:
        a, b = net.peer("alice"), net.peer("bob")
        ws, item = net.share_app(a, [b], CHAT)

Every peer is a `Session` whose data dir outlives a restart (`peer.restart()`), so offline
scenarios kill and revive a process without losing its vault. The node can be stopped and
started the same way. `OSVAULD_OFFSCREEN` (default 900x640 here) keeps everything windowless.
"""

import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from .session import ROOT, Session

KUNKI_BINARY = ROOT / "target" / "debug" / "kunki"
SHELL_BINARY = ROOT / "target" / "debug" / "shell2"
PASSPHRASE = "demo"


def build() -> None:
    if os.environ.get("OSVAULD_SHELL_BINARY"):
        return
    cmd = ["cargo", "build", "-p", "kunki", "-p", "shell2"]
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
    def __init__(self, shots: str | None = None, verbose: bool = False):
        self.root = Path(tempfile.mkdtemp(prefix="osvauld-net-"))
        self.shots = Path(shots or os.environ.get("OSVAULD_SHOTS") or ROOT / "target" / "e2e-shots")
        self.shots.mkdir(parents=True, exist_ok=True)
        self.verbose = verbose or bool(os.environ.get("OSVAULD_VERBOSE"))
        spec = os.environ.get("OSVAULD_OFFSCREEN", "900x640").lower().split("x")
        self.offscreen = (int(spec[0]), int(spec[1]))
        self.node = Node(self.root)
        self.peers: list[Peer] = []

    def __enter__(self) -> "Net":
        # Every desktop reads the node socket from its own environment.
        os.environ["OSVAULD_KUNKI_SOCKET"] = str(self.node.socket)
        self.node.start()
        return self

    def __exit__(self, *_exc) -> None:
        for p in self.peers:
            p.stop()
        self.node.stop()
        if not os.environ.get("OSVAULD_KEEP"):
            shutil.rmtree(self.root, ignore_errors=True)

    def peer(self, name: str) -> Peer:
        p = Peer(self, name)
        self.peers.append(p)
        p.start()
        p.rpc.signup(name, PASSPHRASE)
        return p

    def share_app(self, owner: Peer, others: list[Peer], folder: Path) -> tuple[dict, dict]:
        """`owner` claims the node and publishes an app from `folder`; each of `others` joins
        by invite and adopts it. Returns the workspace and item records."""
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
        p.rpc.claim_node(owner.rpc.invite())
        p.rpc.join_item(ws, item)
        p.rpc.open_item(item["id"])
