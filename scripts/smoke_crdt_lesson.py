"""Interactive counter mathematics and a subject-neutral presenter through the real shell."""

import argparse
import sys
from pathlib import Path

from osvauld.session import Session, offscreen_default, shell_binary

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "demo_apps" / "crdt_lesson"


def nodes(node):
    yield node
    for child in node.get("children", []):
        yield from nodes(child)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shots", type=Path)
    args = parser.parse_args()
    assert (APP / "main.lua").is_file(), "CRDT lesson is not implemented"
    with Session(shell_binary=shell_binary(), offscreen=offscreen_default() or (1280, 900)) as session:
        rpc = session.rpc
        rpc.signup("lesson", "lesson test")
        ws = rpc.create_workspace("Interactive lessons")
        item = rpc.create_item(ws["id"], "Why CRDTs converge", "app")["id"]
        rpc.upload_folder(item, APP)
        rpc.open_item(item)

        def tree():
            return rpc.dump_tree(item)

        def text(eid):
            return next(n.get("text", "") for n in nodes(tree()) if n.get("id") == eid)

        def click(eid):
            rpc.click_at(*rpc.centre_of(eid))

        def totals(a, b):
            assert text("counter-total:alice") == str(a), text("counter-total:alice")
            assert text("counter-total:bob") == str(b), text("counter-total:bob")

        def capture(name):
            if args.shots:
                args.shots.mkdir(parents=True, exist_ok=True)
                rpc.save_screenshot(item, args.shots / f"{name}.png")

        totals(0, 0)
        click("action:inc-alice")
        click("action:inc-bob")
        totals(1, 1)
        click("action:send-alice")
        click("action:deliver-first")
        totals(1, 1)
        assert text("counter-queue") == "1 snapshot waiting"
        click("action:inc-alice")
        click("action:network")
        click("action:deliver-first")
        totals(2, 2)  # Bob receives the earlier Alice=1 snapshot and keeps Bob=1.

        # A stale snapshot cannot lower an already received component.
        click("action:send-alice")
        click("action:send-bob")
        click("action:deliver-last")
        click("action:deliver-first")
        totals(3, 3)
        click("action:redeliver")
        totals(3, 3)
        assert text("counter-slot:alice:alice") == "2"
        assert text("counter-slot:alice:bob") == "1"
        capture("explore")

        click("scene:naive")
        click("action:copy-total")
        totals(1, 1)
        assert text("counter-expected") == "2 clicks happened"
        click("scene:merge")
        click("action:send-alice")
        click("action:send-bob")
        click("action:deliver-first")
        click("action:deliver-first")
        totals(2, 2)

        for delivery in ("deliver-first", "deliver-last"):
            click("scene:order")
            for _ in range(3):
                click("action:" + delivery)
            totals(3, 3)
            click("action:redeliver")
            totals(3, 3)
        capture("converged")

        click("mode:presenter")
        assert "commutative" in text("presenter-narration").lower()
        assert text("presenter-cue")
        capture("presenter")
        click("mode:record")
        ids = {n.get("id") for n in nodes(tree())}
        assert "presenter-narration" not in ids and "mode:explore" not in ids
        assert "action:inc-alice" not in ids
        capture("record")
        rpc.keyboard("Escape", "Escape", True)
        rpc.keyboard("Escape", "Escape", False)
        assert text("presenter-narration")
        click("scene:hook")
        click("lesson-play")
        rpc.frame(150)
        totals(1, 1)
        before = text("lesson-time")
        for _ in range(3):
            rpc.screenshot(item)
            rpc.dump_tree(item)
        assert text("lesson-time") == before
        click("lesson-play")
        before = text("lesson-time")
        rpc.frame(120)
        assert text("lesson-time") == before
        assert rpc.read_data(item) == {}, "experiments wrote shared document state"
        assert rpc.read_console(item) == [], rpc.read_console(item)

        # Same library, unrelated subject and a completely different visual.
        other = rpc.create_item(ws["id"], "Color mixing", "app")["id"]
        for path in ("lib/presenter.lua", "lib/theme.lua"):
            rpc.write_file(other, path, (APP / path).read_text())
        rpc.write_file(other, "main.lua", '''local presenter = require("lib/presenter")
return presenter.app({
 id = "colors", title = "Color mixing",
 scenes = {{ id = "mix", title = "Mix colors", summary = "Two colors combine", narration = "Mix blue and yellow.", cue = "Press mix.", seconds = 3, cues = {{at = 1, action = "mix"}} }},
 prepare = function() return {mixed = false} end,
 actions = {mix = function(m) m.mixed = true end},
 view = function(m, ctx) return ui.button({id = "mix-colors", on_click = function() ctx.act("mix") end, ui.text({m.mixed and "Green" or "Blue + yellow", id = "color-result"})}) end,
})''')
        rpc.open_item(other)
        rpc.click_at(*rpc.centre_of("mix-colors"))
        assert any(n.get("text") == "Green" for n in nodes(rpc.dump_tree(other)))
        source = rpc.read_file_versioned(other, "main.lua")
        result = rpc.edit_file(other, "main.lua", source["revision"], [{"old_text": 'action = "mix"', "new_text": 'action = "unknown"'}])
        assert result["activation"] != "activated", result
        assert any("unknown" in entry.lower() for entry in rpc.read_console(other))
        assert any(n.get("text") == "Green" for n in nodes(rpc.dump_tree(other)))
        source = rpc.read_file_versioned(other, "main.lua")
        result = rpc.edit_file(other, "main.lua", source["revision"], [
            {"old_text": 'action = "unknown"', "new_text": 'action = "mix"'},
            {"old_text": 'scenes = {{', "new_text": 'scenes = {{id = "mix", title = "first", summary = "one", narration = "one", cue = "one", seconds = 1}, {'},
        ])
        assert result["activation"] != "activated", result
        assert any("duplicate scene id mix" in entry for entry in rpc.read_console(other))

        rpc.open_item(item)
        from present_lesson import collect, write_script
        import tempfile
        with tempfile.TemporaryDirectory(prefix="lesson-script-") as folder:
            script = Path(folder) / "script.md"
            scenes = collect(rpc, item)
            write_script(scenes, script)
            exported = script.read_text()
            assert len(scenes) == 6
            assert scenes[0]["narration"] in exported
            assert scenes[3]["cue"] in exported
            assert scenes[3]["actions"][0] in exported
        assert rpc.read_console(item) == [], rpc.read_console(item)
    print("CRDT lesson: computed merges, snapshots, duplicates/order, modes, cues and reusable presenter")


if __name__ == "__main__":
    main()
