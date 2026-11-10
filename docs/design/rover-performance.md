# Rover performance and live FPS

Status: **implemented and verified, 2026-11-05**, in the uncommitted `moon-rover` worktree. Measure the first
physical rover before expanding the landscape or its visual model. Build/test on the NixOS
worker via LoadTable `--peer nixos`; the laptop runs the resulting preview and is measured
separately. The user's existing exception skips `pi` review, not verification.

## End-to-end acceptance

Write these checks first, observe failure, then implement:

- `smoke_rover_performance.py` uploads the real rover into an isolated vault, enables the
  runtime overlay, drives actual inputs, and reads native `Performance` snapshots. Offscreen
  snapshots must report no display FPS despite the 60 Hz virtual driver. Frame samples are
  finite, bounded and inspectable; paused screenshots preserve native world state.
- Capture real overlay pixels above the shell/app, without introducing hit regions or changing
  app geometry. Normal captures may show the last sample but never become performance samples.
- In a live release preview, sample completed presentations with a monotonic wall clock and
  report FPS, work-time p50/p95/max, viewport, build profile and graphics adapter. A bounded
  sample window and cached overlay text avoid per-frame sorting/formatting. Diagnostics must
  not request extra frames or execute Lua.
- Audit the actual world: lunar gravity, body/vehicle/wheel counts, collision and visual triangle
  counts, finite poses/speeds, suspension travel, paused clocks and dropped simulation time.
  Reuse existing drive/collision/control smokes rather than duplicating their assertions.

Native tests use an injected clock to prove FPS uses completed presentations, captures/tiles
do not count as display frames, idle/stale FPS is unavailable, samples are bounded, and disabled
diagnostics have no per-frame clocks/allocations. Shell tests pin the observational wire shape.
Run workspace check/tests and the full smoke suite before landing runtime/shell changes.

## Scope and budgets

Runtime owns an opt-in `OSVAULD_PERFORMANCE=1` native overlay and rolling samples. Shell receives
plain snapshots through an `App` hook and answers read-only `Performance` RPCs. The bridge and
Lua VM own no timer or rendering loop. Keep the existing opt-in per-thread CPU phase tracing
for locating work in app preparation, Lua view/layout, painting and surface submission.

FPS means completed normal surface presentations per wall-clock second, not simulation Hz,
offscreen throughput, GPU completion or display scanout. Work time includes CPU work and waits;
GPU timestamp measurements are outside this step. Offscreen FPS is null. Captures and tile
builds never add display FPS. Read-only diagnostics show stale/insufficient FPS as unavailable.

The initial product target is 60 FPS, a 16.67 ms frame interval, and 120 Hz physics (8.33 ms fixed
steps, at most eight catch-up steps). Report a steady release window, startup separately, and
flag shortfalls rather than silently relaxing the target. Report CPU/work percentiles separately
from cadence and do not claim a GPU budget has passed without measuring it. Expected course:
one 400 kg vehicle, four wheels, four rigid bodies (terrain, two probes, chassis), 6144 terrain
collision triangles, lunar gravity `{0,-1.62,0}`, finite state and zero catch-up drops in a steady
drive. The collision triangle cap remains 131072. Intentional wall-clock pauses, startup and
captures are not a steady-play benchmark.

Performance verdicts are recorded with hardware, build profile and viewport. Worker results
prove behavior and worker performance; laptop results are the basis for laptop FPS claims.

## Measurements — 2026-11-05

Release binary built on the NixOS worker and fetched through LoadTable (job 2526). Launch on
this laptop uses its compatible dynamic loader explicitly; the fetched ELF references a Nix
store interpreter absent here. No local build or artifact rewrite is needed. The worker's
Nix graphics/Xvfb wrappers run visual tests using llvmpipe; those results are not laptop FPS.

The controlled foreground laptop run (job 2540) used Intel Iris Xe / Vulkan, 1920×1080,
scale 1, held 10% throttle, three-second warmup and an eight-second sample (16 snapshots).
FPS is native presentation-call cadence, not measured physical scanout. The report flags a
shortfall below 57 FPS (5% measurement tolerance against the 60 FPS goal); it does not redefine
the product target as 57 FPS.

