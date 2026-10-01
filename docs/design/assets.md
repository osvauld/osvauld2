# Assets — drawings and clips as shared, editable files

Status: **design draft, 2026-09-29; nothing built.** Talked through, not yet a contract. Grows
out of [2D world](2d-world.md) §2–3: the drawing and clip formats it built become files that
many apps read and editors write.

Companions:
- [2D world](2d-world.md) — drawings, clips and the world that plays them.
- [Workspace permissions](workspace-permissions-sync.md) — who may read or write another item.
- [Agent source editing](agent-source-editing.md) — the bridge write path an agent uses today.

## 1. Where things stand (read from the code, 2026-09-29)

- **A drawing is a Lua source file.** `demo_apps/world/hero.lua` returns `size` and `parts`;
  each part has `id`, `parent`, `pivot` and `shapes` (path commands, `fill`, `stroke`).
  Coordinates are drawing-absolute. Clips live in a sibling `clips.lua`.
- **Dimensions are not stored — they are the path points.** A leg's length is `164 → 212` in
  its path plus its foot shape; no field says "leg length". `Pose` (`x, y, rot, scale`) is the
  only runtime change, and `scale` is uniform, so it cannot lengthen a limb.
- **One view per file.** Front, side and back are three files sharing part ids; one edit to the
  character is three edits.
- **`gfx.drawing(t)` takes any table** (`app_host/src/gfx.rs`), not only a `require` result.
- **A drawing already works outside the world.** `ui.frame({ visual = d:pose(...) })` shows it
  in any app, and a hover reports the part under the pointer (`demo_apps/hero`).
- **Clips only play inside `ui.world`.** The Animator is in the `world` crate; a `ui.frame`
  cannot play one.
- **App docs are private to their app.** `doc:open(name)` resolves to
  `ws/<ws>/item/<this item>/doc/<name>` (`vault/src/item.rs`); no app can open another's doc.
- **A doc mirror goes straight into `gfx.drawing`** — probed 2026-09-30
  (`a_drawing_mirrored_from_a_doc_draws`): a drawing written to a doc and read back through the
  mirror draws with its parts intact; the read side needs no copy. The write side is the cost:
  every nested table must be tagged `doc.map`/`doc.list`, so writing a whole drawing wants a
  small deep-tagging helper.
- `Drawing::part_at(id, pose)` (landed with `attach`) maps a point on a part through a pose —
  what an editor needs to put handles on a posed character.

## 2. The idea

**An asset is data; editors are many and disposable.** One file shape, read by every game and
every app; any number of editors write it:

- a **general editor** — select, drag points, pivots, parts, style, create shapes;
- **bespoke editors** made for one asset or a family ("hero editor" with an arm-length slider,
  a "toggle" editor with on/off states). The meaning of "arm length" is content, so it lives in
  Lua, per editor — Rust stays vocabulary, as [2D world](2d-world.md) §1 requires.

The primary workflow is **the user describes, the agent writes, the user adjusts**: the agent
edits the asset through the bridge, the user fixes the last 20% by hand. As a CRDT file, both
edit the same asset at once.

Assets are not only for games. A bespoke toggle or button is a drawing with clips — `idle`,
`hover`, `press`, `on`, `off` — so characters, props and widgets share one pipeline.

## 3. Proposals

1. **An asset is a non-app item**, owned by no app, synced like any other doc, open to every
   app in the workspace — access follows the user and the workspace, not the app
   ([Workspace permissions](workspace-permissions-sync.md) §3: apps hold no key). Its content
   is the item's doc; it has no `src`. Being an item is what keeps kunki's sync, subscriptions
   and authorization unchanged — they address (workspace, item, layer) (`kunki/src/admin.rs`).
   Found by name with the prefix scan `items()` already does, until the designed-but-unbuilt
   workspace index replaces it ([Visual substrate](visual-substrate.md) §7).
   *Settled 2026-09-30; earlier the same day this read "a workspace-level doc at
   `ws/<ws>/doc/<name>`", which would have needed a new address through sync and auth.* **Opened by name,
   always relative to the workspace** — settled 2026-09-30; a rename is the index's problem,
   not the app's. Opened by a call
   distinct from `doc:open`, so the scope shows in source. The shell must hold **one shared Loro
   doc per workspace doc** — today every open app gets its own copy of each doc
   (`LuaApp::open`), so an editor's edit would not reach a running game.
   *Revised 2026-09-30:* this read "an asset is its own item, kind `asset`, opened by item id,
   read-only unless declared". Only apps exist as items in practice; data is meant to be
   workspace-wide, so the item kind, the manifest declaration and the read-only split went.
