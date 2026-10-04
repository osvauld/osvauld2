"""Combined thread/chrome gate: retained app instances, split routing and budget fallback."""
import json

from osvauld.client import BridgeError
from osvauld.session import Session, shell_binary

with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    rpc = s.rpc
    rpc.signup("threaded chrome", "passphrase")
    ws = rpc.create_workspace("Workspace")
    items = [rpc.create_item(ws["id"], name, "app")["id"] for name in ("A", "B")]
    for item in items:
        rpc.write_file(item, "main.lua", """
local n = 0
return function()
    return ui.button({id='counter', 'Count: '..n, on_click=function() n+=1 end})
end
""")
        rpc.open_item(item)
        rpc.click(item, "counter")
    rpc.request("SplitWith", item_id=items[0])
    rpc.frame(2)
    assert sum(r["id"] == "counter" for r in rpc.rects()) == 2
    for item in items:
        assert "Count: 1" in json.dumps(rpc.dump_tree(item))
    try:
        rpc.dump_launcher_tree()
        raise AssertionError("launcher senses should require Home")
    except BridgeError as error:
        assert "focus Home" in str(error)

    rpc.select_workspace(ws["id"])
    assert all(r["id"] != "counter" for r in rpc.rects())
    source = rpc.read_launcher_file_versioned("main.lua")
    rpc.edit_launcher_file("main.lua", source["revision"], [
        {"old_text": "Your apps", "new_text": "Threaded apps"},
    ])
    for item in items:
        rpc.click_at(*rpc.centre_of("launcher:item:" + item))
        assert "Count: 1" in json.dumps(rpc.dump_tree(item))
        rpc.select_workspace(ws["id"])
    assert all(r["id"] != "counter" for r in rpc.rects())

    # Trial view passes once; the activated view loops. Rust controls must remain reachable.
    source = rpc.read_launcher_file_versioned("main.lua")
    rpc.edit_launcher_file("main.lua", source["revision"], [{
        "old_text": source["content"],
        "new_text": """
local views = 0
return function()
    views += 1
    if views > 1 then while true do end end
    return ui.col({id='launcher:workspace-header', ui.text({'trial'})})
end
""",
    }])
    rpc.frame(2)
    assert any("budget exceeded" in line for line in rpc.read_launcher_console())
    assert rpc.centre_of("back")
    assert rpc.centre_of("add_item")
    rpc.save_shell_screenshot("/tmp/lua-launcher-thread-fallback.png")
    rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
    rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
    assert "Your apps" in json.dumps(rpc.dump_launcher_tree())
    for item in items:
        rpc.open_item(item)
        assert "Count: 1" in json.dumps(rpc.dump_tree(item))
        assert rpc.read_console(item) == []

print("threaded launcher: split, Home senses, edits, retained instances and hot-loop Rust fallback/reset passed")
