# Demo recorder — a Lua pointer, real zoom, frame-exact video

Status: **built 2026-10-03, on branch `worktree-demo-recorder`, not merged.** Long-horizon, test-first: §0 lists the
end-to-end tests that prove it done; §4's steps make them pass.

## 0. Done means these pass

**Rust (`cargo test`)**

| # | test | proves | step |
|---|---|---|---|
| T1 | `runtime::hover`: a press, then a release, under a still pointer each report one `Move`; holding still while down reports none | the button state is hover news | 1 ✅ |
| T2 | `app_host`: `on_hover` sees `e.down` false, then true | it reaches Lua | 1 ✅ |
| T3 | `runtime`: `Headless` press/release under a parked pointer fire `on_hover` with `down` | the real press path, not only the diff | 1 ✅ |
| T4 | `runtime`: `DriverOp::Wheel` with `ctrl` over a `zoomable` scales its child's rect by `1.1^(dy/30)` around the pointer; without `ctrl` it pans | scripted zoom is the user's zoom | 2 ✅ |
| T5 | `runtime`: offscreen, an ambient message (`App::is_ambient`) paints but leaves the clock; a request moves it one frame | wall-clock wakes cannot shift a recording | 4 ✅ |
| T6 | `runtime`: over an element with `hide_system_cursor`, `cursor_icon` is `None`; elsewhere the usual icon | the OS pointer hides where the app draws its own | 6 ✅ |
| T7 | `runtime`: hover reports the topmost *declared* look (a plain child doesn't hide its parent's); a look change under a still pointer is a `Move` | the target chooses the look | 7 ✅ |
| T8 | `app_host`: `cursor` takes a name or a `gfx.frame`, else a type error; `e.look` reaches Lua; a frame reads back `width`/`height` | the Lua surface | 7 ✅ |

**Smoke (`scripts/smoke_demo_record.py`, real shell over the bridge, in `SMOKES`)**

S1. Upload `demo_apps/pointer`. Park the pointer on a button: the cursor's pixels sit at the
    pointer tip. Press: the tip pixel turns the pressed colour. Release: the button's counter went
    up — the cursor never takes a click.
S2. Ctrl+wheel over the demo's `zoomable` board through `rpc.wheel`: a card's rect grows by the
    expected factor; the cursor stays the same size (it lives outside the board).
S3. Record a script (glide → click → zoom in → hold → zoom out) to mp4. `ffprobe` frame count is
    exactly `fps × duration`; a decoded frame shows the cursor at the scripted point; the app's
    state after recording shows the click landed.
S4. Record the same script twice: per-frame hashes (`ffmpeg -f framemd5`) are identical. The
    virtual clock makes a demo a pure function of its script.
S5. Over a card the cursor is `cursor:grab`, over bare board the board's own drawing
    (`cursor:visual`), over a button the arrow — read from `DumpTree`.

All of S1–S5 pass (`python3 scripts/smoke.py smoke_demo_record.py`).

## 1. What

Scripted demo videos of the real app. Three parts, each the platform's own:

- **The pointer is a Lua component** (`cursor.lua`), drawn in the scene like anything else. An
  agent can restyle it, the same drawing later renders peers' cursors (presence), and the shell
  draws nothing special for recordings. Today an app mounts it; once the workspace is a Lua app,
  the workspace mounts it once for every app.
- **Zoom is the app's real zoom** — a `zoomable` driven by Ctrl+wheel, so the video shows exactly
  what a person zooming sees. Vector re-render, never an upscaled crop.
- **Video is screenshots on a virtual clock**, piped to `ffmpeg`. Offscreen time moves only when
  asked, so frame k is the app at `t0 + k/fps` exactly — no dropped frames, no races.

## 2. The pointer in Lua

The runtime already tracks press and release; step 1 forwards them. `on_hover`'s event gains
`e.down`, and a press or release under a still pointer fires a `"move"`. No new handler, nothing
consumed: the cursor observes, it never intercepts.

```lua
local cursor = require("cursor")
return function()
    return ui.col({
        id = "root", grow = true,
        on_hover = cursor.track,     -- or call cursor.track(e) from your own handler
        ...app...,
        cursor.view(),               -- last child: painted on top
    })
end
```

`cursor.view()` is an `absolute` `ui.frame` of unnamed `gfx` shapes at the tracked point —
unnamed shapes are paint, so the pointer falls through to the app. Down: the arrow darkens and a
ring sits under the tip. Release: the ring fades out over 300 ms via `on_frame`, present only
while the ring lives (presence keeps repainting). `require` resolves inside the app's own source,
so the module is copied into an app, not shared by path; `demo_apps/pointer/cursor.lua` is the
canonical copy.

