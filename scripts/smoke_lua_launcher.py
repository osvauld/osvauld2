"""Real bundled launcher: empty state, inventory refresh, workspace scope and pointer opening."""

import json

from osvauld.session import Session, shell_binary

for width in (320, 900):
    with Session(shell_binary=shell_binary(), offscreen=(width, 700)) as s:
        rpc = s.rpc
        rpc.signup("launcher", "passphrase")
        ws = rpc.create_workspace("Home")
        rpc.click_at(*rpc.centre_of(ws["id"]))
        assert rpc.centre_of("launcher:add-item")
        assert not any(r["id"].startswith("launcher:item:") for r in rpc.rects())

        item = rpc.create_item(ws["id"], "Counter", "app")
        rpc.write_file(item["id"], "main.lua", "return function() return ui.text({'launched from Lua', id = 'app-root'}) end")
        card_id = "launcher:item:" + item["id"]
        card = next(r for r in rpc.rects() if r["id"] == card_id)
        assert card["w"] >= 199 and card["h"] >= 143, f"clipped at {width}: {card}"
        assert card["x"] >= 0 and card["x"] + card["w"] <= width

        other_ws = rpc.create_workspace("Other")
        other_item = rpc.create_item(other_ws["id"], "Private to other workspace", "app")
        assert not any(r["id"] == "launcher:item:" + other_item["id"] for r in rpc.rects())
        rpc.click_at(*rpc.centre_of(card_id))
        assert rpc.centre_of("tab:" + item["id"])
        assert "launched from Lua" in json.dumps(rpc.dump_tree(item["id"]))
        assert rpc.read_console(item["id"]) == []
        rpc.click_at(*rpc.centre_of("tab:home"))
        assert rpc.centre_of(card_id)

print("Lua launcher: mounted, responsive, refreshes inventory, scoped and opens real apps")
