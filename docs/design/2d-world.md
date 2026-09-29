# 2D world — drawings, animation and a Rust-run game world

Status: **design draft, 2026-09-28; nothing built.** Agreed in conversation, not yet reviewed as a
contract. This is the 2D, game-first slice of the [Environment runtime](environment-runtime.md):
it narrows that plan's World, scheduler and Rapier2D rows to one milestone. Names and Lua shapes
below are proposals until the slice that builds them lands.

Companions:
- [Environment runtime](environment-runtime.md) — the parent plan: layers, authority, gates.
- [2D tank game](2d-tank-game.md) — the Lua-owns-everything proof this revises for games.
- [Animation](animation.md) — the declare-in-Lua, run-in-Rust pattern this extends.
- [Frame implementation](frame-implementation-plan.md) — the `gfx.*` resources drawings compile to.

## 1. The split — Lua describes, Rust runs

The pattern already shipped for UI animation (`fade = {1, 140}`: Lua states intent, Rust eases
every frame, `on_faded_out` reports back) becomes the rule for game objects. Almost every mechanic
decomposes into five blocks; each has one owner:

| block | owner | Lua sees |
|---|---|---|
| input (continuous: hold to move, aim) | Rust | a declaration: `controller = { speed = 120, axis_x = { neg = "KeyA", pos = "KeyD" } }` |
| input (discrete: press E, fire) | Rust routes, Lua decides | `actions = { interact = "KeyE" }` → `on_action(e)` |
| detection (in range, hit) | Rust (Rapier sensors/contacts) | `area = {…}`, `collider = {…}` → `on_enter` / `on_exit` / `on_hit` |
| transformation (motion, animation) | Rust | `play = "walk"`, `speed`, velocity |
| communication, state and logic | Lua | plain Lua, reacting to events |

**Rust holds the vocabulary; Lua writes the content.** Rust knows primitives — part, shape, path
command, pivot, transform, an axis driven by two keys, a sensor — plus their validation, caps and
rendering. It never names content: no hero, leg, walk, chest or WASD preset. To Rust, `leg_l` is a
string id with a parent and a pivot. A Rust change that names a game concept is in the wrong place.

No Lua runs per tick to move a hero. `on_frame` stays the escape hatch for custom behaviour, still
experimental per the parent plan's Gate 0.

## 2. Drawings — our own vector representation

A character or prop is a **pure-data Lua module**: tables, numbers and strings only, no functions.
Rust reads it; an agent edits it with `edit_file`; a future vector editor rewrites it; it can move
into a CRDT doc later without changing shape. App source already syncs (`SyncLayer::Src`) and hot
reloads, so a drawing edit reaches a running game.

Images are not the primary asset: an agent cannot meaningfully edit a PNG, and a drawing's named
groups are already its rig. `gfx.image` remains for imported art, off the critical path. Pixel art
wants its own grid representation, not vector squares (§6).

```lua
return {
  size = { 32, 48 },
  parts = {                                        -- list order is draw order, back to front
    { id = "arm_r", parent = "body", pivot = { 22, 18 } },                 -- group: no shapes
    { id = "body", pivot = { 16, 30 }, shapes = {
        { path = body_outline, fill = "#f84aa7", stroke = { 1.5, "#1b1b3a" } },
        { path = body_shade, fill = "#c23a86", blend = "multiply", clip = 1 },
    } },
    { id = "sword", parent = "arm_r", pivot = { 22, 36 }, use = "sword" },  -- another drawing
  },
  clips = { … },                                   -- §3
}
```

Three kinds of part — **shape** (has `shapes`), **group** (none), **use** (places another drawing,
which keeps its own parts and clips). `parent` is the hierarchy; list order is draw order. They
are deliberately independent: a far arm belongs to the body but draws behind it, which a nested
layout cannot express. **Parts are what move; shapes are the painting inside a part** — detail
adds shapes, not parts.

Paths use today's `gfx.path` commands. Paint is today's solid and linear gradient; radial/sweep
gradients, clips, blend modes and group opacity are Vello-native and each a small exposure slice.
General blur is not native and is deferred.

## 3. Clips — animation as data

```lua
walk = {
  length = 0.6, loop = true,
  tracks = {
    leg_l = { rot = { {0, -20}, {0.3, 20, "in_out"}, {0.6, -20} } },
    body  = { y   = { {0, 0}, {0.15, -2}, {0.3, 0} } },
  },
  events = { {0.3, "step"}, {0.6, "step"} },       -- reach Lua as on_clip
},
```

A track is keyframes `{time, value, easing?}` for one property of one part. Rust samples clips on
the world clock; the virtual clock makes "screenshot at t = 0.3" exact. Fundamentals, in build
order: tracks and keyframes, easing, loop/once/hold, events, playback speed, transitions (walk →
run crossfade), layers (legs walk while arms shoot). Tweens are two-key clips; sprite animation is
a track of frame indices.

*Built 2026-09-28 (slice 3):* tracks, keys, easing and loop/once-hold. A key's easing shapes the
segment arriving at it. A clip plays from when its handle first appears on an entity; switching
handles is how Lua switches clips. Events, speed, transitions and layers are not built.

## 4. The world element

A `ui.world` leaf — like `ui.scene3d`, one element owning a retained world. Lua describes entities
as data; Rust keeps them keyed by id and reconciles each description: new id spawns, missing id
despawns, changed data updates. Systems run fixed-step on the world clock, explicitly ordered:

