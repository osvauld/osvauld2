"""Real Lua workspace list, pointer selection and compatibility/recovery gates."""
import json
from osvauld.session import Session, shell_binary

for width in (320, 900):
    with Session(shell_binary=shell_binary(), offscreen=(width, 700)) as session:
        rpc = session.rpc
        rpc.signup("spaces", "passphrase")
        assert "no workspaces yet" in json.dumps(rpc.dump_launcher_tree())
        first = rpc.create_workspace("First workspace")
        second = rpc.create_workspace("Second workspace with a longer title")
        for workspace in (first, second):
            assert rpc.centre_of("launcher:space:" + workspace["id"])
        rpc.save_shell_screenshot(f"/tmp/lua-workspaces-{width}.png")
        rpc.click_at(*rpc.centre_of("launcher:space:" + second["id"]))
        assert second["name"] in json.dumps(rpc.dump_launcher_tree())
        original = rpc.read_launcher_file_versioned("main.lua")
        rpc.click_at(*rpc.centre_of("launcher:back"))
        assert "launcher:workspace-list" in json.dumps(rpc.dump_launcher_tree())
        rpc.click_at(*rpc.centre_of("launcher:space:" + first["id"]))
        assert first["name"] in json.dumps(rpc.dump_launcher_tree())

        # A legacy workspace view stays intact; it is not evaluated as the account-list entry.
        rpc.edit_launcher_file("main.lua", original["revision"], [{
            "old_text": original["content"],
            "new_text": "return function() return ui.text({'Legacy workspace marker'}) end",
        }])
        rpc.click_at(*rpc.centre_of("back"))
        assert "launcher:workspace-list" in json.dumps(rpc.dump_launcher_tree())
        rpc.click_at(*rpc.centre_of("launcher:space:" + first["id"]))
        assert "Legacy workspace marker" in json.dumps(rpc.dump_launcher_tree())

        # An invalid list override falls back without losing the workspace override.
        entry = rpc.read_launcher_file_versioned("spaces_view.lua")
        rpc.edit_launcher_file("spaces_view.lua", entry["revision"], [{
            "old_text": entry["content"], "new_text": "not valid Luau !!!",
        }])
        rpc.click_at(*rpc.centre_of("back"))
        assert "launcher:workspace-list" in json.dumps(rpc.dump_launcher_tree())
        rpc.click_at(*rpc.centre_of("launcher:space:" + first["id"]))
        assert "Legacy workspace marker" in json.dumps(rpc.dump_launcher_tree())
        rpc.reset_launcher()

        # Fail only after a successful trial; native workspace cards and reset must remain.
        entry = rpc.read_launcher_file_versioned("spaces_view.lua")
        rpc.edit_launcher_file("spaces_view.lua", entry["revision"], [{
            "old_text": entry["content"],
            "new_text": "local calls=0; return function() calls+=1; if calls>1 then error('broken list') end; return ui.col({id='launcher:workspace-list'}) end",
        }])
        rpc.click_at(*rpc.centre_of("launcher:back"))
        assert rpc.centre_of(first["id"]), "Rust card fallback is not reachable"
        rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
        rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
        assert "launcher:workspace-list" in json.dumps(rpc.dump_launcher_tree())
        assert rpc.centre_of("launcher:space:" + first["id"])

print("Lua workspace list: empty/narrow, pointer selection, legacy overrides and Rust fallback/reset passed")
print("previews: /tmp/lua-workspaces-320.png, /tmp/lua-workspaces-900.png")
