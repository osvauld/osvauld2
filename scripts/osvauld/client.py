"""Bridge — one connection per request, speaking osvauld-rpc framing.

Request:  {"op": "...", ...fields}          (4-byte BE length prefix + JSON)
Response: {"status": "ok", "result": ...} | {"status": "err", "message": "..."}
"""

import base64
import json
import os
import socket
from typing import Any


class BridgeError(RuntimeError):
    """The shell answered with an error response."""


def format_tree(node: Any, depth: int = 0) -> str:
    """`dump_tree`'s JSON as one line per element: kind, #id, [handlers], "text".

    Ported from the `open` example's `--tree` when that driver was deleted (design/six-apps.md
    §7). JSON is the right wire format and the wrong thing to read, and two of these diff
    cleanly — which is how you answer "did anything actually move" without eyeballing braces.
    """
    if not isinstance(node, dict):
        return ""
    out = ["  " * depth + node.get("kind", "?")]
    if node.get("id"):
        out[0] += f"#{node['id']}"
    if node.get("handlers"):
        out[0] += "  [" + " ".join(node["handlers"]) + "]"
    text = node.get("text") or ""
    if text:
        clipped = text if len(text) <= 48 else text[:48] + "\u2026"
        out[0] += f"  {clipped!r}"
    for child in node.get("children", []):
        out.append(format_tree(child, depth + 1))
    return "\n".join(p for p in out if p)


