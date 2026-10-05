#!/usr/bin/env python3
"""The benchmark suite: builds release, runs every `bench_*` test, and judges each number.

    python3 scripts/bench.py            # run and compare; exit 1 if anything is over budget
    python3 scripts/bench.py --record   # also store this run as the baselines

A benchmark is an `#[ignore]`d test named `bench_*` that prints `BENCH <name> <microseconds>`
(see `bench` in world/src/tests.rs). Its budget and last baseline live in
scripts/bench_budgets.json; a benchmark without a budget fails until one is written (or
`--record` starts one at twice this run). More than 25% over the baseline is a warning even
under budget. Plan: docs/design/perf-benchmarks.md.
"""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUDGETS = ROOT / "scripts" / "bench_budgets.json"
CRATES = ["world", "runtime", "app_host"]
DRIFT = 1.25


def run(crate):
    env = {**os.environ, "CARGO_BUILD_JOBS": os.environ.get("CARGO_BUILD_JOBS", "2")}
    cmd = ["cargo", "test", "-q", "--release", "-p", crate, "--lib", "--",
           "--ignored", "bench_", "--nocapture", "--test-threads=1"]
    done = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True)
    out = done.stdout + done.stderr
    if done.returncode != 0:
        sys.exit(f"benchmarks in {crate} failed:\n{out[-3000:]}")
    found = {}
    # Progress dots share a line with the output, so match anywhere.
    for name, us, debug in re.findall(r"BENCH (\S+) ([\d.]+)( debug)?", out):
        if debug:
            sys.exit(f"{name} ran in a debug build: its number means nothing")
        found[name] = float(us)
    return found


def main():
    record = "--record" in sys.argv[1:]
    budgets = json.loads(BUDGETS.read_text()) if BUDGETS.exists() else {}
    measured = {}
    for crate in CRATES:
        print(f"running {crate} benchmarks…", flush=True)
        measured.update(run(crate))
    failed, warned = [], []
    print(f"\n{'benchmark':<30} {'measured':>12} {'budget':>12} {'baseline':>12}")
    for name in sorted(measured):
        us = measured[name]
        entry = budgets.get(name)
        if entry is None:
            if record:
                budgets[name] = entry = {"budget_us": round(us * 2, -1), "note": "started at 2× the first run"}
            else:
                failed.append(f"{name}: no budget in {BUDGETS.name} (or run with --record)")
                print(f"{name:<30} {us:>10.0f}µs {'—':>12} {'—':>12}  NO BUDGET")
                continue
        budget, baseline = float(entry["budget_us"]), entry.get("baseline_us")
        verdict = ""
        if us > budget:
            verdict = "OVER BUDGET"
            failed.append(f"{name}: {us:.0f} µs > budget {budget:.0f} µs")
        elif baseline and us > baseline * DRIFT:
            verdict = f"+{(us / baseline - 1) * 100:.0f}% on baseline"
            warned.append(f"{name}: {verdict}")
        base = f"{baseline:>10.0f}µs" if baseline else f"{'—':>12}"
        print(f"{name:<30} {us:>10.0f}µs {budget:>10.0f}µs {base}  {verdict}")
        if record:
            entry["baseline_us"] = round(us, 1)
    for name in sorted(set(budgets) - set(measured)):
        failed.append(f"{name}: has a budget but no benchmark printed it")
    if record:
        BUDGETS.write_text(json.dumps(budgets, indent=2, sort_keys=True) + "\n")
        print(f"\nbaselines recorded in {BUDGETS.relative_to(ROOT)}")
    for w in warned:
        print(f"warning: {w}")
    if failed:
        print("\n" + "\n".join(f"FAIL {f}" for f in failed))
        sys.exit(1)
    print(f"\n{len(measured)} benchmarks within budget")


if __name__ == "__main__":
    main()
