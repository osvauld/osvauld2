"""Lua creation form through the real shell VM, socket and pointer path."""
import json
import time
from osvauld.session import Session, shell_binary

for width in (320, 900):
    with Session(shell_binary=shell_binary(), offscreen=(width, 700)) as session:
        rpc = session.rpc
        rpc.signup("creation", "passphrase")
        ids = {rect["id"] for rect in rpc.rects()}
        assert "launcher:new-workspace" in ids and "add_space" not in ids
        rpc.click_at(*rpc.centre_of("launcher:new-workspace"))
        rpc.click_at(*rpc.centre_of("launcher:create:submit"))
        assert "enter a workspace name" in json.dumps(rpc.dump_launcher_tree())
        assert rpc.list_workspaces() == []
        rpc.type_launcher("launcher:create:name", "cancel this draft")
        rpc.click_at(*rpc.centre_of("launcher:create:cancel"))
        rpc.click_at(*rpc.centre_of("launcher:new-workspace"))
        assert "cancel this draft" not in json.dumps(rpc.dump_launcher_tree())
        rpc.type_launcher("launcher:create:name", "x" * 257)
        rpc.click_at(*rpc.centre_of("launcher:create:submit"))
        assert "at most 256 bytes" in json.dumps(rpc.dump_launcher_tree())
        assert rpc.list_workspaces() == []
        rpc.type_launcher("launcher:create:name", "  My Lua workspace ✨  ")
        rpc.save_shell_screenshot(f"/tmp/lua-create-{width}.png")
        rpc.click_at(*rpc.centre_of("launcher:create:submit"))
        deadline = time.monotonic() + 10
        while "launcher:create:name" in json.dumps(rpc.dump_launcher_tree()):
            assert time.monotonic() < deadline, "creation did not wake/complete"
            time.sleep(0.01)
        workspaces = rpc.list_workspaces()
        assert len(workspaces) == 1 and workspaces[0]["name"] == "My Lua workspace ✨"
        rpc.click_at(*rpc.centre_of("launcher:space:" + workspaces[0]["id"]))
        assert "My Lua workspace ✨" in json.dumps(rpc.dump_launcher_tree(), ensure_ascii=False)

        # A custom list without the creation marker keeps the independent native control.
        entry = rpc.read_launcher_file_versioned("spaces_view.lua")
        rpc.edit_launcher_file("spaces_view.lua", entry["revision"], [{
            "old_text": entry["content"],
            "new_text": "local view=require('spaces'); return function() return view(shell.snapshot().workspaces, shell.select_workspace) end",
        }])
        rpc.click_at(*rpc.centre_of("launcher:back"))
        assert rpc.centre_of("add_space")
        rpc.reset_launcher()
        assert rpc.centre_of("launcher:new-workspace")
        rpc.lock()
        rpc.unlock("creation", "passphrase")
        assert rpc.list_workspaces() == workspaces
        assert rpc.centre_of("launcher:space:" + workspaces[0]["id"])
        assert rpc.read_launcher_console() == []

print("Lua creation: pointer form, validation/cancel, Unicode name, worker wake, durable record and legacy control passed at 320/900pt")
print("previews: /tmp/lua-create-320.png, /tmp/lua-create-900.png")
