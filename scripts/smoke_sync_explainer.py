"""An interactive explanation through real shell pixels and pointer hits, not a sync test."""

import argparse
import base64
from pathlib import Path

from osvauld.png import Image, near
from osvauld.session import Session, offscreen_default, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def nodes(node):
    yield node
    for child in node.get("children", []):
        yield from nodes(child)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shots", type=Path)
    args = parser.parse_args()
    viewport = offscreen_default() or (1280, 900)
    app = ROOT / "demo_apps" / "sync_explainer"
    assert (app / "main.lua").is_file(), "sync explainer app has not been implemented"
    with Session(shell_binary=shell_binary(), offscreen=viewport) as s:
        rpc = s.rpc
        rpc.signup("explorer", "explainer test")
        ws = rpc.create_workspace("Interactive explanations")
        item = rpc.create_item(ws["id"], "How sync works", "app")["id"]
        rpc.upload_folder(item, app)
        rpc.open_item(item)

        def text(eid):
            return next(n.get("text", "") for n in nodes(rpc.dump_tree(item)) if n.get("id") == eid)

        def click(eid):
            rpc.click_at(*rpc.centre_of(eid))

        def stage(expected):
            assert text("sync-stage") == expected, text("sync-stage")

        def shot(name):
            if args.shots:
                args.shots.mkdir(parents=True, exist_ok=True)
                rpc.save_screenshot(item, args.shots / f"{name}-{viewport[0]}.png")

        stage("Step 1 of 6")
        assert "Illustrative" in text("sync-disclaimer")
        assert "local Unix socket" in text("sync-transport")
        for component in ("edit", "vault", "policy", "merge", "listen", "peer"):
            rpc.centre_of(f"sync-node:{component}")
            click(f"sync-node:{component}")
            for source_path in text("sync-source").split(" · "):
                assert (ROOT / source_path).is_file(), source_path
        click("sync-node:policy")
        assert text("sync-inspector-title") == "Permission check"
        assert "courier/src/access.rs" in text("sync-source")
        stage("Step 1 of 6")

        # The centre of the save connector is a point relative to measured node rectangles.
        rects = {r["id"]: r for r in rpc.rects()}
        edit, vault = rects["sync-node:edit"], rects["sync-node:vault"]
        x = edit["x"] + edit["w"] / 2
        y = (edit["y"] + edit["h"] + vault["y"]) / 2

        def connector_pixel():
            img = Image(base64.b64decode(rpc.screenshot(item)["png_base64"]))
            return img.at(x * img.width / viewport[0], y * img.height / viewport[1])

        inactive = connector_pixel()
        click("sync-next")
        stage("Step 2 of 6")
        assert near(connector_pixel(), "#55d6b1"), connector_pixel()
        assert not near(inactive, "#55d6b1"), inactive
        click("sync-prev")
        stage("Step 1 of 6")
        click("sync-prev")
        stage("Step 1 of 6")
        for _ in range(5):
            click("sync-next")
        stage("Step 6 of 6")
        assert text("sync-outcome") == "Both desktops have the accepted update"
        click("sync-next")
        stage("Step 6 of 6")
        shot("success")

        click("sync-scenario:offline")
        stage("Step 1 of 7")
        click("sync-play")
        rpc.frame(200)
        stage("Step 3 of 7")
        assert text("sync-play-label") == "Play"
        assert text("sync-outcome") == "Saved locally; waiting for the node"
        rpc.frame(200)
        stage("Step 3 of 7")
        shot("offline")
        click("sync-reconnect")
        stage("Step 4 of 7")
        assert text("sync-inspector-title") == "Permission check"
        for _ in range(3):
            click("sync-next")
        stage("Step 7 of 7")
        assert text("sync-outcome") == "Both desktops have the accepted update"

        click("sync-scenario:denied")
        for _ in range(3):
            click("sync-next")
        stage("Step 4 of 4")
        assert text("sync-outcome") == "Write refused; peer unchanged"
        assert "rollback" in text("sync-step-body").lower()
        assert text("sync-node-status:merge") == "Unchanged"
        assert text("sync-node-status:peer") == "Unchanged"
        shot("denied")
        assert not rpc.read_console(item), rpc.read_console(item)

        click("sync-scenario:success")
        click("sync-play")
        rpc.frame(90)
        stage("Step 2 of 6")
        before = text("sync-stage")
        for _ in range(3):
            rpc.dump_tree(item)
            rpc.screenshot(item)
        assert text("sync-stage") == before, "inspection advanced playback"
        click("sync-play")
        rpc.frame(180)
        stage("Step 2 of 6")
        click("sync-play")
        rpc.frame(400)
        stage("Step 6 of 6")
        assert text("sync-play-label") == "Play", "completion did not stop playback"
        click("sync-play")
        rpc.frame(15)
        click("sync-scenario:denied")
        stage("Step 1 of 4")
        assert text("sync-play-label") == "Play", "scenario switch kept playback running"
        assert text("sync-inspector-title") == "App document"
        click("sync-scenario:success")
        click("sync-restart")
        stage("Step 1 of 6")
        click("sync-node:merge")
        source = rpc.read_file_versioned(item, "main.lua")
        result = rpc.edit_file(item, "main.lua", source["revision"], [{
            "old_text": '"How sync works"', "new_text": '"How sync works, explored"',
        }])
        assert result["persisted"] and result["activation"] == "activated", result
        assert text("sync-inspector-title") == "Merge & persist"
        assert rpc.read_data(item) == {}, "viewing wrote shared document state"
        rpc.lock()
        rpc.unlock("explorer", "explainer test")
        rpc.open_item(item)
        stage("Step 1 of 6")
        assert text("sync-title") == "How sync works, explored"
        assert not rpc.read_console(item), rpc.read_console(item)
        shot("overview")
    print(f"sync explainer: pointer selection, paths, pixels, playback, reload and vault reopen at {viewport}")


if __name__ == "__main__":
    main()
