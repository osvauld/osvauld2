# Maker parity — the asset editor as a GameMaker-class sprite and room editor

Status: **plan, 2026-10-04. Step 1 built 2026-10-05 (E1 green).** A test-first plan: §0 lists the end-to-end tests that
prove it done; they are written first, then §3's steps make them pass. Grows out of
[Assets](assets.md) §4 (editor tools in the order they pay off) and the world session's
directives of 2026-10-03: stage colliders, an editor smoke, the clip timeline, then tilemaps.

Companions:
- [Assets](assets.md) — the asset format, the editor so far, sockets and plugs, vars.
- [2D world](2d-world.md) — colliders, sensors, loose bodies, walkers; the world session owns it.
- [App threads](app-threads.md) — each app on its own thread; the fixed-step world rides there.

## 0. Done means these pass

Written before the code they test. Each names the step (§3) that turns it green.

**End-to-end (`scripts/smoke_asset_editor.py`, offscreen shell, real socket, real Lua VM).**
Positions come from the dump with a tolerance — the world steps at 1/120 s and sums in f32.

| # | scenario | proves | step |
|---|---|---|---|
| E1 | Open the editor, play the `room` stage. Drive the hero right into the chest: it stops short of it. Drive it into a loose crate placed in the stage: the crate's `pos` in the dump moves | a room made in the editor is a playable map | 1 |
| E2 | Open the `toggle` test app. Click the toggle: within the clip's length the dump's frame shows the `to_night` pose, then rests on `night`; a second click goes back to `day`. No `ui.world` in the tree | a UI asset is a widget in any app | 2 |
| E3 | Open the editor, select `arm_far`, drag a dot by its rect; read the asset back over the bridge: the point moved by the snapped step. Drag `head`: its socket moved too | the editor's gestures, over the real path | 3 |
| E4 | In the timeline, add a key on `head.rot` at 0.25 s, scrub to 0.25 s: the canvas shows the turned head. Play: the dump's clock passes the key. Reopen the asset: the key is there | clips are editable, not only playable | 4 |

**App tests (`cargo test -p app_host`, headless):** each step lands its own beside the existing
`asset_editor_*` ones — field validation, defaults and overrides, `clip:at` against
`Clip::sample`, timeline writes.

**Unchanged:** every script in `scripts/smoke.py`'s `SMOKES` passes after every step.

## 1. Where we stand against GameMaker

GameMaker splits making a game into resource editors. Ours are one editor over shared assets, and
the agent writes first; the user adjusts. The difference that stays: GameMaker draws **pixel
frames**, we draw **vector parts with clips** — which the agent writes far better than pixels.

| GameMaker | does | ours | gap |
|---|---|---|---|
| Sprite | frames, origin, collision mask, speed, nine-slice | drawing: parts, pivots, clips; select, drag points and parts, inspector, vars | mask lives on the stage, not the asset (step 1); resizing (open, §4) |
| Image editor | pencil, fill, shapes, layers, onion skin | point and part drags, snap | create shapes, pen, add/reparent/reorder parts (assets.md §4) |
| Sequence | tracks, keyframes, curves, scrub, broadcast messages | clips as data, played in place | the timeline (step 4); events at a time (assets.md §5b) |
| Object | sprite, mask, parent, physics, events with code | world fields: `collider`, `sensor`, `loose`, controllers; handlers in Lua | the stage doesn't carry them (step 1) |
| Room | layers: instances, tiles, backgrounds; grid; views | stage: place, drag, equip; play | tiles (step 6), layers, camera (world session) |
| Tile set | a grid of a sprite, brushes, autotile | — | step 6 |

The world already has the physics GameMaker's objects declare — a mask is our `collider`, a
collision event our sensor's `on_zone` or `on_hit`. What's missing is the editor using it.

## 2. Decisions

- **Behaviour lives on the asset, the stage overrides.** A chest is always solid, so its
  `collider` / `sensor` / `loose` are the asset's defaults (a `world` root beside `parts` and
  `clips`); a stage entity may override a field. Every room and game that places the asset gets
  them without asking. Kept as data; Rust names no content.
- **UI assets play clips without a world.** `clip:at(t)` returns each part's pose at `t`, from the
  world crate's `Clip::sample`; `drawing:pose(clip:at(t))` on a plain `ui.frame` animates, and
  `on_frame` runs only while it plays. A world per widget would bring a physics world, entity-only
  clicks and a fixed pixel size along. The world session agreed (2026-10-04): read-only use,
  `gfx.rs` only.
- **The editor becomes a kit.** Canvas, handles, drags, snap, inspector rows and later the
  timeline are modules any editor uses; the general editor is the kit's default layout and a
  bespoke one ("hero editor" with an arm-length slider) is a layout plus a few handlers.

## 3. Steps

Each is talked through before it's written; its test comes first and every step leaves
`cargo test` and the smokes green.

1. **World fields on assets and stages.** The asset's `world` root; a stage entity's own fields
   override it; play builds `ui.world` entities with them. The stage draws each collider and
   sensor as an outline; a loose crate joins the room seed. Turns E1 green.
2. **`clip:at(t)` and a toggle widget.** The binding in `gfx.rs`, a ~20-line Lua state helper
   (day → `to_night` → night), and a small test app using the toggle. Turns E2 green.
3. **The editor smoke.** `scripts/smoke_asset_editor.py`, added to `SMOKES`. Turns E3 green.
4. **The clip timeline.** One row per part, keys as dots, a scrub line; add, move and delete a
   key; one write on release. Turns E4 green.
5. **The kit split.** Move canvas, handles, drag and inspector out of `main.lua` behind small
   interfaces; a bespoke hero editor proves it. Waits on §4's shared-Lua question.
6. **Tilemaps — design only.** Tiles as assets, a grid layer the world draws and collides with.
   It touches `world/`, so it goes to the world session before anything is built.

## 4. Open

- **Sharing Lua between apps.** `require` loads only the app's own folder, so a kit is copied
  into each editor. Lua source as a workspace asset (`require("asset:kit/canvas")`) would share
  one copy — but shared code runs inside every app that loads it, a sandbox question.
- **Resizing UI assets.** A button grows to its label. Nine-slice is GameMaker's answer for
  pixels; for vector parts, pinning parts to edges may fit better. Undecided.
- **Live edits across apps.** Each app holds its own copy of an asset until sync lands
  (lua-apps.md, 2026-10-01): an edit in the editor reaches an open app only when it reopens.
- **The fixed step.** Stage-play tests passed through the 1/120 s step unchanged; E1 must still
  read positions with a tolerance and drive the world by frames, not by one long tick.
