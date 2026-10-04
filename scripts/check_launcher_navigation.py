"""Lua workspace header and Rust compatibility/recovery chrome, using real pointer targets."""
from osvauld.client import BridgeError
from osvauld.session import Session, shell_binary

with Session(shell_binary=shell_binary(), offscreen=(320, 700)) as s:
    rpc = s.rpc
    rpc.signup("navigation", "passphrase")
    ws = rpc.create_workspace("A workspace with a long title")
    rpc.select_workspace(ws["id"])
    original = rpc.read_launcher_file_versioned("main.lua")
    ids = {rect["id"] for rect in rpc.rects()}
    assert {"launcher:back", "launcher:add-item"} <= ids
    assert "back" not in ids and "add_item" not in ids, "duplicate Rust header"
    rpc.save_shell_screenshot("/tmp/lua-workspace-header-320.png")
    rpc.click_at(*rpc.centre_of("launcher:back"))
    try:
        rpc.dump_launcher_tree()
    except BridgeError:
        pass  # Spaces has no workspace launcher.
    else:
        raise AssertionError("Back did not leave the workspace")
    rpc.select_workspace(ws["id"])

    # Older/custom sources without the reserved marker retain the Rust controls.
    rpc.edit_launcher_file("main.lua", original["revision"], [{
        "old_text": original["content"],
        "new_text": "return function() return ui.text({'Legacy launcher'}) end",
    }])
    ids = {rect["id"] for rect in rpc.rects()}
    assert {"back", "add_item"} <= ids
    assert "launcher:back" not in ids
    rpc.click_at(*rpc.centre_of("back"))
    rpc.select_workspace(ws["id"])
    rpc.reset_launcher()

    # A view can fail only after activation. Rust controls must return in that frame.
    current = rpc.read_launcher_file_versioned("main.lua")
    rpc.edit_launcher_file("main.lua", current["revision"], [{
        "old_text": current["content"],
        "new_text": "local broken=false; return function() if broken then error('broken header') end; return ui.col({id='launcher:workspace-header', ui.button({id='break', 'Break', on_click=function() broken=true end})}) end",
    }])
    rpc.click_launcher("break")
    ids = {rect["id"] for rect in rpc.rects()}
    assert {"back", "add_item"} <= ids
    rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
    rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
    assert rpc.read_launcher_file_versioned("main.lua") == original
    assert rpc.read_launcher_console() == []

print("Lua workspace header: narrow layout, pointer Back, legacy fallback and runtime-error recovery passed")
print("preview: /tmp/lua-workspace-header-320.png; import command is unit-tested, native picker remains manual")
