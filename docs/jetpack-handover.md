# Jetpack Arena handover — 2026-10-26

## Resume here

Worktree: `/home/abe/osvauld2/.claude/worktrees/jetpack-arena`.
Branch: **`jetpack-arena`**. All changes are uncommitted; nothing merged into `dev`.
Plan: `docs/design/jetpack-arena.md`. Current progress: `docs/status.md`.

The user has played the single-player flying-bot game on the physical Pixel and confirmed it
works. Fullscreen was then implemented and verified on that Pixel. Their latest gameplay
feedback was: thrust is too fast, and the map should extend beyond the screen so the player
can travel around it. Those changes are implemented and pass tests, **but the updated Lua
source has NOT yet been uploaded to the Pixel**.

The final compatible Android APK **is installed**. It includes viewport clipping, Area AA,
fullscreen, retained GPU pipelines during suspend, and persistent Vulkan pipeline caching.
The Pixel's existing game item still contains the earlier single-screen/faster-thrust Lua.
Next: unlock the account on the phone, update that existing item's source with versioned exact
edits, reopen it, enter fullscreen, and confirm slower thrust and scrolling travel physically.
An async request asking the user to unlock after the final update is pending. Do not ask for
their passphrase; they enter it on the phone. The last bridge probe timed out while inactive.

**No jobs are running at handover.** All final build/test jobs finished successfully. The user
requested this document urgently because their session limit is near.

## User preferences and working rules

- Continue autonomously; avoid repeated permission questions. No commit/merge without their nod.
- Heavy builds/tests run on **nixos**, through `~/.local/bin/tasks2`, explicitly `--peer nixos`.
  Mac is allowed but was held by battery policy; do not run heavy jobs on the laptop.
- Read `AGENTS.md`, architecture/status/Lua guide/conventions and matching expert skills.
  Tests first; plan in `docs/design/`; real bridge/shell/Lua validation matters.
- Never bare cargo builds/tests, SSH, old `tasks`, custom build locks or shared target overrides.
  `tasks2` manages the checkout cache and target symlink. Use at most `-j 6`.
- This environment needs sandbox escalation for task-manager/ADB socket access. Existing user
  authorization covers builds, install updates preserving data, launch, debugging and source upload.
- Existing uploaded source: `read_file_versioned` then `edit_file` with exact replacements.
  `WriteFile` is for new files. Do not clear/uninstall the Pixel app or overwrite unrelated data.
- A direct external `pi` source-export review was rejected by automatic approval review. Do not
  bypass that via another tool. Fresh internal reviewer `/root/final_internal_review` reviewed
  the changes read-only instead; final reviews found no BLOCKER/SHOULD issues.

Main checkout `/home/abe/osvauld2` is `dev` and has unrelated user changes (AGENTS.md,
scripts/demo_reel.py, scripts/osvauld/record.py, demo_apps/swag_shop/, upload.png). Leave it alone.

## What changed

### Lua game

`demo_apps/jetpack/` contains `main.lua`, `model.lua`, `art.lua`, `theme.lua`, `camera.lua`,
and `manifest.osv`. Original vector art, named Skyward / Jetpack Arena. One gun, unlimited
ammo, two imperfect flying bots, cover/platform collision, swept bullets, health, fuel,
kills/deaths, respawn/shields, pause/resume/restart. Match state is ephemeral, never doc writes.

Lua owns a bounded 120 Hz fixed-step simulation, gated by elapsed time. Same-clock screenshots
and inspections do not advance it; stalls are bounded. Native multi-touch: left stick moves/up
flies, right stick aims/fires, JET thrusts. Desktop A/D, Space, mouse + F also work.

Latest revision, not yet deployed as source:

- Thrust 1400 → 1120 against gravity 720: roughly 40% less net lift acceleration.
- Maximum ascent speed 280 → 190 world units/second.
- Map width 960 → 2880; height remains 480. Additional platforms/cover in three yards.
- A bounded horizontal camera follows the player with a 960 × 480 world view. HUD/sticks stay
  fixed. Screen-to-world aiming includes viewport scale/offset and camera offset, refreshed
  as the camera moves. The first yard's original collision landmarks are preserved.