class Bridge:
    def __init__(self, socket_path: str, timeout: float = 30.0):
        self.socket_path = socket_path
        self.timeout = timeout

    def request(self, op: str, **params: Any) -> Any:
        """Send one request on a fresh connection; return `result` or raise BridgeError."""
        payload = json.dumps({"op": op, **params}).encode("utf-8")
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(self.timeout)
            sock.connect(self.socket_path)
            sock.sendall(len(payload).to_bytes(4, "big") + payload)
            resp = self._read_msg(sock)
        body = json.loads(resp.decode("utf-8"))
        if body.get("status") == "err":
            raise BridgeError(f"{op}: {body.get('message')}")
        return body.get("result")

    @staticmethod
    def _read_msg(sock: socket.socket) -> bytes:
        header = Bridge._recv_exact(sock, 4)
        (length,) = int.from_bytes(header, "big"),  # single-element tuple unpack
        return Bridge._recv_exact(sock, length)

    @staticmethod
    def _recv_exact(sock: socket.socket, n: int) -> bytes:
        buf = b""
        while len(buf) < n:
            chunk = sock.recv(n - len(buf))
            if not chunk:
                raise ConnectionError("bridge closed mid-message")
            buf += chunk
        return buf

    # ── vocabulary: one method per request ──────────────────────────────────────
    def ping(self) -> str:
        return self.request("Ping")

    def list_accounts(self) -> list:
        return self.request("ListAccounts")

    def list_workspaces(self) -> list:
        return self.request("ListWorkspaces")

    def signup(self, name: str, passphrase: str) -> dict:
        """Create an account; returns {"mnemonic": "…"} — shown once, save it now."""
        return self.request("Signup", name=name, passphrase=passphrase)

    def unlock(self, account: str, passphrase: str) -> str:
        """Open the vault (account = id or name). Wrong passphrase raises BridgeError."""
        return self.request("Unlock", account=account, passphrase=passphrase)

    def lock(self) -> str:
        return self.request("Lock")

    def create_workspace(self, name: str) -> dict:
        return self.request("CreateWorkspace", name=name)

    def list_items(self, ws_id: str) -> list:
        return self.request("ListItems", ws_id=ws_id)

    def create_item(self, ws_id: str, name: str, kind: str) -> dict:
        return self.request("CreateItem", ws_id=ws_id, name=name, kind=kind)

    def open_item(self, item_id: str) -> str:
        return self.request("OpenItem", item_id=item_id)

    def list_files(self, item_id: str) -> list:
        return self.request("ListFiles", item_id=item_id)

    def read_file(self, item_id: str, path: str) -> str:
        return self.request("ReadFile", item_id=item_id, path=path)

    def read_file_versioned(self, item_id: str, path: str) -> dict:
        """Read source plus the opaque revision required by edit_file."""
        return self.request("ReadFileVersioned", item_id=item_id, path=path)

    def edit_file(
        self,
        item_id: str,
        path: str,
        expected_revision: str,
        edits: list[dict[str, str]],
    ) -> dict:
        """Apply exact old_text/new_text replacements to the existing LoroText."""
        return self.request(
            "EditFile",
            item_id=item_id,
            path=path,
            expected_revision=expected_revision,
            edits=edits,
        )

    def write_file(self, item_id: str, path: str, content: str) -> str:
        return self.request("WriteFile", item_id=item_id, path=path, content=content)

    def reload_item(self, item_id: str) -> str:
        return self.request("ReloadItem", item_id=item_id)

    def dump_tree(self, item_id: str) -> dict:
        return self.request("DumpTree", item_id=item_id)

    def click(self, item_id: str, el_id: str) -> str:
        return self.request("Click", item_id=item_id, el_id=el_id)

    def type_text(self, item_id: str, el_id: str, content: str) -> str:
        return self.request("Type", item_id=item_id, el_id=el_id, content=content)

    def key(self, item_id: str, el_id: str, key: str) -> str:
        return self.request("Key", item_id=item_id, el_id=el_id, key=key)

    def read_data(self, item_id: str) -> dict:
        """Every open runtime-data doc as {name: deep JSON} — live, pre-flush."""
        return self.request("AppDataGet", item_id=item_id)

    def frame(self, count: int = 1) -> dict:
        """Paint `count` frames offscreen, each 1/60s. Returns {"clock", "frames"}."""
        return self.request("Frame", count=count)

    def advance(self, secs: float) -> dict:
        """Jump the virtual clock, then paint once so the app notices. Offscreen only."""
        return self.request("Advance", secs=secs)

    def keyboard(self, code: str | None, key: str, down: bool, *, repeat: bool = False,
                 shift: bool = False, ctrl: bool = False, alt: bool = False,
                 super_: bool = False) -> dict:
        """Offscreen key down/up through live keyboard eligibility, not Click-by-id."""
        return self.request("Keyboard", code=code, key=key, down=down, repeat=repeat,
                            shift=shift, ctrl=ctrl, alt=alt, **{"super": super_})

    def rects(self) -> list[dict]:
        """Where every reachable element is: [{id, x, y, w, h, hits}]. Clipped ones are absent."""
        return self.request("Rects")["rects"]

    def move_to(self, x: float, y: float) -> list[dict]:
        """Move the pointer; returns what is under it afterwards (empty list = a miss)."""
        return self.request("PointerMove", x=x, y=y)["rects"]

    def press(self) -> list[dict]:
        return self.request("PointerPress")["rects"]

    def release(self) -> list[dict]:
        return self.request("PointerRelease")["rects"]

    def click_at(self, x: float, y: float) -> list[dict]:
        """Move, press, release — a click at a point, through the real hit-test.

        Composed here rather than as one op because the three are orthogonal and the round trip
        is a local socket. `drag` is not composable this way: its interpolation has to happen
        runtime-side to be timed like a real gesture.
        """
        under = self.move_to(x, y)
        self.press()
        self.release()
        return under

    def centre_of(self, el_id: str) -> tuple[float, float]:
        """The aim point for an element, from `rects`. Raises if it is not reachable."""
        for r in self.rects():
            if r["id"] == el_id:
                return r["x"] + r["w"] / 2, r["y"] + r["h"] / 2
        raise BridgeError(f"{el_id!r} is not reachable; it may exist but be clipped")

    def drag(self, frm: tuple[float, float], to: tuple[float, float], steps: int = 8) -> list[dict]:
        return self.request("Drag", **{"from": list(frm), "to": list(to), "steps": steps})["rects"]

    def read_console(self, item_id: str, last: int = 100) -> list[str]:
        return self.request("ReadConsole", item_id=item_id, last=last)

    def screenshot(
        self,
        item_id: str,
        width: float | None = None,
        height: float | None = None,
        scale: float | None = None,
    ) -> dict:
        return self.request(
            "Screenshot", item_id=item_id, width=width, height=height, scale=scale
        )

    def save_screenshot(self, item_id: str, path, **size) -> dict:
        """Capture a PNG to `path`; return its dimensions without the base64 payload."""
        from pathlib import Path

        result = self.screenshot(item_id, **size)
        Path(path).write_bytes(base64.b64decode(result["png_base64"], validate=True))
        return {k: result[k] for k in ("width_px", "height_px")}

    def upload_folder(self, item_id: str, root) -> list:
        """Mirror the shell's GUI upload: walk `root`, keep *.lua/*.osv (skipping dotfiles),
        one WriteFile per file. Root main.lua is required — the same contract as the picker."""
        from pathlib import Path
        root = Path(root)
        files = []
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames[:] = sorted(d for d in dirnames if not d.startswith("."))
            for fn in sorted(filenames):
                if fn.startswith(".") or os.path.splitext(fn)[1] not in (".lua", ".osv"):
                    continue
                full = Path(dirpath) / fn
                files.append((full.relative_to(root).as_posix(), full.read_text(encoding="utf-8")))
        if not any(p == "main.lua" for p, _ in files):
            raise BridgeError(f"no main.lua in {root}")
        for path, body in sorted(files):
            self.write_file(item_id, path, body)
        return [p for p, _ in sorted(files)]
