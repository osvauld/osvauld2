"""Launcher and RPC reopening preserve one tab and the running app's Lua locals."""

import json

from osvauld.client import BridgeError
from osvauld.session import Session


with Session() as s:
    rpc = s.rpc
    rpc.signup("tabs", "passphrase")
    ws = rpc.create_workspace("workspace")
    item = rpc.create_item(ws["id"], "local counter", "app")
    missing = rpc.create_item(ws["id"], "no source", "app")
    rpc.write_file(item["id"], "main.lua", """
local count = 0
return function()
	return ui.col({ full = true, center = true,
		ui.text({ "local count: " .. count }),
		ui.button({ id = "inc", w = 100, h = 40, center = true,
			"increment", on_click = function() count += 1 end }),
	})
end
""")

    def click(el_id):
        rpc.click_at(*rpc.centre_of(el_id))

    def home():
        click("tab:home")

    def assert_one_tab():
        assert sum(r["id"] == "tab:" + item["id"] for r in rpc.rects()) == 1

    click(ws["id"])
    click("launcher:item:" + item["id"])
    click("inc")
    assert "local count: 1" in json.dumps(rpc.dump_tree(item["id"]))

    for _ in range(2):
        home()
        click("launcher:item:" + item["id"])
        assert_one_tab()
        assert "local count: 1" in json.dumps(rpc.dump_tree(item["id"]))

    home()
    assert rpc.open_item(item["id"]) == "open"
    assert_one_tab()
    assert "local count: 1" in json.dumps(rpc.dump_tree(item["id"]))

    home()
    click("launcher:item:" + missing["id"])
    assert rpc.centre_of("launcher:item:" + item["id"]), "failed opening should leave the launcher focused"
    assert all(r["id"] != "tab:" + missing["id"] for r in rpc.rects())
    try:
        rpc.open_item(missing["id"])
        raise AssertionError("source-less item opened")
    except BridgeError as error:
        assert "source" in str(error)
    assert all(r["id"] != "tab:" + missing["id"] for r in rpc.rects())

    rpc.open_item(item["id"])
    assert_one_tab()
    assert "local count: 1" in json.dumps(rpc.dump_tree(item["id"]))
    assert rpc.read_console(item["id"]) == []

print("open-or-focus: launcher/RPC preserve the VM and refuse missing source")