- Scenery, actors, effects and yard signs are inside a clipped viewport. No Rust shooter engine
  or shooter-specific native primitives were introduced.

### Generic Rust substrate

| Files | Purpose |
| --- | --- |
| `runtime/src/{lib.rs,el.rs,touch_tests.rs}`, tile modules | Independent captured fingers, up to 16, fresh handlers by stable ID, frozen initial local coordinates. Ordered `TouchHit` surfaces/tiles/ordinary controls preserve reverse paint hit order. Ordinary buttons work while gameplay fingers are held. Native resize/insets/focus/suspend cancel input. |
| `app_host/src/{lib.rs,props.rs,tests.rs}` | Strict `on_touch` Lua binding; opaque string finger IDs preserve u64 precision. Frame callbacks expose element width/height. Existing CallFrame sites updated. |
| `runtime/src/{el.rs,layout.rs,tests.rs}`, props registry | Generic `.clip()` / Lua `clip=true` clips child painting and hit regions without scrolling. Regression checks outside hits are excluded. |
| `osvauld-rpc`, `scripts/osvauld/client.py` | Offscreen bridge Touch injection, existing rect inspection extended to native windows. |
| `shell2/src/native_rects*`, app_thread modules | Live rect sampling on owning app threads, one outstanding request, total deadline, account/app/layout guards. Scope contains app instance, monotonic resize revision and size; snapshot records scope only after paint. Rejects resize-before-paint and resize-away-and-back. No metadata scan on every ordinary native frame. |
| `runtime/src/render.rs` | Android Area AA fixes horizontal corruption on Pixel 3a; desktop retains MSAA16. Android compiles only Area AA variants. |
| Runner + Render | Suspend drops native Surface/window references synchronously but parks the remaining GPU renderer. Resume attaches a fresh surface and reuses compiled pipelines. Repeated suspend preserves the parked renderer. Input still cancels. |
| `runtime/src/render/pipeline_cache*` | Optional private Vulkan Vello cache keyed by adapter and schema/version, atomic save, stale-driver fallback. Stored under Android internal data `gpu-cache/`, not workspace documents. Unsupported features/I/O failures retain uncached startup. |
| Shell, App trait, Android InsetsAdapter Rust/Java | Full screen action hides shell tabs and Android system bars; visible Exit control restores them. Closing the focused fullscreen app clears fullscreen, including when another app remains open. Insets still protect cutouts/IME. |

Important: `Tile::resize` deliberately does not cancel touches; custom captures temporarily
resize tiles. Actual native window geometry changes cancel input at the host boundary.

### Supporting scripts and docs

- `scripts/test_jetpack_model.lua`: pure mechanics, gentler lift, camera bounds/aim mapping.
- `scripts/smoke_jetpack.py`: real shell, isolated vault, uploaded Lua, two layouts, cover pixels,
  independent fingers, gameplay/death/respawn, pause, observational captures, fullscreen enter/exit,
  fullscreen close-with-another-tab, and travel into yard 2 while yard 1 leaves view.
- `scripts/smoke_jetpack_android.py`: opt-in disposable **root emulator only**, native three-finger
  input, held-control pause, background/resume. Do not run it on the user's physical Pixel.
- Both smokes registered in `scripts/smoke.py` (32 entries; Android opt-ins skip by default).
- `scripts/with_xvfb.py`: worker display via Xvfb displayfd, temporary runtime dir.
- `scripts/with_nix_graphics.py`: ephemeral Nix graphics dependencies pinned to worker's own
  nixpkgs revision, Mesa/lavapipe. Registry-latest Mesa needed incompatible glibc; do not undo pin.
- `docs/lua-apps.md`: touch, frame dimensions, native rect sampling and clip contracts updated.

## Verification already completed

All below ran on NixOS except the explicitly physical-device checks and earlier emulator run.