2. **One asset holds its views and its clips:**

   ```lua
   {
     size = { 160, 240 },
     views = {                       -- map: front / side / back share part ids
       front = { parts = {           -- list: order is draw order; :move reorders
         { id = "arm_near", parent = "body", pivot = { 104, 98 }, shapes = {
             { id = "s1", path = { ... }, fill = "#f84aa7", stroke = { 2, "#1b1b3a" } },
         } },
       } },
     },
     clips = { idle = { ... }, walk = { ... } },   -- map by name, shared by every view
     meta = { hero_editor = { arm_len = 60 } },     -- one private section per editor
   }
   ```

3. **Points are the truth; `meta` is optional.** Every reader draws from resolved points. An
   editor's params sit in its own `meta` section; ignored or stale, the asset still draws.
   Storing only params would tie the asset to one editor's generator.
4. **Shapes get an `id`**, so a write addresses a shape by id, never by position. `gfx.drawing`
   rejects unknown fields today and must learn it.
5. **A path is written whole, on release.** A drag updates locally and commits one write when it
   ends (the kanban resize pattern). Two people editing one shape at once: last write wins.
   Per-point ids are not worth their cost here.
6. **`ui.frame` plays clips**: `ui.frame({ drawing = d, clip = c })`, played in Rust like `fade`,
   so a widget animates without per-frame Lua. Not a new prop alone (world session's review,
   2026-09-30): `clip.rs` moves from `world` down into `runtime`, next to `Drawing` — its only
   dependency is `Pose`. Then the frame needs an `id` to keep a clip's start time across view
   re-runs (the world keeps it on the entity; a `ui.frame` has no retained slot), the runtime
   frame clock plus a way to request frames (the world's `needs_ticks`), and clip handles made
   once, outside `view` — a `gfx.clip` per view is a new handle, so a restart every frame. A
   finished once clip still requesting frames is a known, unbuilt fix in the world too.
7. **`use` versus `attach`.** `use` places one asset inside another — part of the design, a
   sword drawn into the hand. `attach` joins two entities at play time — the chest picked up.

8. **A live edit reaches a running game through a new handle.** Reconcile recomputes the rest
   pose when the drawing's handle changes, and a clip whose handle is unchanged keeps playing
   (world session's review, 2026-09-30). The game rebuilds `gfx.drawing` **only when the doc
   changes** — one per view means a rest-pose recompute per entity per frame — so the mirror
   needs a change signal Lua can see: proposed, a version number on the mirror.
9. **Play is lenient about missing parts; the editor is strict.** Reconcile is all-or-nothing
   today: a clip track or an `attach` naming a part the drawing lacks errors the whole
   `ui.world`, which keeps its last good state. Under live, concurrent editing an asset is
   briefly inconsistent all the time, so deleting a part would freeze the game. Proposed: a
   track for a missing part is skipped and logged once to the console; the editor warns
   ("clip `walk` moves `arm_far`, which no longer exists"). A design call to make with the
   world session, since reconcile is theirs.

## 4. Editor tools, in the order they pay off

Select part or shape → drag points and curve handles → pivot and part operations (reparent,
reorder, add a part for fingers) → fill and stroke → create rect and ellipse → pen tool → scrub
a clip to check an edit animates. The agent covers creation well, so the pen tool comes last.

## 4b. Colour: palette tokens, a picker, gradient control (talked through 2026-10-01)

Built so far: the inspector (selected part's pivot, fills, strokes, shadow blur and colour, as
inputs — Enter writes if it parses). Gradients show but don't edit; colours are typed hex.

*Revised 2026-10-01 — palette became `vars`, and P1 is built.* What we're formalising isn't skins
but an asset's **inputs**: named values with defaults that its user may override — the inputs
half of the asset's interface, beside sockets and plugs (attach), clips (behaviour) and events
(out). So the field is `vars`, and a var is a colour **or a lone number** (stroke width, shadow
radius and blur): any value substituted whole. Path points stay literal — "arm length" moves many
points at once and needs expressions or part scaling, a separate step. Read "palette" and
"token" below as `vars` and `@name`.

**Palette tokens.** An asset names its colours once; shapes refer to the name.

```lua
palette = { skin = "#e8b894", outline = "#1b1b3a", shirt = "#f84aa7" },
-- …
{ path = { … }, fill = "@shirt", stroke = { 2, "@outline" } }
```

One edit recolours every shape that uses the token; an agent edits meaning ("make the shirt
teal") rather than hunting hex; a widget's tokens are its theme, so an app can restyle the toggle.

- **`gfx.drawing` resolves them**, so every consumer — editor, world, apps — gets them free. An
  unknown token is an error naming the shape, like any other bad field.
- Tokens may stand wherever a colour does: fills, stroke colours, gradient stops, shadow colours.
- **Variants** are `gfx.drawing(data, { palette = { shirt = "#2ec4b6" } })`: overrides merged over
  the asset's palette, compiled once per variant — a team colour or a night version without
  copying the asset. Not per pose: that would rebuild brushes every frame.
- No alpha modifier (`"@skin/50"`) in v1; a glow that needs one gets its own token.
- Later: a socket may hand its host's palette to whatever it mounts, so a hat can wear the
  hero's colours. Interface work, after stage play.

**The picker** (`colour.lua`, reused by the inspector, gradient stops and stage fields): a swatch
on each colour field opens an overlay with a saturation/value square, a hue strip, an alpha
strip and a hex field — the square and strips are stacked linear gradients in one frame. Its
swatches are the asset's palette, so you pick `@outline`, not a lookalike. A drag updates the
picker only; release writes once.

**Gradients** have two kinds of control. Their colours — stops, kind, extend — are fields: a
preview strip, one row per stop (offset 0–1, colour via the picker), add and remove, a kind
switch that seeds default points from the shape's bounds. Their geometry — linear from/to, radial
centre and radius, sweep centre and start angle — is handles on the canvas, sharing the point-drag
machinery of E2b.

**Slices:** P1 `gfx.drawing` resolves `vars` (built) → P2 variants (built) → P3 vars in the inspector (built),
colour fields take tokens → seeds move to tokens → C1a picker (square, hue, hex) → C1b alpha and
palette swatches → Ga gradient stops, kind, extend → Gb gradient handles with E2b point handles.

## 5. Motion and physics

Three kinds of motion, in build order:

| kind | moves | example | state |
|---|---|---|---|
| clips | parts, by keyframes, the same every time | walk, jump, a lid opening | built, in `ui.world` only |
| springs on parts | parts, driven by a spring instead of a key | a cape swinging after a stop, a button squishing on press | not built |
| deformation | path points, driven by a solver | a ripple across a toggle, jelly, cloth | not built; `Pose` cannot express it |

**Two solvers, split by what they simulate.** Rapier handles rigid bodies — walls, bumping,
sensors — and is the world's slice 5 ([2D world](2d-world.md) §7), not this doc's. Soft things
(rope, cloth, hair, ripples) are verlet/PBD: points moved by their implied velocity, then nudged
until distance constraints hold. Its core is ~50 lines and we write it — Rapier has no soft
bodies and Avian needs the Bevy app. The work is the seams: particles from an asset's paths,
pins riding a part through a clip, moved points back to paths, input poking points
([Visual substrate](visual-substrate.md) §5.1, [Environment runtime](environment-runtime.md)).

**The same split as clips: the asset describes, Rust runs, Lua reacts.** The asset stores a
material (`soft = { part = "cape", pin = { part = "body" }, stiffness = 0.4 }`); Rust ticks it;
Lua acts at moments (`poke`, `on_settle`). The simulation is view state — only its parameters go
in the CRDT, since two machines running one sim diverge ([Visual substrate](visual-substrate.md)
§5.3).

**Rust holds solver kinds; Lua holds materials.** `ripple`, `jelly`, `cape` and `rope` are Lua
libraries that generate soft descriptions — a new material is a Lua file an agent can write
([Visual substrate](visual-substrate.md) §7: if the standard library has to be Rust, the
primitives are wrong). Rust grows only for a new kind of physics. Real fluid (sloshing, smoke) is
the first such kind and is out of scope; a "liquid" toggle is a springy outline, or metaballs,
which need a render pass that is its own project.

**Physics belongs to the entity, not the drawing** — the same drawing is a solid chest in a game
and a plain button in an app. An asset may still carry a *default* collider, in slice 5's
vocabulary, drawn in the editor, so a game does not guess the chest's footprint.

## 5b. Events

An asset is data and sends nothing. It **declares named moments and places**; whatever plays it
passes them to Lua. Rust carries the strings and never interprets them.

| source | the asset says | reaches Lua as | state |
|---|---|---|---|
| clip markers | *when* — `events = { {0.15, "click_sound"} }` | `on_clip` | designed ([2D world](2d-world.md) §3), not built; `on_clip_end` is built |
| hits | *where* — its part ids | `e.shape` on a pointer event | built for posed frames |
| materials | nothing — the solver reports | `on_settle` and the like | with the soft solver |

**Meaning is not in the asset.** "The toggle turned on" is a decision, so it is Lua. A bespoke
widget is two things: the asset (look, motions, named moments, a `knob` part) and a Lua component
that wraps it (a click flips, and it emits the app-level `on_change`). Both are things an agent
writes. The sketch assumes proposal 6 (`ui.frame` plays clips):

```lua
return function(p) -- toggle.lua: p.id, p.on, p.on_change
  return ui.frame({ id = p.id, drawing = art, clip = p.on and on_clip or off_clip,
    on_click = function() p.on_change(not p.on) end,
    on_clip = function(e) if e.event == "click_sound" then play("tick") end end,
  })
end
```

## 5c. Stages and interfaces

*Talked through 2026-10-01.* Three tiers, each with one home:

| tier | is | stored as | made by |
|---|---|---|---|
| asset | one thing: parts, shapes, clips | an asset doc | the agent; the asset editor |
| stage | several assets placed together: positions, mounts, which clip each plays | an asset doc of another shape — data | the editor's stage mode |
| interaction | what happens when: click → `to_night`, E near the chest → carry | Lua | the agent; the editor runs it to preview |

**Whether pieces are one asset or several** turns on whether they move together. The toggle's knob,
moon, sky and clouds are timed in one clip, so it is one asset; split, the timing would have to be
coordinated across clips from Lua. A piece that is *reused* becomes its own asset placed with
`use` (a cloud in the toggle and in a game background). Things that *act independently* — hero
and chest, three toggles on a panel — are separate assets in a stage.

**A stage** is `{ scene = { size = {w, h}, entities = { { id, asset, pos, clip?, mount? }, … } } }`.
Edit draws every entity as a named instance in one frame, so a drag reports the entity; play is
the same scene as a `ui.world`, clips per entity, mounts as `attach`.

**Interfaces — sockets and plugs.** A hat is the classic swappable piece. The contract has two
halves, matched by declaration like a Go interface:

- a **socket** on the host says what it accepts and where:
  `sockets = { head_top = { kind = "hat", part = "head", at = {80, 30}, width = 60 } }`
- a **plug** on the mountable says what it is and where it joins:
  `plug = { kind = "hat", at = {30, 44} }` — the point on the brim that sits on the head.

Any asset whose plug `kind` matches a socket's mounts there, and rides the socket's part through
every clip: in play, a mount is `attach = { to, part = socket.part, at = socket.at - plug.at }`.
`width` lets the editor scale to fit and warn on a mismatch. The same shape serves wearables,
held items (a sword in `hand_r`), and a button's icon.

*2026-10-01, from the world session:* the world maps `at` through the carrier part's current pose
and mirrors it when the carrier is flipped, but does not copy flip over — stage play sets the
mounted entity's `flip` to its host's. The carried box never rotates or scales, so a socket on a
part that turns (the hero's `idle` tilts the head 3°) moves the plug off the socket by up to
`|plug.at|` — about 3 units for the hat. Asked of the world session: a mount that turns and
scales with its part, before S2. **Agreed:** `attach = { to, part, at = socket.at,
pivot = plug.at, turn = true }` places the mount by carrier × part (composed through its
parents) × translate(`at`) × translate(−`pivot`), so the hat turns about its brim. `turn` and
`pivot` are opt-in (a carried chest stays upright). With `turn` the carrier's mirror is already
in the transform, so setting `flip` too is an error; without it, stage play sets `flip` as above.
A carried entity has no body — collider and sensor off while attached; dropped, it is upright.
The world session builds it.

**Behaviour interfaces.** An asset can claim to implement one: `implements = { "toggle" }`, where
`toggle` means *clips `on`, `off`, `to_on`, `to_off` and a `knob` part*. A Lua component is
written against the interface, not a drawing, so any conforming asset is a drop-in skin — the
day/night toggle, a lever, a candle. The editor checks conformance ("claims `toggle`, has no
`to_off`"). Where interface definitions live is open.

**Not interfaces — inside one asset:** *variants*, alternative shape sets for a part (mouth
`smile | open | flat`, an open or closed hand), picked by a clip or by Lua, so a blink is a clip;
and *palettes*, named colours (`skin`, `shirt`, `accent`) the user of an asset fills in — team
colours, a dark toggle. Neither is built.

**Slices:** S1 the stage doc and stage edit (hero, chest and a hat on the hero's `head_top`; drag
to move, one write on release); S2 stage play (clips, mounts as `attach`); S3 behaviour preview.

## 5d. Split with the world session

*Agreed 2026-10-01.* Both branches build on the same crates; this is who changes what.

| piece | owner | note |
|---|---|---|
| move `clip.rs` from `world` into `runtime` | assets | unblocks proposal 6; its only dependency is `Pose` |
| clip `events = { {t, "fire"} }` as data, `events_between(t0, t1)` | assets | after the move |
| firing them as `on_clip(e)` (`e.id`, `e.name`) in `World2d::tick` | world | like `on_clip_end` |
| `opacity` as a `Pose` field and clip track | assets | with the move; a part at opacity 0 answers no hits |
| part-level clipping: a clip path on a group or part, for draw and hit | assets | a new frame item, not `Frame::clipped` (a whole-frame box only `ui.world` uses) |
| clip layers: `clip = { walk, carry }`, later clips overriding what they name | world | composes at sampling time, so the move is unaffected |
| lenient reconcile (proposal 9) | world | a track naming a missing part is skipped with one console note; a missing attach part stays an error |
| colliders and sensors | world | `{ circle = r \| rect = {w, h}, at = {x, y}? }`, strict; grows by new one-of keys and optional fields |

The world session stays out of `drawing()`, frame items and brushes; this branch stays out of
`entity()`, `collider()` and `attach()`.

Consequences for stage play (S2):

- **An asset's default collider and sensor use the world's shape verbatim** — a one-of plus
  optional fields, so `capsule` or `layer` arriving later needs no format change here.
- **A mount passes its host's `flip` to the mounted entity.** The world mirrors the anchor and
  places the child's right edge there, so an unflipped hat only lands right if its plug is
  horizontally centred; flipping it with the host is correct whatever the plug.
- A mounted entity has no body (an attached entity's collider and sensor are off); unmounted, it
  gets one and leaves with its host's velocity.

## 6. Open

- the workspace-level open call's name (`space:open`, `ws:open`, …);
- which `ItemKind` an asset takes — an existing one or a new `Asset`;
- where a `use` reference points — an item id, and what happens when that item is gone;
- whether views stay a map or become one drawing with per-view overrides;
- an asset's default collider, once slice 5 fixes the collider vocabulary;
- SVG import, so the editor has a way in from Figma and Inkscape — export is Frame plan phase H,
  import is planned nowhere;
- the paint the editor's style panel wants: radial and sweep gradients, clips, blend modes and
  group opacity are Vello-native and each a small exposure slice ([2D world](2d-world.md) §2).
  *2026-10-01:* radial and sweep gradients and soft shadows are built; clips and opacity are ours
  in §5d; blend modes remain open.
