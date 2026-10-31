# iOS — shell2 on the simulator, then the device

Status: **plan, 2026-10-31. Nothing built.** Mirrors the Android port (`docs/status.md`,
`android_app/`). Prior art: Tinline (`~/p2p_chat/ios/`, the loadtable `mac` peer and its
`ios-sim` suite), which builds and runs there.

## 0. End-to-end tests (written first, run on the Mac peer)

All run with `loadtable run --who NAME --peer mac -- <script>` from the repo root; results come
back with `--fetch`. The Mac starts jobs only on AC power unless its owner allows battery.

| id | proves | how |
|---|---|---|
| I1 | shell2 compiles for `aarch64-apple-ios-sim` | `scripts/ios/run_sim.sh` step 1 |
| I2 | it boots in the simulator and draws real text | same script: install, launch, after 10 s the process is alive, no crash report, and `ios/build/sim/launch.png` is the auth screen (checked by eye once, then by `scripts/ios/check_launch.py`: not one flat colour, top and bottom of the screen differ) |
| I3 | the bridge answers from inside the simulator | the app is launched with `SIMCTL_CHILD_OSVAULD_SOCKET` / `SIMCTL_CHILD_OSVAULD_DATA_DIR` pointing at host paths; `scripts/ios/smoke_bridge_sim.py` (Python on the Mac) connects, `Whoami`, `rects()`, signs up through the bridge and sees the launcher |
| I4 | Jetpack comes inside the app | `scripts/smoke_bundled_apps.py` (Linux, in `smoke.py`): with `OSVAULD_BUNDLED_APPS` set, signup makes one "Apps" space holding Jetpack, which opens and draws; unlock adds nothing; without it, nothing. On iOS the folder is `apps/` in the `.app` (`run_sim.sh` copies `demo_apps/jetpack`). |

A simulator app is a process on the Mac's own kernel, so a host path is reachable from both sides.
I3 is how every later iOS test gets driven: through the bridge, as offscreen tests are, never by
guessing a tap coordinate.

## 1. Shape

No Xcode project and no Swift. winit 0.30 runs `UIApplicationMain` itself from a plain Rust
`fn main`, so the `shell2` binary is the app: cross-compiled, wrapped in a `.app` with an
`Info.plist`, ad-hoc signed for the simulator. Same `run_desktop()` entry; iOS-only differences
stay behind `cfg(target_os = "ios")`.

- Data directory: `dirs::data_dir()` already resolves inside the app container (`$HOME` is the
  container on iOS). `OSVAULD_DATA_DIR` still overrides, for tests.
- `rfd` (folder picker) is off on iOS, as on Android.
- **UIScene (found 2026-10-31).** The iOS 26 SDK refuses to launch an app that doesn't adopt
  the UIScene lifecycle; winit 0.30 doesn't (upstream PR #4723, open). Stopgap: `vtool` stamps
  the binary as built against SDK 18. Fine for the simulator and dev devices; an App Store build
  needs real UIScene support, either upstream winit or our own patch.
- **The simulator cannot draw (found 2026-10-31).** Simulator Metal exposes only the Apple2 GPU
  family: no indirect dispatch (`wgpu-hal` metal/adapter.rs), which vello needs on every buffer,
  even with `use_cpu`. A patched vello copy was tried and dropped the same day (decided with the
  user: test on real iPhones, not vendor vello). So the simulator proves I1 and launch only;
  I2/I3 run on a device (`scripts/ios/run_device.sh`, the developer's own Apple account). Also
  found: the simulator falls one short on `max_inter_stage_shader_variables` (15); devices now
  request `adapter.limits()`.
- `libc` stays below 0.2.190 (Tinline: `netdev` breaks on iOS past it). Lockfile has 0.2.186.

## 2. Steps

1. **Compile (I1).** `scripts/ios/run_sim.sh` adds the Rust target, builds `shell2` without
   default features. Fix what breaks, behind `cfg`.
2. **Bundle and launch (I2).** Same script: `.app` + `Info.plist`, `codesign -s -`,
   `simctl install/launch`, screenshot, log, crash report into `ios/build/sim/`.
3. **Bridge (I3).** Socket and data dir from the host; Python smoke on the Mac.
4. **Later, each its own step:** safe-area insets, on-screen keyboard and text input, touch,
   suspend/resume (Android's list), then a device build with signing.
