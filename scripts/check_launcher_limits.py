"""Focused resource-limit and Rust recovery checks in a disposable encrypted store."""
import json
from osvauld.client import BridgeError
from osvauld.session import Session, shell_binary

with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    rpc = s.rpc
    rpc.signup("launcher limits", "passphrase")
    ws = rpc.create_workspace("Limits")
    rpc.select_workspace(ws["id"])
    original = rpc.read_launcher_file_versioned("main.lua")
    rpc.edit_launcher_file("main.lua", original["revision"], [
        {"old_text": "Your apps", "new_text": "Working launcher"},
    ])
    working = rpc.read_launcher_file_versioned("main.lua")
    for bad in [
        "while true do end",
        "return function() while true do end end",
        "local huge=string.rep('x', 64*1024*1024); return function() return ui.text({tostring(#huge)}) end",
    ]:
        try:
            rpc.edit_launcher_file("main.lua", working["revision"], [
                {"old_text": working["content"], "new_text": bad},
            ])
        except BridgeError as error:
            assert any(word in str(error) for word in ("budget", "deadline", "memory")), str(error)
        else:
            raise AssertionError("resource-exhausting candidate activated")
        assert rpc.read_launcher_file_versioned("main.lua") == working
        assert "Working launcher" in json.dumps(rpc.dump_launcher_tree())
        assert rpc.ping() == "pong"

    # A runtime handler can poison scratch export and then exhaust its execution budget.
    # Reset must rebuild without entering that VM, rather than relying on state carry.
    rpc.edit_launcher_file("main.lua", working["revision"], [{
        "old_text": working["content"],
        "new_text": "return function() return ui.button({id='break', 'Break', on_click=function() _dump_state=function() error('poisoned state export') end; while true do end end}) end",
    }])
    rpc.click_launcher("break")
    assert any(any(word in line for word in ("budget", "deadline")) for line in rpc.read_launcher_console())
    rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
    rpc.click_at(*rpc.centre_of("shell:reset-launcher"))
    assert rpc.read_launcher_file_versioned("main.lua") == original
    assert rpc.read_launcher_console() == []
    assert rpc.ping() == "pong"

print("launcher limits: runaway load/view/handler, heap rejection, source rollback and Rust pointer reset passed")
