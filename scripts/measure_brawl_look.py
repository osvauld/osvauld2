"""Brawl step 4's pose-cost gate (docs/design/brawl-movement.md decision 7): two robots under 4 ms of
app-thread time a frame. Opens brawl offscreen with OSVAULD_FRAME_TIMINGS=1 and OSVAULD_PERFORMANCE=1,
turns on the hidden `pair` (a second robot, Brute, facing the other way, so every frame poses two),
and plays the 1 200-tick scripted run twice: the first fills the pose cache (misses), the second
runs on it. Prints the app's own pose timing (`look:cost`, wall clock around clip:at + pose) and
each thread's frame stages from the timing log, per run. Never builds: pass OSVAULD_SHELL_BINARY.

    OSVAULD_OFFSCREEN=900x700 OSVAULD_SHELL_BINARY=target/debug/shell2 python3 scripts/measure_brawl_look.py [--one]
"""

import math
import os
import re
import sys
import tempfile
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
os.environ["OSVAULD_FRAME_TIMINGS"] = "1"
os.environ["OSVAULD_PERFORMANCE"] = "1"
from osvauld.session import Session, shell_binary  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent


def find(tree, identity):
    if isinstance(tree, dict):
        if tree.get("id") == identity:
            return tree
        tree = tree.get("children", [])
    for child in tree:
        found = find(child, identity)
        if found:
            return found


def summarize(lines, label):
    groups = defaultdict(list)
    for line in lines:
        if not line.startswith("OSVAULD_FRAME ") or "capture=true" in line:
            continue
        thread = re.search(r"thread=(\S+)", line)
        groups[thread[1] if thread else "?"].append({k: float(v) for k, v in re.findall(r"(\w+_ms)=([\d.]+)", line)})
    for thread, rows in sorted(groups.items()):
        def pct(key, p):
            values = sorted(r[key] for r in rows)
            return values[max(0, math.ceil(len(values) * p) - 1)]
        print(f"  {label} thread {thread}: {len(rows)} frames; "
              f"view p50={pct('view_ms', .5):.2f} p95={pct('view_ms', .95):.2f} max={pct('view_ms', 1):.2f}; "
              f"total p50={pct('total_ms', .5):.2f} p95={pct('total_ms', .95):.2f} max={pct('total_ms', 1):.2f} ms")


def main():
    log = Path(tempfile.mkdtemp(prefix="brawl-look-", dir=os.environ.get("TMPDIR"))) / "stderr.log"
    with Session(shell_binary=shell_binary(), offscreen=(900, 700), stderr_path=str(log)) as s:
        r = s.rpc
        r.signup("measure", "brawl measure passphrase")
        ws = r.create_workspace("Brawl measure")
        item = r.create_item(ws["id"], "Brawl", "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps/brawl")
        r.open_item(item)
        for _ in range(120):
            r.frame(1)
            node = find(r.dump_tree(item), "art:loaded")
            if node and node["text"].split("/")[0] == node["text"].split("/")[1]:
                break
        pair = "--one" not in sys.argv  # --one: the baseline, a single robot
        if pair:
            r.click(item, "pair")
        print("robots:", 2 if pair else 1)
        perf = r.performance()
        print("build:", perf.get("profile"), "adapter:", perf.get("adapter"))
        for run in ("cold", "warm"):
            before = len(log.read_text().splitlines())
            r.click(item, "script")
            r.frame(2)
            for _ in range(61):
                r.frame(10)
            lines = log.read_text().splitlines()[before:]
            print(f"{run} run: {find(r.dump_tree(item), 'look:cost')['text']}")
            summarize(lines, run)
        perf = r.performance()
        print("performance:", {k: perf.get(k) for k in ("profile", "work_ms", "cpu_ms", "frames") if k in perf} or perf)
        console = r.read_console(item)
        assert not console, console


if __name__ == "__main__":
    main()