| Job/check | Result |
| --- | --- |
| 1009 `cargo test --workspace -j 6` | Passed, all 16 reported test summaries successful, after clip/map changes. |
| 1013 `cargo check --workspace -j 6` | Passed, after final changes. Existing warnings remain. |
| 1012 full `python3 scripts/smoke.py` with Nix graphics/Xvfb | **All 32 passed**, after final changes; Android opt-ins skip normally. |
| 1008 focused game smoke | Both 960×540 and 393×780 passed, including travel/camera and fullscreen. |
| 1006 pure Lua model/camera | Passed through `nix shell nixpkgs#lua --command lua ...`. Worker has no bare `lua`; job1003 failed for that environment reason. |
| 999 runtime including ignored GPU checks | 196 passed, including real pixels and cache round-trip, before the later clip addition. Final clip regression is covered by workspace1009. |
| 1010 ARM64 debug APK | Passed, fetched and installed on Pixel; includes final Rust changes. |
| 939 native Android emulator | Passed three-finger flight/fire/thrust, held-control pause and background/resume, before later rendering/camera changes. Emulator stopped. |
| Pixel original game | User confirmed game works. Area AA fixes corruption. Physical landscape fullscreen screenshot verified with no system bars/tabs and reachable Exit. |
| Pixel retained-process wake | One sleep/wake bridge response 0.26 s, same process. This is bridge readiness, not a measured frame-time/FPS guarantee. |
| Pixel consecutive cold launches with cache | First bridge readiness7.26 s, render init6.280 s, cache absent; second cold process1.31 s, render init0.664 s, cache bytes supplied. First-ever compilation still costs time. |

Fresh internal expert reviews found no remaining BLOCKER/SHOULD issues. Advisory note: a smoke
that explicitly verifies aimed hits while crossing a camera-follow boundary could strengthen
combined coverage; camera mapping and combat are currently tested separately.

## Pixel / upload details

ADB: `/home/abe/Android/Sdk/platform-tools/adb -s 9BWAY1LSDC`.
Physical Pixel 3a, Android12, non-root, old Adreno615 Vulkan driver dated2020-12-01.
Package `com.osvauld.android`; activity `com.osvauld.android/android.app.NativeActivity`.
Debug UDS: `/data/user/0/com.osvauld.android/files/shell.sock` (app-private).
Existing workspace and item are both named **Jetpack Arena**. Preserve them and the account.

Rootless USB bridge transport works:

```sh
adb -s 9BWAY1LSDC shell -T run-as com.osvauld.android \
  /system/bin/nc -U /data/user/0/com.osvauld.android/files/shell.sock
```

Use **`shell -T`**, not `exec-out`, for bidirectional request transport. Keep stdin open while
reading a response. Framing: four-byte big-endian JSON length, JSON request `{op: ..., ...}`;
same response framing with `{status: "ok", result: ...}` or `{status: "err", message: ...}`.
Use a bounded read deadline. The event loop must be awake for requests to complete.

Temporary helpers already prepared, not repository contracts and may disappear:

- `/tmp/pixel_bridge.py`: `PixelBridge(Bridge)` using that ADB transport, deadline-bounded reads.
- `/tmp/pixel-jetpack-item.json`: existing workspace/item IDs, no passphrase.
- `/tmp/update_pixel_jetpack.py`: ready to run **after unlock**. Selects existing workspace,
  closes the existing game tab, creates new `camera.lua`, reads each existing source version,
  computes uniquely anchored exact replacements, applies them with `edit_file`, verifies content,
  reopens and checks Lua console. Run through local Python; no build is involved.

If helpers are gone, reconstruct this flow with `scripts/osvauld/client.py`; do not overwrite
existing files via WriteFile. Closing before edits avoids partially updated module combinations
running between edits. After source upload, find `shell:fullscreen` via live `rects`, convert its
logical centre to physical pixels using current density (Pixel was2.75), then native ADB tap.
Retry rect collection on transient layout-change errors. Never guess tap coordinates.

Final install has restarted/locked the account; user must unlock on-phone before source changes.
Do not save/export recovery material. Use only the game screenshots below when sharing artifacts.

## Artifacts

All paths are under this worktree unless marked `/tmp`. `shots/` is gitignored.