| measurement | result | expected |
|---|---:|---:|
| median recent presentation FPS | 63.37 | at least 57, target 60 |
| lowest sampled recent FPS | 58.76 | at least 57 |
| median of rolling p95 CPU+wait frame windows | 12.89 ms | at most 16.67 ms |
| physics tick rate | 120.17 Hz | 120 Hz ±5% |
| catch-up time dropped during sample | 0 s | 0 s |
| rigid bodies / vehicles / wheels | 4 / 1 / 4 | 4 / 1 / 4 |
| scene objects | 10 | at most 256 |
| instanced visual triangles | 11992 | at most 131072 |
| collision triangles | 6144 | at most 131072 |
| unique mesh payload | 371712 bytes | at most 16 MiB |

Finite state, lunar gravity, 400 kg chassis mass, speed, suspension travel and clean console
checks pass. The rolling maximum included an 83.29 ms outlier in the first 14 normal
frames; it leaves the history after the first two sampled snapshots. Once the history is
full, the largest recorded window maximum is 19.26 ms, and the final window maximum is
14.07 ms. The p95 gate passing does not mean every frame meets 16.67 ms. Raw report/capture: `shots/rover-laptop-performance-final.{json,png}` (generated,
not source-controlled). This short run establishes a baseline, not a long-duration soak result.
GPU timestamps/VRAM measurements remain unbuilt and `gpu_ms` is null.

A separate foreground stage trace (job 2539) measured p95 host frame 13.20 ms, of which surface
wait/encoding/submission was 13.02 ms. The app thread's p95 total was 1.43 ms, preparation
(including simulation) 0.21 ms, Lua view 0.81 ms, layout 0.30 ms and paint 0.13 ms. Threads run
asynchronously; do not add their percentiles as if they were a serial frame. These traced and
untraced runs are separate samples, not interchangeable percentile calculations.

An initial uncontrolled run held 60.0 FPS but its p95 work flag was 17.06 ms. A background trace
then showed ~1 FPS and ~1 s surface waits while app simulation continued at 120 Hz. Preserve
these findings: focus/visibility materially affects presentation on Sway. Controlled runs use
`--foreground`, targeting only the preview session's PID, and record actual viewport and adapter.
Do not infer a slow physics solver from a presentation wait or erase a failed sample by changing
its threshold. The native FPS overlay is recent presentation cadence; it does not keep idle apps
repainting.

Workspace check and 1127 workspace tests pass (23 ignored), along with the dedicated diagnostic
smoke and native timing tests. Full 42-script sweep passes (two optional Android checks skip without opt-in). User acceptance/landing is
still pending; all changes remain uncommitted in `moon-rover`.

## Component boundary revision — 2026-11-05

The first vehicle-specific 3D API is superseded by [physics components](physics-components.md).
The measurement script now inspects flat physical wheels and parent body mass, and its isolated
held-throttle fixture calls the actual Lua `vehicle.apply` function. Native inspection has no
`vehicles` field; nullable wheel motors and optional body mass declarations are valid state.

After refactoring, the Nix-built release (job 2575) measured on the same laptop in controlled
foreground (job 2577), Iris Xe / Vulkan, 1920×1080 at scale 1, three-second warmup and 16 snapshots
over 7.56 seconds: median recent presentation FPS 63.56, lowest sampled 59.49; median rolling-p95
CPU+wait work 8.88 ms; physics 120.18 Hz and dropped time 0 s. All 14 audit checks pass. Counts
remain 37 bodies, one wheel parent/four wheels, 53 scene objects, 8,336 collision triangles,
10,332 visual triangles and 194,688 unique mesh bytes. Raw report/capture are generated at
`shots/components-rover-performance.{json,png}`. This short run establishes no sustained-speedup
claim against earlier runs; GPU timestamps remain unavailable. Neither build nor tests ran on
the laptop; only the fetched release preview/measurement did.
