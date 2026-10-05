# Performance benchmarks — numbers with budgets, run the same way every time

Status: **plan, 2026-10-04.** Grows out of [signals](signals.md) and the 2D world's crowd
measurements: those numbers lived in ad-hoc `eprintln!`s and status notes. This makes them a
suite with budgets, so a slowdown is a failure someone sees, not a number nobody reruns.

## 1. Done means

| # | check | proves |
|---|---|---|
| B1 | `python3 scripts/bench.py` builds release, runs every benchmark, prints one table (name, measured, budget, baseline) and exits non-zero if any is over budget | one command, a verdict |
| B2 | Each benchmark is an `#[ignore]`d test named `bench_*` printing `BENCH <name> <microseconds>`; the script finds them, nothing is listed by hand twice | adding one is writing one test |
| B3 | `scripts/bench_budgets.json` holds each budget and the last recorded baseline; `--record` rewrites the baselines; a result more than 25% over its baseline is a warning even under budget | regressions show before they break a budget |
| B4 | A world of 20000 entities builds its frame (the 4096-item cap is gone) and the bench says what it costs | the cap was a guess; this is the number |
| B5 | Signals T6 runs at 5000 coins, not 4000 | the cap no longer shapes the tests |
| B6 | The suite covers each layer the 2D world crosses: physics steps, reconcile (fresh and kept), frame build, frame encode into a vello scene, inspection, rays; in the app: a walking view, a pickup in one group and chunked, a signal update | a slowdown points at its layer |

Debug numbers are 10–30× off (Luau and mlua unoptimised), so the script always builds release;
a `bench_*` test run in debug says so and is not compared.

## 2. The frame item cap

`MAX_FRAME_ITEMS` (4096) was one of several frame budgets in `runtime/src/frame.rs`; it refused
any world past 4096 entities before culling exists, and culling only moves the limit (a
zoomed-out map shows everything). It goes. What it guarded — an app freezing the renderer — is
covered by the app thread's CPU budget. *Revised 2026-10-05:* the frame-wide path-command total
(`MAX_PATH_COMMANDS` summed over every instance) went too — at 20000 three-shape sprites it was
the next wall, and B4's numbers (3.4 ms to build, 2.5 ms to encode) say the cost is fine. A frame
still counts both (`stats`); one path stays capped at 65,536 commands in `Path::new`.

## 3. Budgets

Release, on the development machine. A budget is about twice the first measurement, rounded,
unless a product target is tighter (a 60 fps frame is 16.6 ms in all; a world's share of it
should stay under ~2 ms while nothing happens). Budgets live in `bench_budgets.json` with a note
each; changing one is a reviewed decision, like a test's expectation.

## 4. Steps

1. Remove `MAX_FRAME_ITEMS`, the frame-wide path total and their errors; their tests become
   "a frame counts but does not cap" and 20000 items build (B4). ✅
2. `bench_*` tests in `world`, `runtime` (frame encode — `draw` is crate-private) and `app_host`;
   the earlier ad-hoc measurements (`five_thousand_small_things_cost_this_much`, the ray timing,
   signal update, T6) become benchmarks or functional tests without timings. ✅
3. `scripts/bench.py` and `bench_budgets.json`; record the first baselines. ✅
4. Status and the guide's numbers point at the suite rather than restating figures. ✅