- `shots/jetpack-host-arm64-debug.apk`: final1010 host APK,137114096 bytes,
  SHA256 `5b7a467c3b78f8283fbaa5b9a643294ae17fe35cf99d3bfb417a9b0200d2625f`.
  Same fetched file remains at `target/debug/apk/osvauld.apk`. It is the Osvauld host APK;
  game source is uploaded separately, not bundled as a standalone game application.
- `shots/jetpack-source.zip`: refreshed with latest camera/slower-thrust source,8817 bytes,
  SHA256 `1320b65e54a754bea98cf18823fac3eaa0136479c5b760d7a98b257a585e6884`.
- `shots/jetpack-camera-landscape.png`: real-shell tested view after travel into yard2.
- `/tmp/pixel-jetpack-fullscreen.png`: physical Pixel landscape fullscreen, earlier Lua revision.
- `/tmp/pixel-jetpack-loaded.png`: physical Pixel original game portrait.
- Other earlier x86 APK/screenshots exist; do not confuse them with final ARM64 artifact.

## Reproducing builds/tests

From this worktree root, through tasks2, never bare cargo:

```sh
tasks2 run --who YOUR_AGENT --peer nixos -- cargo check --workspace -j 6
tasks2 run --who YOUR_AGENT --peer nixos -- cargo test --workspace -j 6
tasks2 run --who YOUR_AGENT --peer nixos -- \
  python3 scripts/with_nix_graphics.py python3 scripts/with_xvfb.py python3 scripts/smoke.py
tasks2 run --who YOUR_AGENT --peer nixos -- \
  nix shell nixpkgs#lua --command lua scripts/test_jetpack_model.lua
```

Android worker setup is already in checkout-managed cache: cargo-apk under
`target/jetpack-tools/bin`, SDK35 under `target/jetpack-sdk`, NDK
`/home/abe/Android/Sdk/ndk/28.2.13676358`, worker JAVA_HOME `/etc/taskworker/jdk21`.
Cached SDK d8/apksigner shebangs were patched to the worker's real bash. ARM Rust target installed.
Build via tasks2 `--peer nixos --fetch target/debug/apk/osvauld.apk --detach -- python3 -c ...`:
set PATH to resolved `target/jetpack-tools/bin` plus existing PATH, ANDROID_HOME to resolved
`target/jetpack-sdk`, ANDROID_NDK_ROOT to the above, CARGO_BUILD_JOBS=6; invoke
`python3 scripts/build_android.py` with that environment. No machine-wide SDK/profile changes.

## Asset-editor direction discussed with the user

The user asked what Rust was needed and whether desktop asset editing can produce game assets.
The answer is yes: Rust supplies generic input/render/lifecycle/inspection/clip capabilities;
gameplay, bots, weapon behavior, art and this camera are Lua. No Rust feature per gun is needed.

`demo_apps/asset_editor/` already edits vector parts, pivots, animation clips and attachment
sockets. It stores named **workspace assets** through `asset:open(...)`. The game currently
uses direct `gfx.frame` vector art in `art.lua`, including one reusable gun visual; it does
**not yet load editor-authored workspace assets**.

Next asset work, after the pending phone update, should connect a shared gun/character asset
to the game. Author/preview on desktop; define grip and muzzle attachment points; load the saved
drawing in the game. Keep damage, rate of fire, projectile speed and ammo rules in Lua weapon
definitions, separate from appearance. Likely assets: gun(s), character, bullet/tracer, muzzle
flash, impacts, jetpack effects, platforms/props.

Important existing limitation from `docs/lua-apps.md` Assets: assets save/load locally, but live
cross-app asset refresh and desktop-to-phone asset sync are not built. Do not promise that a
desktop edit automatically appears on Pixel; game asset loading and delivery need explicit work.
This was a discussion of direction, not a completed asset integration.

Still outside delivered scope: audio, multiplayer/iroh game traffic, measured physical FPS,
release/store packaging. Do not mark the latest map/thrust phone validation complete until the
Lua source is uploaded and exercised. Do not commit/merge without the user's judgment.
