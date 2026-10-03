"""Search, end to end: a real shell, the chat demo, its `index.lua`, the vault-sealed index.

    python3 scripts/smoke_search.py

S1–S6 in docs/design/search.md §0. Driven through the UI (type, click) rather than by writing
docs over the bridge, because the claim is that what a *user* writes becomes findable.
"""

import os
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session

ROOT = Path(__file__).parent.parent
CHAT = ROOT / "demo_apps" / "chat"
SECRET = "marigold"  # a word that appears nowhere else, so finding it on disk means a leak


def ids(result):
    return {h["id"] for h in result["hits"]}


def message_id(s, item, text):
    """A sent message's uuid, read back from the live doc."""
    for name, value in s.rpc.read_data(item).items():
        if name.startswith("channel:"):
            for m in value.get("messages", []):
                if m["text"] == text:
                    return m["id"]
    raise AssertionError(f"no message {text!r} in the docs")


def send(s, item, text):
    s.rpc.type_text(item, "composer", text)
    s.rpc.click(item, "send")
    s.rpc.frame(2)


data_dir = tempfile.mkdtemp(prefix="osvauld-search-")

with Session(data_dir=data_dir) as s:
    s.rpc.signup("abe", "test")
    ws = s.rpc.create_workspace("search")
    item = s.rpc.create_item(ws["id"], "chat", "app")["id"]
    s.rpc.upload_folder(item, CHAT)
    s.rpc.open_item(item)
    s.rpc.frame(2)

    # S1 — what the UI writes is findable through the bridge, seeds included
    send(s, item, f"{SECRET} deploy is tomorrow")
    send(s, item, "unrelated chatter")
    sent = message_id(s, item, f"{SECRET} deploy is tomorrow")
    assert ids(s.rpc.search(item, SECRET)) == {sent}, s.rpc.search(item, SECRET)
    assert ids(s.rpc.search(item, "deploy")) == {sent, "seed-1"}
    print("S1 ok: sent and seeded messages are findable")

    # S2 — the app's own search box renders the hit
    s.rpc.type_text(item, "search", SECRET)
    s.rpc.frame(2)
    tree = str(s.rpc.dump_tree(item))
    assert f"snippet:{sent}" in tree, "the app did not render its own hit"
    s.rpc.type_text(item, "search", "")
    s.rpc.frame(1)
    print("S2 ok: the search box shows the hit")

    # S3 — edits and deletes reach the index
    s.rpc.click(item, f"edit:{sent}")
    send(s, item, "rescheduled to friday")
    assert ids(s.rpc.search(item, SECRET)) == set(), "an edited-away word still matches"
    assert ids(s.rpc.search(item, "friday")) == {sent}
    s.rpc.click(item, "del:seed-1")
    s.rpc.frame(2)
    assert "seed-1" not in ids(s.rpc.search(item, "deploy")), "a deleted message still matches"
    print("S3 ok: edit and delete are reflected")

    # put the secret back for the persistence and leak checks
    send(s, item, f"{SECRET} again")
    kept = message_id(s, item, f"{SECRET} again")

    # S4a — lock, unlock: still there
    s.rpc.lock()
    s.rpc.unlock("abe", "test")
    s.rpc.open_item(item)
    s.rpc.frame(2)
    assert ids(s.rpc.search(item, SECRET)) == {kept}
    print("S4a ok: survives lock/unlock")

# S4b — a fresh process on the same data dir: still there, and nothing was re-extracted
with Session(data_dir=data_dir) as s:
    s.rpc.unlock("abe", "test")
    s.rpc.open_item(item)
    s.rpc.frame(2)
    result = s.rpc.search(item, SECRET)
    assert ids(result) == {kept}, result
    assert result["fields_runs"] == 0, f"restart re-ran index.lua {result['fields_runs']} times"
    print("S4b ok: survives a restart without re-indexing")

    # S6 — the app's own Lua tests, in a non-persisting tab with its own index
    results = s.rpc.run_tests(item)
    failed = [r for r in results if not r["ok"]]
    assert results and not failed, failed
    print(f"S6 ok: {len(results)} app-shipped search test(s)")

# S5 — no indexed word on disk, anywhere in the store
for root, _dirs, files in os.walk(data_dir):
    for f in files:
        blob = Path(root, f).read_bytes()
        assert SECRET.encode() not in blob, f"{SECRET!r} found in plaintext in {f}"
print("S5 ok: nothing indexed reaches disk unsealed")
shutil.rmtree(data_dir, ignore_errors=True)

print("smoke ok: search")
