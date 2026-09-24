"""Keyboard driver through a real shell, Lua VM and virtual-clock frames."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.client import BridgeError
from osvauld.session import Session, shell_binary


def texts(node):
    if isinstance(node, dict):
        if isinstance(node.get("text"), str):
            yield node["text"]
        for value in node.values():
            yield from texts(value)
    elif isinstance(node, list):
        for value in node:
            yield from texts(value)


app = '''
local held, steps, last = false, 0, "none"
return function()
    return ui.col({ id = "game", full = true,
        on_key = function(e)
            if e.cancelled then held = false; last = "cancel"
            elseif e.code == "KeyW" and not e.repeated then
                held = e.down; last = e.down and "down" or "up"
            end
        end,
        on_frame = function(e) if held then steps = steps + 1 end end,
        ui.text({ "steps: " .. steps }),
        ui.text({ "key: " .. last }),
    })
end
'''

with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    s.rpc.signup("tank", "tank passphrase")
    ws = s.rpc.create_workspace("tank smoke")
    item = s.rpc.create_item(ws["id"], "keys", "app")["id"]
    s.rpc.write_file(item, "main.lua", app)
    s.rpc.open_item(item)
    tree = s.rpc.dump_tree(item)
    assert "on_key" in str(tree), (tree, s.rpc.read_console(item))
    assert s.rpc.keyboard("KeyW", "z", True)["frames"] == 1
    s.rpc.frame(3)
    moving = list(texts(s.rpc.dump_tree(item)))
    assert "key: down" in moving and any(t.startswith("steps: ") and int(t[7:]) > 0 for t in moving)
    s.rpc.keyboard("KeyW", "z", False)
    s.rpc.frame(2)
    stopped = list(texts(s.rpc.dump_tree(item)))
    assert "key: up" in stopped, (stopped, s.rpc.read_console(item))
    s.rpc.frame(2)
    assert list(texts(s.rpc.dump_tree(item))) == stopped, "released key still moved the tank"
    try:
        s.rpc.keyboard("F12", "F12", True)
        raise AssertionError("driver bypassed the reserved shell shortcut")
    except BridgeError as err:
        assert "reserved" in str(err)
    assert s.rpc.read_console(item) == []

print("tank keyboard smoke ok")