Verified by probe (2026-10-03): root `on_hover` coordinates land the `absolute` arrow exactly on
the pointer; a click under the arrow reaches the button.

### Who draws, who decides

The thing pointed at decides the look; the cursor draws it. An element declares
`cursor = "grab"` (a name the cursor knows) or `cursor = <gfx.frame>` (its own drawing, centred
on the pointer — frames read back `width`/`height` for that). The runtime collects declared looks
in paint order and puts the topmost one under the pointer on every hover event as `e.look`; a
plain element on top does not hide its parent's look. A prop, not a call, so it crosses the
workspace/app boundary once the workspace is a Lua app: neither side calls the other.
`system_cursor = false` hides the OS pointer over the element that draws its own.

**Lag, windowed.** The Lua cursor is drawn by the app's next frame; the OS pointer is drawn by the
compositor ahead of any app, so every app-drawn cursor trails it. Hiding the OS pointer removes the
comparison. Not done yet: present latency 1 instead of 2 (`render.rs`), and a `follow_pointer`
prop that places the element at paint time from the newest pointer position. Recordings have no
lag — the virtual clock waits for Lua.

## 3. Wheel over the bridge, and the recorder

**`Wheel { x, y, dx, dy, ctrl }`** — a driver op like `Drag`: move to `(x, y)`, then the window's
own wheel path with control held for the call. `dy > 0` zooms in by `1.1^(dy/30)` around the
pointer, clamped to the zoomable's 0.4–3.0. Fractional deltas are fine, so a zoom spread over
frames is smooth.

**`scripts/osvauld/record.py`** — a `Recorder(session, item, path, fps=30)`; every verb renders
the frames it spans:

| verb | frames |
|---|---|
| `glide_to(target, secs)` | eased pointer path; `target` is an element id (its centre via `rects`) or a point |
| `click(hold=0.12)` | press, hold, release — the ring shows |
| `drag_to(target, secs)` | press, glide, release |
| `zoom(factor, secs, at=None)` | eased Ctrl+wheel deltas in log space around the pointer or `at` |
| `hold(secs)` | nothing scripted; the app's own springs and fades are what gets recorded |

Each frame: apply this frame's pointer/wheel step, bring the clock to `t0 + (k+1)/fps` with
`Advance`, screenshot (default target — never a custom viewport, which clears hit regions), write
the PNG to `ffmpeg -f image2pipe`. Output mp4 (H.264, yuv420p); `gif=True` adds a palette-pass GIF
from the same frames.

**The frame budget** (measured 2026-10-03). Every request that reaches the shell costs clock: a
screenshot 1/60s (one delivered message, one frame), a pointer op 1/60s + 8ms, `Wheel` the same
as a move (it paints once). What a frame does before its screenshot must fit in
`1/fps − 1/60`, so the default is **24fps with one pointer op per frame**; a frame that overruns
raises instead of silently shifting video time.

**Ambient wakes.** Offscreen, every delivered message used to tick — including the shell's 20s
`SyncTick` and `DocChanged` from a doc subscriber, which arrive on wall-clock time. A long take
picked one up mid-recording and overran by exactly a frame (seen in S3). `App::is_ambient` marks
those: offscreen they paint, so the app still notices, but do not move the clock. Only requests
move time.

## 4. Steps

1. `e.down` through runtime → app_host, documented in the guide. T1–T3. ✅
2. `DriverOp::Wheel` + shell2 request + `rpc.wheel`. T4, S2 (with step 3's app). ✅
3. `demo_apps/pointer`: `cursor.lua` + a small showcase (`main.lua`: buttons and a zoomable
   board). S1. ✅
4. `App::is_ambient` (T5), then `record.py`. S3, S4. ✅
5. `scripts/demo_pointer_video.py` — the showcase demo, the first real use. `docs/status.md`. ✅
6. `system_cursor = false` (T6). ✅
7. `cursor` looks: runtime `CursorLook`, `e.look`, frame size readback, cursor.lua looks
   (arrow, grab — closed while down, text, crosshair, supplied visual). T7, T8, S5. ✅

## 5. Not now

- A pressed look during the drag *ghost* — kanban's ghost is its own overlay; the cursor shows
  down regardless, which is enough.
- Panning the zoomable to centre a target (plain wheel pans; add a `pan` verb when a demo needs it).
- Audio, captions, typed-text overlays.
