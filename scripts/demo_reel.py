#!/usr/bin/env python3
"""Record the landing-page demo reel: one mp4 (and GIF with --gif) per showcase app.

    python3 scripts/demo_reel.py [clip ...] [--out shots/reel] [--gif] [--fps 24]

Offscreen and frame-exact like `demo_pointer_video.py`: same script, same video. Clips are
independent (a fresh shell each), so one failing doesn't cost the others. Use a release binary:
`OSVAULD_SHELL_BINARY=target/release/shell2`.
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.record import Recorder
from osvauld.session import Session, build_shell, shell_binary
from agent_builds_src import EDITS, STATS_LUA, seed_dir

ROOT = Path(__file__).resolve().parent.parent
APPS = ROOT / "demo_apps"
CHROME_TOP = 38  # the shell's tab bar; the landing page wants the app only


def press(rec: Recorder, code: str, key: str, secs: float) -> None:
    """Hold a key for `secs` of recorded time."""
    rec.step(lambda: rec.rpc.keyboard(code, key, True))
    rec.hold(secs)
    rec.step(lambda: rec.rpc.keyboard(code, key, False))


def kanban(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.5)
    for card, col in (("k1", "k3"), ("k2", "k4")):
        try:
            rec.glide_to(card, 0.6)
            rec.drag_to(col, 1.0)
        except RuntimeError:
            continue
        rec.hold(0.5)
    rec.glide_to("open-col", 0.7)
    rec.click()
    rec.hold(0.8)


def marble_gates(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.4)
    rec.glide_to("marble-play", 0.6)
    rec.click()
    rec.hold(0.6)
    rec.glide_to("gates-release", 0.6)
    rec.click()
    rec.hold(3.0)  # the marble rolls on the app's own physics
    rec.glide_to("gates-retry", 0.5)
    rec.click()
    for _ in range(3):
        rec.glide_to("gates-tilt-left", 0.3)
        rec.click()
    rec.glide_to("gates-release", 0.4)
    rec.click()
    rec.hold(3.0)


def model_viewer(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.4)
    rec.glide_to("scene-viewport", 0.6)
    x, y = rec.pos
    rec.click()
    rec.hold(0.3)
    rec.step(rpc.press)
    rec.glide_to((x + 220, y - 60), 1.0)  # orbit
    rec.glide_to((x - 160, y + 90), 1.0)
    rec.step(rpc.release)
    for dy in (-30,) * 12 + (30,) * 12:  # wheel zoom in, then back out
        rec.step(lambda dy=dy: rpc.wheel(x, y, 0, dy))
    rec.hold(0.5)


def tank(rec: Recorder, rpc, item: str) -> None:
    rec.glide_to("arena", 0.4)
    rec.hold(0.4)
    for code, key, secs in (("KeyD", "d", 0.8), ("KeyW", "w", 0.8), ("KeyA", "a", 1.0),
                            ("KeyS", "s", 0.8), ("KeyD", "d", 0.8), ("KeyW", "w", 0.8)):
        press(rec, code, key, secs)
    rec.hold(1.0)


def dashboard(rec: Recorder, rpc, item: str) -> None:
    plot = next(r for r in rpc.rects() if r["id"].startswith("plot:"))
    y = plot["y"] + plot["h"] / 2
    left, right = plot["x"] + 8, plot["x"] + plot["w"] - 8
    rec.hold(0.6)
    rec.glide_to((left, y), 0.7)
    rec.glide_to((right, y), 2.2)  # hover: every chart reads the same month
    rec.glide_to((left + plot["w"] * 0.25, y), 0.9)
    rec.step(rpc.press)
    rec.glide_to((left + plot["w"] * 0.7, y), 1.2)  # drag a range: the tiles recompute
    rec.step(rpc.release)
    rec.hold(1.2)
    rec.glide_to("clear", 0.8)
    rec.click()
    rec.hold(0.8)


def world(rec: Recorder, rpc, item: str) -> None:
    rec.glide_to("room", 0.5)
    rec.hold(0.4)
    for code, key, secs in (("KeyD", "d", 1.2), ("KeyS", "s", 0.9), ("KeyA", "a", 1.6),
                            ("KeyW", "w", 0.9), ("KeyD", "d", 0.8)):
        press(rec, code, key, secs)
        if code == "KeyA":
            press(rec, "Space", " ", 0.3)
    rec.hold(0.8)


def orbit(rec: Recorder, rpc, item: str) -> None:
    f = next(r for r in rpc.rects() if r["id"] == "field")
    cx, cy = f["x"] + f["w"] / 2, f["y"] + f["h"] / 2
    rec.glide_to((f["x"] + 60, cy), 0.5)
    for dx, dy in ((0.8, -0.3), (0.3, 0.35), (-0.6, -0.25), (-0.1, 0.4)):
        rec.glide_to((cx + dx * f["w"] / 2, cy + dy * f["h"] / 2), 1.2)
    rec.click()
    rec.hold(0.6)
    rec.step(rpc.press)
    rec.glide_to((cx - 200, cy + 80), 1.0)
    rec.step(rpc.release)
    rec.hold(1.5)


def linkage(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.4)
    rec.glide_to("wave", 0.6)
    rec.click()
    rec.hold(3.0)
    rec.glide_to("reset", 0.5)
    rec.click()
    for _ in range(4):
        rec.glide_to("inc:elbow", 0.3)
        rec.click()
    rec.glide_to("inc:shoulder", 0.3)
    rec.click()
    rec.click()
    rec.hold(1.0)


def voronoi(rec: Recorder, rpc, item: str) -> None:
    c = next(r for r in rpc.rects() if r["id"] == "canvas")
    pts = [(0.2, 0.3), (0.7, 0.25), (0.45, 0.6), (0.85, 0.7), (0.15, 0.8), (0.55, 0.2)]
    for fx, fy in pts:
        rec.glide_to((c["x"] + fx * c["w"], c["y"] + fy * c["h"]), 0.5)
        rec.click()
        rec.hold(0.3)
    rec.glide_to((c["x"] + 0.45 * c["w"], c["y"] + 0.6 * c["h"]), 0.5)
    rec.step(rpc.press)
    rec.glide_to((c["x"] + 0.6 * c["w"], c["y"] + 0.45 * c["h"]), 1.0)
    rec.glide_to((c["x"] + 0.3 * c["w"], c["y"] + 0.55 * c["h"]), 1.0)
    rec.step(rpc.release)
    rec.hold(0.8)


def node_graph(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.5)
    rec.glide_to("port:ingest", 0.7)
    rec.drag_to("node:validate", 1.2)  # wire a new edge
    rec.hold(0.5)
    rec.glide_to("label:enrich", 0.6)
    rec.drag_to((700, 200), 1.2)  # move a node, its edges follow
    rec.hold(0.4)
    rec.glide_to("label:parse", 0.6)
    rec.drag_to((420, 460), 1.2)
    rec.hold(0.4)
    rec.glide_to("add-node", 0.6)
    rec.click()
    rec.hold(1.0)


def hockey(rec: Recorder, rpc, item: str) -> None:
    rec.glide_to((500, 350), 0.4)
    rec.hold(0.4)
    for (c1, k1), (c2, k2), secs in (
        (("KeyD", "d"), ("ArrowLeft", "ArrowLeft"), 0.6),
        (("KeyW", "w"), ("ArrowDown", "ArrowDown"), 0.5),
        (("KeyD", "d"), ("ArrowUp", "ArrowUp"), 0.6),
        (("KeyS", "s"), ("ArrowLeft", "ArrowLeft"), 0.6),
        (("KeyD", "d"), ("ArrowDown", "ArrowDown"), 0.6),
    ):
        rec.step(lambda: rpc.keyboard(c1, k1, True))
        rec.step(lambda: rpc.keyboard(c2, k2, True))
        rec.hold(secs)
        rec.step(lambda: rpc.keyboard(c1, k1, False))
        rec.step(lambda: rpc.keyboard(c2, k2, False))
    rec.hold(1.5)


def hero(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.4)
    for pose in ("wave", "step", "lean", "rest", "wave"):
        rec.glide_to(f"pose:{pose}", 0.6)
        rec.click()
        rec.hold(0.8)


def bharat_compass(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.5)
    rec.glide_to("activity:c1_morning_direction", 0.7)
    rec.click()
    rec.hold(0.6)
    rec.glide_to("option:c1_morning_direction:east", 0.7)
    rec.click()
    rec.glide_to("check:c1_morning_direction", 0.5)
    rec.click()
    rec.hold(1.0)
    rec.glide_to("next", 0.5)
    rec.click()
    rec.hold(1.0)


def math_mela(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.5)
    rec.glide_to("activity:choose", 0.7)
    rec.click()
    rec.hold(0.6)
    rec.glide_to("choose:c1-choose-seven-guavas:seven", 0.7)
    rec.click()
    rec.glide_to("check:c1-choose-seven-guavas", 0.5)
    rec.click()
    rec.hold(1.2)


def mesh_math(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.6)
    for b in ("wave-more", "wave-rotate", "wave-flat", "wave-more"):
        rec.glide_to(b, 0.5)
        rec.click()
        rec.hold(1.4)
    v = next(r for r in rpc.rects() if r["id"] == "mesh-view")
    cx, cy = v["x"] + v["w"] / 2, v["y"] + v["h"] / 2
    rec.glide_to((cx - 100, cy), 0.6)
    rec.step(rpc.press)
    rec.glide_to((cx + 160, cy - 40), 1.2)
    rec.step(rpc.release)
    rec.hold(1.0)


def tilt_maze(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.5)
    for b in ("east", "east", "south", "release"):
        rec.glide_to(b, 0.4)
        rec.click()
    rec.hold(2.0)
    for b in ("retry", "north", "west", "release"):
        rec.glide_to(b, 0.4)
        rec.click()
    rec.hold(2.0)


def camera(rec: Recorder, rpc, item: str) -> None:
    rec.glide_to("map", 0.4)
    rec.hold(0.4)
    for code, key, secs in (("KeyD", "d", 1.6), ("KeyS", "s", 1.2), ("KeyD", "d", 1.4),
                            ("KeyW", "w", 1.6), ("KeyA", "a", 2.2)):
        press(rec, code, key, secs)
    rec.hold(0.6)


def khoj(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.5)
    rec.glide_to("activity:classify", 0.7)
    rec.click()
    rec.hold(0.6)
    for item_, kind in (("butterfly", "living"), ("kite", "nonliving"), ("neem", "living"),
                        ("pot", "nonliving")):
        rec.glide_to(f"answer:c1-living:{item_}:{kind}", 0.5)
        rec.click()
        rec.hold(0.2)
    rec.glide_to("check:c1-living", 0.5)
    rec.click()
    rec.hold(1.5)


def pie(rec: Recorder, rpc, item: str) -> None:
    rec.hold(0.4)
    for k in ("search", "direct", "social", "mail", "refer", "other"):
        rec.glide_to(f"legend:{k}", 0.5)
        rec.hold(0.5)
    p = next(r for r in rpc.rects() if r["id"] == "pie")
    rec.glide_to((p["x"] + p["w"] * 0.7, p["y"] + p["h"] * 0.4), 0.6)
    rec.hold(0.8)
    rec.glide_to((p["x"] + p["w"] * 0.25, p["y"] + p["h"] * 0.65), 0.8)
    rec.hold(0.8)


def line_chart(rec: Recorder, rpc, item: str) -> None:
    p = next(r for r in rpc.rects() if r["id"] == "plot")
    y = p["y"] + p["h"] * 0.5
    rec.hold(0.5)
    rec.glide_to((p["x"] + 10, y), 0.6)
    rec.glide_to((p["x"] + p["w"] - 10, y + 20), 3.0)
    rec.glide_to((p["x"] + p["w"] * 0.4, y - 30), 1.5)
    rec.hold(0.8)


def agent_edit(rec: Recorder, rpc, item: str) -> None:
    """An agent rewrites the running app's source through the bridge (read_file_versioned +
    edit_file, the same path the MCP-less agents use). The count lives in the document, so it
    survives every edit. The caption shows the edit a beat before the app changes."""

    def edit(caption: str, *pairs: tuple[str, str]) -> None:
        rec.caption("agent edits main.lua while the app runs\n" + caption)
        rec.hold(1.4)
        cur = rpc.read_file_versioned(item, "main.lua")
        rpc.edit_file(item, "main.lua", cur["revision"],
                      [{"old_text": a, "new_text": b} for a, b in pairs])
        rpc.frame(2)
        rec.rebase()
        rec.hold(1.6)
        rec.caption(None)

    rec.hold(0.5)
    for _ in range(5):
        rec.glide_to("plus", 0.3)
        rec.click()
    rec.hold(0.8)
    edit("- accent = #2f81f7\n+ accent = #ff5d8f",
         ('accent = "#2f81f7"', 'accent = "#ff5d8f"'),
         ('accent_hi = "#4493f8"', 'accent_hi = "#ff7fa6"'),
         ('{ "TALLY", id', '{ "TALLY - live", id'))
    rec.glide_to("plus", 0.4)
    rec.click()
    rec.hold(0.6)
    edit("+ pill(\"plus10\", \"+10\", 10, ...)",
         ('pill("plus", "+", 1, C.accent, C.accent_hi),',
          'pill("plus", "+", 1, C.accent, C.accent_hi),\n\t\t\tpill("plus10", "+10", 10, C.accent, C.accent_hi),'))
    rec.glide_to("plus10", 0.5)
    rec.click()
    rec.click()
    rec.hold(0.8)
    edit("~ colours: bg, card, text",
         ('bg = "#0d1117"', 'bg = "#1a1033"'), ('card = "#1c2128"', 'card = "#2a1b52"'),
         ('text = "#e6edf3"', 'text = "#ffd166"'))
    rec.hold(0.8)


def physics_playground(rec: Recorder, rpc, item: str) -> None:
    rack = next(r for r in rpc.rects() if r["id"] == "rack")
    ox, oy = 60, rack["y"] - 12 - 500  # the 880x500 table, centred above the button row
    cue = (ox + 150 + 14, oy + 236 + 14)
    rec.hold(0.4)
    rec.glide_to("rack", 0.6)
    rec.click()
    rec.hold(0.6)
    rec.glide_to(cue, 0.7)
    rec.step(rpc.press)
    rec.glide_to((cue[0] - 110, cue[1] + 80), 0.9)  # pull back: the dots show the bank shot
    rec.hold(0.4)
    rec.step(rpc.release)
    rec.hold(4.0)
    rec.glide_to("burst", 0.7)
    rec.click()
    rec.hold(5.5)


def agent_builds(rec: Recorder, rpc, item: str) -> None:
    """An agent grows a to-do app through the bridge: six edits (one new file, one broken), the
    document surviving every reload. Sources and exact replacements: agent_builds_src.py."""
    diff = {  # what the caption shows; the real replacements are in EDITS
        "chips": ("priority chips", "+ local function chip(p) ... end", "+ chip(task.prio),"),
        "filters": ("filter chips with live counts", "+ local function filters(ux, total, done)",
                    "+ filters(ux, #t.tasks, done),"),
        "ring": ("a progress ring drawn with gfx", "+ gfx.stroke({ path = ..., width = 7, cap = round })",
                 "+ ui.frame({ visual = ring(steps) }),"),
        "stats": ("bars by priority, a new file", '+ local stats = require("stats")',
                  "+ stats.view(t.tasks),"),
        "composer": ("add-task composer, Enter submits", "+ ui.input({ id = draft, on_enter = add })",
                     '+ t:insert({ "tasks" }, doc.map({ id = uuid(), ... }))'),
        "typo": ("restyle the rows", "- gap = 12, radius = 10,", "+ gap = 12, raduis = 10,"),
        "fix": ("fix the typo the host reported", "- gap = 12, raduis = 10,", "+ gap = 12, radius = 10,"),
    }

    def edit(key: str) -> None:
        what, a, b = diff[key]
        verb = "write_file stats.lua, edit_file" if key == "stats" else "edit_file main.lua"
        rec.caption(f"agent> {verb}  --  {what}\n{a}\n{b}")
        rec.hold(1.2)
        if key == "stats":
            rpc.write_file(item, "stats.lua", STATS_LUA)
        cur = rpc.read_file_versioned(item, "main.lua")
        res = rpc.edit_file(item, "main.lua", cur["revision"],
                            [{"old_text": o, "new_text": n} for o, n in EDITS[key][1]])
        assert res["persisted"] and res["activation"] == "activated", res
        rpc.frame(2)
        rec.rebase()
        rec.hold(1.3)

    def tap(*ids: str, pause: float = 0.5) -> None:
        for i in ids:
            rec.glide_to(i, 0.5)
            rec.click()
            rec.hold(pause)

    rec.hold(0.6)
    tap("task:t1", pause=0.7)  # a plain checklist: the document is the app's state
    edit("chips")
    tap("task:t3")
    rec.caption(None)
    edit("filters")
    tap("filter:open", "filter:done", "filter:all", pause=0.7)
    rec.caption(None)
    edit("ring")
    tap("task:t4", pause=1.0)
    rec.caption(None)
    edit("stats")
    tap("task:t6", pause=1.0)
    rec.caption(None)
    edit("composer")
    rec.glide_to("draft", 0.5)
    rec.click()
    text = "Ship it on Friday"
    for n in range(1, len(text) + 1):
        rec.step(lambda n=n: rpc.type_text(item, "draft", text[:n]))
        rec.hold(0.06)
    rec.hold(0.3)
    rec.step(lambda: rpc.key(item, "draft", "enter"))
    rec.hold(1.4)
    rec.caption(None)
    edit("typo")  # the host flags the unknown prop in place and keeps everything else alive
    rec.caption("host> unknown prop 'raduis' flagged in place\n"
                "the rest of the app stays alive; the agent fixes one line")
    rec.hold(2.2)
    edit("fix")
    tap("task:t2", pause=0.8)  # the new task and every check survived all of it
    rec.caption(None)
    rec.hold(1.2)


def asset_editor(rec: Recorder, rpc, item: str) -> None:
    """A drawing tool written in Lua, over shared asset files: play a clip, recolour from the
    inspector, drag a point of the drawing, then use the room made in it as a map, and the toggle
    it made as a widget in another app."""
    ZOOM = 2.5  # the canvas draws the drawing at 2.5x from its top-left, camera unzoomed

    def hero():
        return rpc.read_data(item)["asset:hero"]

    rec.hold(0.6)
    for clip, secs in (("clip:walk", 1.6), ("clip:jump", 1.3), ("clip:idle", 0.4)):
        rec.glide_to(clip, 0.45)
        rec.click()
        rec.hold(secs)
    rec.glide_to("mode:edit", 0.5)  # a clip plays in place of the canvas; edit brings it back
    rec.click()
    rec.hold(0.5)
    rec.caption("inspector: shirt and hair are variables of the asset")
    for var, colour in (("var:shirt", "#22c55e"), ("var:hair", "#f59e0b")):
        rec.glide_to(var, 0.5)
        rec.click()
        rec.step(lambda v=var, c=colour: (rpc.type_text(item, v, c), rpc.key(item, v, "enter")))
        rec.rebase()
        rec.hold(0.9)
    rec.caption(None)
    rec.glide_to("part:arm_near", 0.5)
    rec.click()
    rec.hold(0.4)
    rec.caption("drag a point: the asset file itself changes")
    canvas = next(r for r in rpc.rects() if r["id"] == "canvas")

    def hand_end():
        arm = next(p for p in hero()["views"]["front"]["parts"] if p["id"] == "arm_near")
        return arm["shapes"][1]["path"][1][5:7]

    before = hand_end()
    at = (canvas["x"] + before[0] * ZOOM, canvas["y"] + before[1] * ZOOM)
    rec.glide_to(at, 0.6)
    rec.drag_to((at[0] + 75, at[1] + 100), 1.2)  # 30 x 40 drawing units, on the 1x snap
    rec.hold(0.8)
    assert hand_end() != before, "the point drag did not reach the asset"
    rec.glide_to((at[0] + 200, at[1] - 150), 0.5)
    rec.glide_to("clip:walk", 0.5)
    rec.click()
    rec.hold(1.8)
    rec.caption(None)
    rec.glide_to("mode:edit", 0.4)
    rec.click()
    rec.glide_to("asset:room", 0.6)
    rec.click()
    rec.hold(0.6)
    rec.glide_to("stage:play", 0.5)
    rec.click()
    rec.caption("a room made in the editor plays as a map")
    rec.glide_to((440, 560), 0.4)  # the running world has no rect; keys need no hover
    press(rec, "KeyD", "d", 1.8)  # stops at the chest's collider
    press(rec, "KeyA", "a", 2.4)  # walks back, pushes the crate
    rec.hold(0.5)
    rec.caption(None)
    rec.glide_to("asset:toggle", 0.5)
    rec.click()
    rec.hold(0.5)
    ws = rpc.list_workspaces()[0]["id"]
    demo = rpc.create_item(ws, "toggle demo", "app")["id"]
    rpc.upload_folder(demo, APPS / "toggle_demo")
    rpc.open_item(demo)
    rpc.frame(3)
    rec.item = demo
    rec.rebase()
    rec.caption("the same asset, used as a widget in another app")
    rec.hold(0.8)
    for _ in range(2):
        rec.glide_to("toggle", 0.6)
        rec.click()
        rec.hold(1.2)
    rec.caption(None)


# name → (app folder, item title, viewport, scripted clip)
CLIPS = {
    "kanban": ("kanban", "Kanban", (1100, 720), kanban),
    "marble_gates": ("marble_gates", "Marble Gates", (900, 700), marble_gates),
    "model_viewer": ("model_viewer", "Model viewer", (1000, 700), model_viewer),
    "tank": ("tank", "Tank", (900, 700), tank),
    "dashboard": ("dashboard", "Dashboard", (1100, 700), dashboard),
    "world": ("world", "World", (1000, 700), world),
    "orbit": ("orbit", "Orbit", (1000, 700), orbit),
    "linkage": ("linkage", "Linkage", (1000, 700), linkage),
    "voronoi": ("voronoi", "Voronoi", (1000, 700), voronoi),
    "node_graph": ("node_graph", "Node graph", (1000, 700), node_graph),
    "asset_editor": ("asset_editor", "Asset editor", (1100, 720), asset_editor),
    "hockey": ("hockey", "Hockey", (1000, 700), hockey),
    "hero": ("hero", "Hero", (1000, 700), hero),
    "bharat_compass": ("bharat_compass", "Bharat Compass", (1000, 700), bharat_compass),
    "math_mela": ("math_mela", "Math Mela", (1000, 700), math_mela),
    "agent_edit": ("tally", "Tally", (340, 400), agent_edit),
    "physics_playground": ("physics_playground", "Physics playground", (1000, 700), physics_playground),
    "mesh_math": ("mesh_math", "Mesh math", (1000, 700), mesh_math),
    "tilt_maze": ("tilt_maze", "Tilt maze", (1000, 700), tilt_maze),
    "camera": ("camera", "Camera", (1000, 700), camera),
    "khoj": ("khoj", "Khoj", (1000, 700), khoj),
    "pie": ("pie", "Pie", (1000, 700), pie),
    "line_chart": ("line_chart", "Line chart", (1000, 700), line_chart),
    "agent_builds": (seed_dir(), "Launch checklist", (900, 700), agent_builds),
}


def record(name: str, out_dir: Path, fps: int, gif: bool) -> None:
    folder, title, viewport, clip = CLIPS[name]
    out = out_dir / f"{name}.mp4"
    with Session(shell_binary=shell_binary(release=True), offscreen=viewport) as s:
        rpc = s.rpc
        rpc.signup("reel", "correct horse")
        ws = rpc.create_workspace("reel")
        item = rpc.create_item(ws["id"], title, "app")["id"]
        rpc.upload_folder(item, APPS / folder)
        rpc.open_item(item)
        rpc.frame(3)
        with Recorder(rpc, item, out, fps=fps, start=(viewport[0] // 2, viewport[1] - 40), gif=gif,
                      crop_top=CHROME_TOP, upscale=2 if name == "agent_edit" else 1) as rec:
            clip(rec, rpc, item)
        errors = rpc.read_console(item)
    print(f"{name}: {rec.frames} frames → {out}" + (f"  console: {errors[:3]}" if errors else ""))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("clips", nargs="*", choices=[*CLIPS, []], default=[], help="default: all")
    ap.add_argument("--out", type=Path, default=ROOT / "shots" / "reel")
    ap.add_argument("--fps", type=int, default=24)
    ap.add_argument("--gif", action="store_true")
    args = ap.parse_args()

    build_shell(release=True)
    failed = []
    for name in args.clips or CLIPS:
        try:
            record(name, args.out, args.fps, args.gif)
        except Exception as e:  # keep going: one clip's failure is not the reel's
            failed.append(name)
            print(f"{name}: FAILED — {e}", file=sys.stderr)
    if failed:
        raise SystemExit(f"failed: {', '.join(failed)}")


if __name__ == "__main__":
    main()
