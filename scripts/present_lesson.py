"""Open a Lua lesson, export its actual presenter notes, or record its audience view.

Run through LoadTable with a previously built OSVAULD_SHELL_BINARY. The default subject is
the CRDT counter, but export and recording depend only on the bundled presenter's UI contract.
Video is silent; the Markdown script is for the human presenter to narrate.
"""

import argparse
import os
from pathlib import Path

from osvauld.record import Recorder
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def nodes(node):
    yield node
    for child in node.get("children", []):
        yield from nodes(child)


def collect(rpc, item):
    rpc.click(item, "mode:presenter")
    scene_ids = [n["id"][6:] for n in nodes(rpc.dump_tree(item)) if n.get("id", "").startswith("scene:")]
    scenes = []
    for scene_id in scene_ids:
        rpc.click(item, "scene:" + scene_id)
        current = list(nodes(rpc.dump_tree(item)))
        texts = {n["id"]: n.get("text", "") for n in current if "id" in n}
        scenes.append({
            "id": scene_id,
            "title": texts["lesson-scene-title"],
            "summary": texts["lesson-summary"],
            "narration": texts["presenter-narration"],
            "cue": texts["presenter-cue"],
            "takeaway": texts["lesson-takeaway"],
            "seconds": int(texts["lesson-time"].split(" / ")[1][:-1]),
            "actions": [n["text"] for n in current if n.get("id", "").startswith("presenter-action:")],
        })
    assert scenes, "lesson has no scenes"
    return scenes


def write_script(scenes, output):
    lines = ["# Presenter script", "", "Silent visuals accompany this script. Cue timings are rehearsal defaults, not an audio alignment.", ""]
    for i, scene in enumerate(scenes, 1):
        lines += [f"## {i}. {scene['title']}", "", f"Scene `{scene['id']}` · suggested visual duration: {scene['seconds']} seconds.", "", scene["summary"], "", scene["narration"], "", f"**Demonstration:** {scene['cue']}", "", "**Playback cues:**", ""]
        lines += ["- " + action for action in scene["actions"]]
        lines += ["", f"**Takeaway:** {scene['takeaway']}", ""]
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines))


def key(rpc, code, value, down):
    rpc.keyboard(code, value, down)


def record(rpc, item, scenes, output, fps=12, preview=None):
    rpc.click(item, "scene:" + scenes[0]["id"])
    rpc.click(item, "mode:record")
    root = next(r for r in rpc.rects() if r["id"] == "lesson-root")
    shot = rpc.screenshot(item)
    scale = shot["width_px"] / root["w"]
    with Recorder(rpc, item, output, fps=fps, crop_top=round(root["y"] * scale)) as recorder:
        for i, scene in enumerate(scenes):
            if i:
                recorder.step(lambda: key(rpc, "ArrowRight", "ArrowRight", True))
                recorder.step(lambda: key(rpc, "ArrowRight", "ArrowRight", False))
            count = max(2, round((preview or scene["seconds"]) * fps))
            recorder.step(lambda: key(rpc, "Space", " ", True))
            recorder.step(lambda: key(rpc, "Space", " ", False))
            for _ in range(count - 2):
                recorder.step()
        frames = recorder.frames
    assert not rpc.read_console(item), rpc.read_console(item)
    return frames


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", nargs="?", type=Path, default=ROOT / "demo_apps/crdt_lesson")
    parser.add_argument("--script", type=Path)
    parser.add_argument("--video", type=Path)
    parser.add_argument("--preview", type=float, help="record this many seconds of each scene")
    parser.add_argument("--fps", type=int, default=12)
    parser.add_argument("--open", action="store_true", help="leave the lesson open in a visible window")
    args = parser.parse_args()
    if args.preview is not None and args.preview <= 0:
        parser.error("--preview must be positive")
    if args.fps < 1 or args.fps > 30:
        parser.error("--fps must be between 1 and 30")
    if args.open and args.video:
        parser.error("recording uses an offscreen session; run --open separately")
    if args.open:
        os.environ.pop("OSVAULD_OFFSCREEN", None)
    with Session(shell_binary=shell_binary(), offscreen=None if args.open else (1280, 720), startup_timeout=30) as session:
        rpc = session.rpc
        rpc.signup("presenter", "lesson demo")
        ws = rpc.create_workspace("Interactive lessons")
        item = rpc.create_item(ws["id"], args.folder.name, "app")["id"]
        rpc.upload_folder(item, args.folder)
        rpc.open_item(item)
        assert not rpc.read_console(item), rpc.read_console(item)
        if args.script or args.video:
            scenes = collect(rpc, item)
            if args.script:
                write_script(scenes, args.script)
                print(f"Presenter script: {args.script}", flush=True)
            if args.video:
                args.video.parent.mkdir(parents=True, exist_ok=True)
                frames = record(rpc, item, scenes, args.video, args.fps, args.preview)
                print(f"Silent video: {args.video} ({frames} frames at {args.fps} fps)", flush=True)
        if args.open:
            rpc.click(item, "mode:explore")
            print(f"Lesson is open. Socket: {session.socket_path}; item: {item}", flush=True)
            session.process.wait()


if __name__ == "__main__":
    main()
