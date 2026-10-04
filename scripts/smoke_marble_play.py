"""A complete Lua level: ready, tilt, release, win/loss, frozen snapshots and retry."""
from pathlib import Path

from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def nodes(tree):
    yield tree
    for child in tree.get("children", []):
        yield from nodes(child)


def read(rpc, item):
    rpc.rects()
    tree = rpc.dump_tree(item)
    labels = {n["id"]: n.get("text", "") for n in nodes(tree) if n.get("id")}
    return tree["worlds3d"]["marble-level"], labels


def body(state, name):
    return next(e for e in state["entities"] if e["authored"]["id"] == name)


def finish(rpc, item, expected):
    # Frame(n) still renders/dispatches each frame; batching avoids three IPC round-trips
    # per frame while Lua pauses immediately on the result's normal-frame view.
    for _ in range(75):
        rpc.frame(4)
        state, labels = read(rpc, item)
        if labels["gates-status"].startswith(expected):
            return state, labels
        assert labels["gates-status"].startswith("Playing"), labels
    raise AssertionError(f"attempt did not finish as {expected}: {state}, {labels}")


with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    rpc = s.rpc
    rpc.signup("gates", "gates passphrase")
    ws = rpc.create_workspace("Marble Gates playable")
    item = rpc.create_item(ws["id"], "Marble Gates", "app")["id"]
    rpc.upload_folder(item, ROOT / "demo_apps" / "marble_gates")
    rpc.open_item(item)
    rpc.click_at(*rpc.centre_of("marble-play"))
    ready, labels = read(rpc, item)
    assert labels["gates-status"].startswith("Ready"), labels
    assert body(ready, "marble")["resolved"]["position"] == [0, 3, 0]
    rpc.advance(20)
    assert read(rpc, item)[0] == ready, "Ready must hold actual poses without time debt"
    shots = ROOT / "shots"
    shots.mkdir(exist_ok=True)
    rpc.save_screenshot(item, shots / "marble-gates-ready.png")
    rpc.click_at(*rpc.centre_of("gates-release"))
    won, labels = finish(rpc, item, "Won")
    assert "goal" in body(won, "marble")["zones"], won
    assert labels["gates-attempt"] == "Attempt 1", labels
    rpc.save_screenshot(item, shots / "marble-gates-won.png")
    rpc.save_screenshot(item, shots / "marble-gates-won-custom.png", width=1100, height=800, scale=1)
    rpc.advance(20)
    rpc.frame(30)
    assert read(rpc, item)[0] == won, "winning state must pause physics with no capture debt"
    rpc.click_at(*rpc.centre_of("gates-retry"))
    retry, labels = read(rpc, item)
    assert labels["gates-status"].startswith("Ready"), labels
    assert body(retry, "marble")["resolved"]["position"] == [0, 3, 0]
    assert body(retry, "marble")["resolved"]["velocity"] == [0, 0, 0]
    for _ in range(3):
        rpc.click_at(*rpc.centre_of("gates-tilt-left"))
    tilted, labels = read(rpc, item)
    assert body(tilted, "platform")["resolved"]["rotation"][2] > 0, tilted
    assert body(tilted, "platform")["authored"] == body(ready, "platform")["authored"]
    rpc.click_at(*rpc.centre_of("gates-release"))
    lost, labels = finish(rpc, item, "Lost")
    assert "fall" in body(lost, "marble")["zones"], lost
    assert body(lost, "marble")["resolved"]["position"][1] < -3, lost
    assert labels["gates-attempt"] == "Attempt 2", labels
    rpc.save_screenshot(item, shots / "marble-gates-lost.png")
    rpc.advance(20)
    assert read(rpc, item)[0] == lost, "loss must freeze at the detected fall, not keep falling"
    rpc.click_at(*rpc.centre_of("gates-retry"))
    for _ in range(3):
        rpc.click_at(*rpc.centre_of("gates-tilt-right"))
    rpc.click_at(*rpc.centre_of("gates-release"))
    second_win, labels = finish(rpc, item, "Won")
    assert labels["gates-attempt"] == "Attempt 3", labels
    assert second_win["dropped_zone_events"] == 0
    assert not any("error" in line.lower() for line in rpc.read_console(item)), rpc.read_console(item)
    print("marble play ok: real controls, ready/pause, tilt, release, win, loss, retry and captures")
