"""E1: binary item files through the real bridge, shell and sealed vault (blender-import.md §1)."""
from pathlib import Path
import hashlib
import os
import tempfile

from osvauld.client import BridgeError
from osvauld.session import Session, shell_binary

SOURCE = 'return function() return ui.text("binary files") end\n'
# Recognisable plaintext: if it appears anywhere under the data dir, sealing failed.
MARKER = b"OSVAULD-PLAINTEXT-MARKER-" * 64
GLB = b"glTF" + bytes(range(256)) * 40 + MARKER
LARGE = b"\0" * (13 * 1024 * 1024)  # base64 exceeds the node protocol's 16 MiB frame cap


def refuses(fn, *args, why):
    try:
        fn(*args)
    except BridgeError as e:
        return str(e)
    raise AssertionError(f"accepted: {why}")


def binaries(rpc, item):
    return {f["path"]: f for f in rpc.list_binary_files(item)}


def stored_bytes(data):
    total = b""
    for dirpath, _, names in os.walk(data):
        for n in names:
            total += (Path(dirpath) / n).read_bytes()
    return total


def check(rpc, item, expect):
    got = binaries(rpc, item)
    assert set(got) == set(expect), (got, expect)
    for path, body in expect.items():
        assert got[path]["size"] == len(body), got[path]
        assert got[path]["sha256"] == hashlib.sha256(body).hexdigest(), got[path]
        assert rpc.read_binary_file(item, path) == body, path


with tempfile.TemporaryDirectory(prefix="osvauld-binary-") as tmp:
    data = str(Path(tmp) / "store")
    with Session(shell_binary=shell_binary(), data_dir=data, offscreen=(900, 700)) as s:
        rpc = s.rpc
        rpc.signup("binary", "binary smoke passphrase")
        ws = rpc.create_workspace("Binary files")
        item = rpc.create_item(ws["id"], "Models", "app")["id"]
        rpc.write_file(item, "main.lua", SOURCE)
        rpc.write_binary_file(item, "models/a.glb", GLB)
        check(rpc, item, {"models/a.glb": GLB})
        assert rpc.list_files(item) == ["main.lua"], "binary paths are not text sources"

        # Same bytes at a second path: one stored copy.
        before = len(stored_bytes(data))
        rpc.write_binary_file(item, "models/copy.glb", GLB)
        assert len(stored_bytes(data)) - before < len(GLB) // 2, "identical bytes stored twice"

        # Refusals leave the item unchanged.
        for bad in ["", "/abs.glb", "a/../b.glb", "a//b.glb", "./a.glb"]:
            refuses(rpc.write_binary_file, item, bad, b"x", why=f"path {bad!r}")
        refuses(rpc.write_binary_file, item, "main.lua", b"x", why="binary over text source")
        refuses(rpc.write_file, item, "models/a.glb", "x", why="text over binary file")
        refuses(rpc.read_file, item, "models/a.glb", why="text read of binary file")
        error = refuses(rpc.write_binary_file, item, "big.bin", b"\0" * (64 * 1024 * 1024 + 1), why="oversize")
        assert "binary file over" in error, error
        check(rpc, item, {"models/a.glb": GLB, "models/copy.glb": GLB})

        # Open item: writes go through the live source doc and survive its next save.
        rpc.open_item(item)
        small = b"\x00\x01\x02 second revision"
        rpc.write_binary_file(item, "models/a.glb", small)
        source = rpc.read_file_versioned(item, "main.lua")
        rpc.edit_file(item, "main.lua", source["revision"], [{"old_text": "binary files", "new_text": "binary ok"}])
        check(rpc, item, {"models/a.glb": small, "models/copy.glb": GLB})
        rpc.write_binary_file(item, "models/large.bin", LARGE)
        assert rpc.read_binary_file(item, "models/large.bin") == LARGE
        rpc.lock()
        refuses(rpc.write_binary_file, item, "models/b.glb", b"x", why="locked vault")

    assert MARKER not in stored_bytes(data), "binary file stored in plaintext"
    with Session(shell_binary=shell_binary(), data_dir=data, offscreen=(900, 700)) as s:
        rpc = s.rpc
        rpc.unlock("binary", "binary smoke passphrase")
        check(rpc, item, {"models/a.glb": small, "models/copy.glb": GLB, "models/large.bin": LARGE})
        assert "binary ok" in rpc.read_file(item, "main.lua")
    print("binary files ok: sealed, deduplicated, path/kind/size/lock refusals, open-item writes, restart")
