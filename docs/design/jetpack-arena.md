# Jetpack arena

Status: implementation started 2026-10-26. Android single-player game; no networking.

## End-to-end acceptance tests

Through a fresh shell, real bridge, uploaded Lua source and isolated vault:

1. Upload/open Jetpack Arena with a clean console. Start one match with two flying bots.
2. Hold independent movement and aim/fire touches. Both remain active; releasing or cancelling
   one cannot release the other. Native input and bridge touch injection share routing.
3. Thrust lifts the player, consumes fuel, and releasing lets gravity land them on platforms.
   Solid cover blocks both actors and shots. Actors cannot leave the arena.
4. Shots damage bots, kills increment score, and bots respawn. Bots move, aim imperfectly,
   respect cover and damage the player; player death leads to a timed respawn.
5. Pause freezes match time; resume/restart clear held input. Background/focus loss and geometry
   changes cancel touches. Repeated inspection and captures do not advance the match.
6. Controls remain reachable in mobile landscape and portrait viewports. Inspect screenshots.
7. Build the ARM64 APK. Exercise native simultaneous touch and background/resume on an emulator
   when available; report physical-device performance as unverified until measured.

## Decisions

**2026-10-26 physical Pixel gate:** the installed MSAA16 build shows horizontal corruption
on the Pixel 3a before opening the game. Compare the same screen with Android Area AA,
preserving app data, and inspect physical pixels before accepting a renderer change. Then
open the game and exercise native controls. Emulator success alone does not pass this gate.

**2026-10-26 Pixel result and resume refinement:** Area AA removes the horizontal corruption
on the physical Pixel 3a (also confirmed by the user). The game uploads and renders there.
Wake still stalls while rebuilding GPU pipelines. Release the native surface synchronously on
suspend, park the remaining renderer, and reattach a fresh surface on resume. Verify repeated
suspend does not discard the parked renderer, retained input stays cancelled, and a real Pixel
sleep/wake returns game pixels promptly. Compile only Area AA pipelines on Android.

**2026-10-26 fullscreen:** the user has played the game on Pixel and requested fullscreen.
Provide a shell-owned Full screen action for app tabs, with an always-reachable Exit control.
Hide the shell strip and Android system bars while active; preserve cutout/IME safe insets.
Home, closing the app and account transitions leave fullscreen. A bridge-driven smoke must
enter and exit through real pointer hits, check larger tile bounds, and restore the tab strip.
Verify immersive bars and game controls in physical-device pixels after the APK update.

**2026-10-26 cold opening:** Pixel startup still spends roughly six seconds before first
content insets despite fast retained-process wake. Persist Vello's Vulkan pipeline cache in
Android app-private storage, keyed by wgpu's adapter key and cache schema. Only cache bytes
produced by wgpu are loaded; unsupported devices and I/O failures keep the normal startup path.
Verify GPU cache save/reload, real pixels, and consecutive cold-process startup timings on Pixel.

**2026-10-26 play feedback:** the user requests gentler thrust and a map larger than the view.
Expand the yard horizontally to three screens, keep the original first yard and collision
landmarks, and follow the player with a bounded camera. Screen-to-world aim must include the
camera offset; HUD and sticks stay fixed. Add generic child clipping to the element vocabulary
so off-camera art cannot paint or hit outside the playfield. Test gentler lift, camera bounds and
aim mapping, then drive travel across the level and back through real shell controls. Upload the
Lua revision to the existing Pixel item with versioned exact edits after validation.

The user approved the single-player flying-bot scope and autonomous long-horizon implementation.
One arena, one gun, health, fuel, score, death/respawn and pause/restart are the first playable.
Art is original vector geometry assembled through gfx; no copied game assets.

Add a generic captured `on_touch` surface to the existing runtime vocabulary. Fingers have opaque
string IDs in Lua (Android IDs are u64, beyond exact Lua-number range), local coordinates and
start/move/end/cancel phases. Each finger captures its initial target. Tiles forward plain-data
events. Existing UI without a touch handler retains single-finger pointer behavior. Cancellation
never commits a click. The bridge drives the same path using logical coordinates.

Game rules and bot decisions remain Lua. Begin with a bounded fixed-step Lua simulation through
the existing authored-simulation on_frame exception, using swept collision against a small static
arena. No per-tick document writes. Frame elapsed time, rather than dt or wall time, gates updates
so inspection/capture at the same clock is observational. Stall debt is bounded and pause drops it.
This avoids adding shooter-specific primitives to Rust. Reassess native world integration if
measured mobile cost or collision complexity demands it.

The touch prop follows the existing registry's present-key lookup; it adds no unconditional
Lua table read to each walked element. Native Android validation also needs live read-only
`Rects`; clock-moving and synthetic input drivers remain offscreen-only.
Live app geometry is sampled on the app's owning thread under one three-second collection
deadline. Only one native sample may be pending; account generation, app incarnation and tile
size are checked before rendering the observation and again before replying. Native frames keep
the requested metadata without rescanning ordinary paints. Native observations sample the last
painted app geometry while real time continues; they do not freeze a moving match.

**2026-10-26 refinement:** a size alone cannot distinguish resize-away-and-back, and a queued
resize is not yet painted geometry. Scopes therefore include a monotonic resize revision. The
app thread returns the scope of the frame actually painted, recorded only after `tile.frame`;
collection rejects a requested layout that has not yet been painted. Tests pin both orderings.

Heavy verification runs explicitly on `nixos` at the user's request. `scripts/with_xvfb.py`
uses Xvfb's displayfd on a worker without a desktop; Android packaging tools/SDK additions live
under that checkout's task-managed cache. The optional Mac job was held by its battery policy.

## Steps

1. Test-first multi-touch routing, Lua binding and bridge injection; preserve ordinary UI tests.
2. Test-first game acceptance, then Lua simulation, controls and original vector presentation.
3. Run focused and workspace tests, full smokes, APK build and available Android checks.
4. Fresh `pi -p` review against expert checklists; repair findings and record actual validation.

Nothing is committed or merged automatically. Review artifacts remain in the working tree for
the user's judgment. Physical-device FPS, audio and multiplayer are outside the proven scope.