input → controller → movement (Rapier character controller) → physics step → detection →
animation → transform propagation → render → events to Lua

```lua
ui.world({ id = "room", w = 900, h = 600,
  { id = "hero", pos = {100, 200}, drawing = "hero", play = walking and "walk" or "idle",
    controller = { speed = 120, axis_x = { neg = "KeyA", pos = "KeyD" },
                   axis_y = { neg = "KeyW", pos = "KeyS" } }, collider = { circle = 12, layer = "player" } },
  { id = "wall:n", body = "static", collider = { rect = {900, 20} }, pos = {0, 0} },
  { id = "chest", pos = {400, 220}, drawing = "chest", area = { circle = 40, detects = "player" } },
  actions = { interact = "KeyE" },
  on_enter = function(e) … end, on_action = function(e) … end,
})
```

**Position ownership.** `pos` is the spawn position. Once spawned, Rust owns live position;
re-sending `pos` does not fight the simulation. Lua reads live state through a read-only userdata
view whose `__index` reads the world directly (no per-frame table copy), and writes only through
explicit commands (`teleport`, `push`) — the document mirror's rule, reads free and writes explicit.

## 5. Libraries

- **`bevy_ecs`, standalone, default features off.** Change detection (reconcile, redraw),
  hierarchy (rigs), events (to Lua) and ordered schedules are what we would otherwise hand-write;
  `hecs` offers none of them. Confined to the world crate; Lua never sees ECS, only our
  description, so it stays replaceable. Pin the version — upstream breaks every few months. This
  settles the parent plan's open "ECS/storage library" decision for the 2D world; it does not
  reopen the closed Bevy-as-engine decision.
- **Single-threaded to start.** Not for determinism — conflicting systems never run in parallel,
  and ordering is fixed with explicit chains either way — but because systems at hundreds to
  thousands of entities cost microseconds and thread handoff costs more. One feature flag later,
  after a profile.
- **`rapier2d`** for detection and collision: sensors (detection, no push), static/kinematic/
  dynamic bodies, contact events, and the kinematic character controller (slide, slopes, snap).
  Each entity holds its Rapier handle — the only link between the libraries. `parry2d` is too low;
  `avian2d` needs the full Bevy app. `enhanced-determinism` is a candidate for cross-architecture
  lockstep; unverified until probed.

## 6. Evidence — the sprite probe (2026-09-28)

Release `shell2`, offscreen, sprites bouncing via `on_frame`, wall time per driven frame
(whether this includes GPU completion is unverified):

| sprite | count | ms/frame |
|---|---|---|
| 16×16 pixel art as vector squares (~900 path commands) | 20 / 40 / 70 | 0.6 / 0.9 / 1.2 |
| one rectangle | 500 / 1000 / 2000 | 2.0 / 2.9 / 5.1 |

Speed was not the limit; the budget was. `runtime/src/frame.rs` caps a frame at 65,536 path
commands and 4,096 items, and instances count at their full expanded cost, so ~70 pixel sprites
or ~4,000 single-item instances. The caps will be raised; the Lua/render cost split is not yet
measured.

## 7. Milestone — the chest

A hero walks a walled room with WASD, idling and walking; near the chest, E opens it with an
animation and it stays open. It exercises all five blocks. Slices, each visible on screen:

1. **Drawing module → visual.** `gfx.drawing(module)` validates the module into a retained part
   tree; `drawing:pose(overrides?)` returns an ordinary `gfx.frame` — the rest pose, or parts
   rotated/moved about their pivots, so hierarchy is checkable by screenshot before any Animator.
   `clips` and `use` are rejected as unknown until their slices.
2. **`ui.world` + reconcile.** Entities spawn, update and despawn by id in `bevy_ecs`.
3. **Animator.** Clips play in Rust; the hero idles and walks in place. *(Landed with an idle and
   the chest's open/close on click; the walk cycle waits for the controller.)*
4. **Controller.** WASD moves the hero; Lua switches `play`. *(Revised 2026-09-28: Lua never sees
   held keys or velocity, so the controller carries `moving = clip`, played in Rust while it moves
   — still vocabulary, not content. Landed that way.)*
   *4b, added 2026-09-28:* facing — per-direction views (front, back, side mirrored for left),
   chosen by the controller's movement. *4c:* draw order by feet (landed as `order = "y"`), and jump as height
   above the ground position (so order and collision keep using the feet).
5. **Rapier.** Static walls, the chest sensor, `on_enter`, the E action, the chest's open clip.

Networking, clock sync across peers, tilemaps, `gfx.image` and AI players are out of scope.

## 8. Decisions

**Settled 2026-09-28:**
- paths are in **drawing-absolute** coordinates; a part rotates about its `pivot`, also absolute;
- drawings compile in **Rust** through `gfx.drawing(module)` — strict validation like the rest of
  `gfx`, and the part tree is kept for the Animator rather than flattened in Lua.
- the first easing set is **`linear` and `in_out`** (smoothstep), named, not cubic-bezier numbers;
  more names join when a clip needs them. *(Was open with `linear`, `ease`, `step` recommended.)*
- the first animatable properties are **`x`, `y`, `rot`, `scale`** — exactly `Pose`. *(Was open
  with `opacity` also recommended; it waits for a Pose field.)*

**Open:**
- how a drawing module is named and loaded (`require` vs a declared `drawing = "hero"`);
- which Frame caps rise, to what, and whether instances keep counting at expanded cost;
- the event batch shape Lua receives, and the read-only view's API.
