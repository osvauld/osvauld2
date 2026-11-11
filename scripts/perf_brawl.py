"""Brawl frame cost: the scripted 1 200-tick timeline, with the shell's own per-frame work samples.

Not a smoke (it asserts nothing about speed): run it against a release shell and read the table.
Offscreen there is no presented frame, so no real FPS; `work_ms` is the wall time the shell spent
building one frame (Lua, layout, paint, render), and 1000 / work_ms is the ceiling it would allow.

    OSVAULD_SHELL_BINARY=$PWD/target/release/shell2 python3 scripts/perf_brawl.py [WxH ...]
"""
import os
import sys
import time
from pathlib import Path

os.environ["OSVAULD_PERFORMANCE"] = "1"
from osvauld.session import Session, shell_binary  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
FRAMES = 600  # the script's 1 200 ticks at 2 ticks a frame
BATCH = 200   # under the shell's 240-sample window, so no frame is counted twice or lost


def run(size):
    with Session(shell_binary=shell_binary(), offscreen=size) as s:
        r = s.rpc
        r.signup("perf", "brawl perf passphrase")
        ws = r.create_workspace("Brawl perf")
        item = r.create_item(ws["id"], "Brawl", "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps/brawl")
        r.open_item(item)
        r.frame(120)  # art decoded, pose cache warm for idle
        r.click(item, "script")
        rows, wall = [], 0.0
        for _ in range(FRAMES // BATCH):
            t = time.perf_counter()
            r.frame(BATCH)
            wall += time.perf_counter() - t
            time.sleep(0.3)  # diagnostics publish on wall time
            r.frame(1)
            rows.append(r.request("Performance"))
        console = r.read_console(item)
        assert not console, console
    p50 = sorted(x["work_ms"]["p50"] for x in rows)[len(rows) // 2]
    p95 = max(x["work_ms"]["p95"] for x in rows)
    worst = max(x["work_ms"]["max"] for x in rows)
    last = rows[-1]
    return {
        "size": f"{size[0]}x{size[1]}", "profile": last["profile"], "adapter": last["adapter"],
        "p50": p50, "p95": p95, "max": worst, "over_budget": last["over_budget"],
        "driven_ms": 1000 * wall / FRAMES,
    }


def main():
    sizes = [tuple(int(n) for n in a.split("x")) for a in sys.argv[1:]] or [(1280, 720), (1920, 1080)]
    out = [run(size) for size in sizes]
    print(f"profile {out[0]['profile']}, adapter {out[0]['adapter']}, {FRAMES} frames of the scripted run")
    print(f"{'size':>10} {'p50 ms':>8} {'p95 ms':>8} {'max ms':>8} {'ceiling fps (p95)':>18} {'over 16.7 ms':>13} {'driven ms/frame':>16}")
    for o in out:
        print(f"{o['size']:>10} {o['p50']:8.2f} {o['p95']:8.2f} {o['max']:8.2f} {1000 / o['p95']:18.0f} {o['over_budget']:>13} {o['driven_ms']:16.2f}")


if __name__ == "__main__":
    main()
