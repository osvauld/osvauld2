"""Focused launcher acceptance loop: durable edits, rejected edits, pointer opening and reset.

Uses a throwaway encrypted store; saves the actual shell preview under /tmp.
"""
import json
import tempfile
from pathlib import Path

from osvauld.client import BridgeError
from osvauld.session import Session, shell_binary


def rejects(action, contains):
    try:
        action()
    except BridgeError as error:
        assert contains in str(error), str(error)
    else:
        raise AssertionError("operation should have been rejected")


with tempfile.TemporaryDirectory(prefix="launcher-acceptance-") as tmp:
    data = str(Path(tmp) / "data")
    args = dict(shell_binary=shell_binary(), data_dir=data, offscreen=(900, 700))
    with Session(**args) as s:
        rpc = s.rpc
        rpc.signup("launcher owner", "passphrase")
        ws = rpc.create_workspace("Personal workspace")
        item = rpc.create_item(ws["id"], "Counter", "app")
        rpc.write_file(item["id"], "main.lua", "local n=0; return function() return ui.button({'Count: '..n, id='counter', on_click=function() n+=1 end}) end")
        rpc.select_workspace(ws["id"])
        before = rpc.read_launcher_file_versioned("main.lua")
        theme = rpc.read_launcher_file_versioned("theme.lua")
        edited = rpc.edit_launcher_file("main.lua", before["revision"], [
            {"old_text": "Your apps", "new_text": "My encrypted workspace"},
        ])
        assert edited["persisted"] and edited["activation"] == "activated"
        rpc.edit_launcher_file("theme.lua", theme["revision"], [
            {"old_text": 'app = "#83adff"', "new_text": 'app = "#c7a0ff"'},
        ])
        rejects(lambda: rpc.edit_launcher_file("main.lua", before["revision"], [
            {"old_text": "Your apps", "new_text": "stale edit"},
        ]), "stale source revision")
        rejects(lambda: rpc.edit_launcher_file("main.lua", edited["revision"], [
            {"old_text": "local snapshot = shell.snapshot()", "new_text": "error('broken launcher')"},
        ]), "broken launcher")
        assert rpc.read_launcher_file_versioned("main.lua")["revision"] == edited["revision"]
        assert "My encrypted workspace" in json.dumps(rpc.dump_launcher_tree())
        rpc.move_to(*rpc.centre_of("launcher:item:" + item["id"]))
        rpc.frame(12)  # Let the declarative cursor fade complete on the virtual clock.
        rpc.save_shell_screenshot("/tmp/lua-launcher-customized.png")
        rpc.click_at(*rpc.centre_of("launcher:item:" + item["id"]))
        rpc.click(item["id"], "counter")
        rejects(lambda: rpc.read_launcher_file_versioned("main.lua"), "focus Home")
        rpc.select_workspace(ws["id"])
        rpc.click_at(*rpc.centre_of("launcher:item:" + item["id"]))
        assert "Count: 1" in json.dumps(rpc.dump_tree(item["id"]))

    with Session(**args) as s:
        rpc = s.rpc
        rpc.unlock("launcher owner", "passphrase")
        rpc.select_workspace(ws["id"])
        assert "My encrypted workspace" in json.dumps(rpc.dump_launcher_tree())
        assert '#c7a0ff' in rpc.read_launcher_file_versioned("theme.lua")["content"]
        rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
        assert "My encrypted workspace" in json.dumps(rpc.dump_launcher_tree())
        assert rpc.centre_of("shell:cancel-launcher-reset")
        rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
        assert rpc.read_launcher_file_versioned("main.lua") == before
        assert rpc.read_launcher_file_versioned("theme.lua") == theme
        rpc.save_shell_screenshot("/tmp/lua-launcher-reset.png")
        rpc.lock()
        rejects(lambda: rpc.read_launcher_file_versioned("main.lua"), "vault is locked")
        rejects(rpc.reset_launcher, "vault is locked")

    with Session(**args) as s:
        rpc = s.rpc
        rpc.unlock("launcher owner", "passphrase")
        rpc.select_workspace(ws["id"])
        assert rpc.read_launcher_file_versioned("main.lua") == before
        assert rpc.read_launcher_console() == []

print("launcher acceptance: edits, persistence/restart, bad-edit rollback, pointer clicks, retained app VM and durable reset passed")
print("previews: /tmp/lua-launcher-customized.png, /tmp/lua-launcher-reset.png")
