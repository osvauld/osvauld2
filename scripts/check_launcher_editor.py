"""Focused real-pixel Lua customizer check: input actions, pointer controls and failed drafts."""
from osvauld.session import Session, shell_binary


def text_of(tree, el_id):
    if tree.get("id") == el_id:
        return tree.get("text")
    for child in tree.get("children", []):
        result = text_of(child, el_id)
        if result is not None:
            return result
    return None


for width in (320, 900):
    with Session(shell_binary=shell_binary(), offscreen=(width, 700)) as s:
        rpc = s.rpc
        rpc.signup("editor owner", "passphrase")
        ws = rpc.create_workspace("Home")
        rpc.select_workspace(ws["id"])
        theme = rpc.read_launcher_file_versioned("theme.lua")
        rpc.click_at(*rpc.centre_of("launcher:customize"))
        for el_id in ("launcher:editor:source", "launcher:editor:save", "launcher:editor:cancel"):
            rect = next(r for r in rpc.rects() if r["id"] == el_id)
            assert rect["h"] >= (70 if el_id.endswith("source") else 31), f"clipped at {width}: {rect}"
        rpc.click_at(*rpc.centre_of("launcher:editor:source"))
        good = theme["content"] + "\n-- edited in the Lua customizer\n"
        # Keyboard drives game keys, not native text editing. TypeLauncher calls on_input.
        rpc.type_launcher("launcher:editor:source", good)
        assert text_of(rpc.dump_launcher_tree(), "launcher:editor:source") == good.replace("\t", "    ")
        rpc.save_shell_screenshot(f"/tmp/lua-launcher-editor-{width}.png")
        rpc.type_launcher("launcher:editor:source", "not valid Luau !!!")
        rpc.click_at(*rpc.centre_of("launcher:editor:save"))
        assert text_of(rpc.dump_launcher_tree(), "launcher:editor:source") == "not valid Luau !!!"
        assert rpc.read_launcher_file_versioned("theme.lua") == theme
        assert any("launcher edit rejected" in line for line in rpc.read_launcher_console())
        save = next(r for r in rpc.rects() if r["id"] == "launcher:editor:save")
        assert save["h"] >= 31, f"failed-save controls clipped at {width}: {save}"
        rpc.type_launcher("launcher:editor:source", good)
        rpc.click_at(*rpc.centre_of("launcher:editor:save"))
        assert text_of(rpc.dump_launcher_tree(), "launcher:editor:source") is None
        assert rpc.read_launcher_file_versioned("theme.lua")["content"] == good
        rpc.select_workspace(ws["id"])
        assert rpc.read_launcher_file_versioned("theme.lua")["content"] == good
        rpc.click_launcher("launcher:customize")
        rpc.type_launcher("launcher:editor:source", "discard this draft")
        rpc.click_at(*rpc.centre_of("launcher:editor:cancel"))
        assert rpc.read_launcher_file_versioned("theme.lua")["content"] == good
        rpc.reset_launcher()
        assert rpc.read_launcher_file_versioned("theme.lua") == theme

print("Lua customizer: input actions, draft retention, pointer save/cancel, persistence and reset passed at 320/900pt")
print("previews: /tmp/lua-launcher-editor-320.png, /tmp/lua-launcher-editor-900.png")
