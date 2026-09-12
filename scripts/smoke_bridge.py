"""End-to-end smoke: spawn a fresh shell, drive it over the bridge, tear it down.

    python3 scripts/smoke_bridge.py

Exercises the wired families: transport + Msg::Rpc, the read-only trio, and
the auth round-trip — signup (mnemonic rides the response), unlock by name
and by did, wrong passphrase refused, lock, and the locked-vault refusal.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.client import BridgeError
from osvauld.session import Session

with Session() as s:
    assert s.rpc.ping() == "pong"
    assert s.rpc.list_accounts() == []

    # workspaces refuse while locked — the correct answer, asserted
    try:
        s.rpc.list_workspaces()
        raise AssertionError("workspaces on a locked vault must refuse")
    except BridgeError as e:
        assert "lock" in str(e).lower(), f"unexpected error: {e}"

    # signup: mnemonic once, vault left unlocked (commit sets it active)
    mnemonic = s.rpc.signup("abe", "correct horse")["mnemonic"]
    assert mnemonic and " " in mnemonic, "expected a multi-word recovery mnemonic"
    did = s.rpc.list_accounts()[0]["id"]

    # unlock: wrong passphrase refused, right one accepted — by label and by did
    s.rpc.lock()
    try:
        s.rpc.unlock("abe", "wrong horse")
        raise AssertionError("a wrong passphrase must be refused")
    except BridgeError:
        pass
    assert s.rpc.unlock("abe", "correct horse") == "unlocked"
    assert isinstance(s.rpc.list_workspaces(), list)

    s.rpc.lock()
    assert s.rpc.unlock(did, "correct horse") == "unlocked"

    # workspaces/items: create, list, and the honest no-source refusal
    made = s.rpc.create_workspace("test")
    assert made["name"] == "test" and made["id"]
    assert any(w["id"] == made["id"] for w in s.rpc.list_workspaces())

    item = s.rpc.create_item(made["id"], "tally", "app")
    assert item["kind"] == "app" and item["ws_id"] == made["id"]
    assert [i["id"] for i in s.rpc.list_items(made["id"])] == [item["id"]]

    try:
        s.rpc.open_item(item["id"])
        raise AssertionError("an item with no source must refuse to open")
    except BridgeError as e:
        assert "source" in str(e).lower(), f"unexpected error: {e}"

    app = """
local s = doc:open("state")
if not s.map then s:set({ "map" }, doc.map({ count = 0 })) end
return function()
	local m = s.map
	return ui.col({ id = "root", grow = true, center = true, gap = 12,
		ui.text({ "count: " .. tostring(m.count), id = "count" }),
		ui.button({ id = "inc", h = 32, px = 14, center = true,
			ui.text({ "inc" }),
			on_click = function() s:set({ "map", "count" }, (m.count or 0) + 1) end }),
		ui.input({ id = "note", value = m.note or "", w = 220, h = 30,
			on_input = function(v) s:set({ "map", "note" }, v) end,
			on_enter = function() s:set({ "map", "count" }, (m.count or 0) + 10) end }),
		ui.button({ id = "boom", h = 32, px = 14, center = true,
			ui.text({ "boom" }),
			on_click = function() error("kaboom") end }),
	})
end
"""
    assert s.rpc.list_files(item["id"]) == []
    assert s.rpc.write_file(item["id"], "main.lua", app) == "written"
    assert s.rpc.list_files(item["id"]) == ["main.lua"]
    assert s.rpc.read_file(item["id"], "main.lua") == app
    assert s.rpc.open_item(item["id"]) == "open"

    def find(node, el_id):
        if node.get("id") == el_id:
            return node
        for c in node.get("children", []):
            if hit := find(c, el_id):
                return hit

    def text_of(tree, el_id):
        return find(tree, el_id)["text"]

    tree = s.rpc.dump_tree(item["id"])
    assert find(tree, "root") and text_of(tree, "count") == "count: 0"
    assert "on_click" in find(tree, "inc")["handlers"]
    assert "on_input" in find(tree, "note")["handlers"]
    assert s.rpc.click(item["id"], "inc") == "fired"
    assert s.rpc.click(item["id"], "inc") == "fired"
    assert text_of(s.rpc.dump_tree(item["id"]), "count") == "count: 2"
    assert s.rpc.type_text(item["id"], "note", "hello bridge") == "fired"
    tree = s.rpc.dump_tree(item["id"])
    assert find(tree, "note")["text"] == "hello bridge"
    assert s.rpc.key(item["id"], "note", "enter") == "fired"
    assert text_of(s.rpc.dump_tree(item["id"]), "count") == "count: 12"

    data = s.rpc.read_data(item["id"])
    assert data["state"]["map"]["count"] == 12
    assert data["state"]["map"]["note"] == "hello bridge"
    assert s.rpc.read_console(item["id"], 10) == []
    assert s.rpc.click(item["id"], "boom") == "fired"
    console = s.rpc.read_console(item["id"], 5)
    assert console and "handler error" in console[-1] and "kaboom" in console[-1], console
    # the app survives its handler's error
    assert text_of(s.rpc.dump_tree(item["id"]), "count") == "count: 12"

    shot = Path(s.tmp) / "screen.png"
    dimensions = s.rpc.save_screenshot(item["id"], shot)
    assert shot.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
    assert dimensions["width_px"] > 0 and dimensions["height_px"] > 0
    custom = Path(s.tmp) / "custom.png"
    assert s.rpc.save_screenshot(
        item["id"], custom, width=320, height=240, scale=1.0
    ) == {"width_px": 320, "height_px": 240}
    assert custom.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")

    try:
        s.rpc.click(item["id"], "nope")
        raise AssertionError("clicking an unknown id must fail")
    except BridgeError as e:
        assert "no element" in str(e), f"unexpected error: {e}"

    print("smoke ok: transport, auth, files, app senses/actions, and screenshot")
