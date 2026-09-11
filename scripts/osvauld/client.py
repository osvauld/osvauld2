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
