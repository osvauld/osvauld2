# Brawl movement — one robot, one stage, the keyboard

Status: **steps 1–4 built 2026-11-10, step 5 built 2026-11-11, on branch `2d-characters` (not
landed).** It builds on `2d-characters` steps 1–5, which have not landed yet either. Decision 1 (movement
in Lua over rectangles, not Rapier) and the direction in §1a were **agreed by the user 2026-11-10**;
the other decisions in §1 are still *proposed*. What steps 1–5 measured and
corrected is in §4.

Why: [2D characters](2d-characters.md) gave us robots that play clips. They don't stand on
anything yet, and nobody controls them. A Brawlhalla-class fighter lives or dies on how movement
feels: run, turn, jump high or low, double jump, fast fall, dash, wall jump, and forgiving input.
Hits, damage and the second player come after. They all assume movement that is exact and
replayable, so that gets built first. Already agreed by the user: the characters are **generic
robots**, each player is on their own device (`net.match` already exists), **PC keyboard first,
gamepad later, no couch multiplayer**, and Lua files are the asset.

**The one idea:** a fighter is **plain data in a `net.match` state**. Its movement is a pure Lua
`step` over per-tick inputs, Jetpack's pattern, not `ui.world`/Rapier. Its picture is a function of
that state too: the clip and the time come from the fighter's action and how many ticks it has
lasted, never from the render clock. So the later network plan only has to add a second player:
rewinding the state already rewinds both the movement and the picture. **No Rust changes.**

```
demo_apps/brawl/
  tune.lua             -- every feel number, per tick
  fighter.lua          -- one fighter's tick: pure Lua, no gfx, ui or net
  stages/yard.lua      -- the stage as data: solids, platforms, blast zone, spawns, art
  camera.lua           -- framing for any number of fighters: pure Lua
  look.lua             -- action → clip, time, mirror, blend, pose cache
```

## 0. End-to-end tests (written first, red until their step lands)

`P` = pure Lua under stock `lua` 5.4 (`scripts/test_brawl_*.lua`, run by
`scripts/run_brawl_lua_tests.sh`, as Jetpack's are). `L` = a real sandboxed Luau VM through
`app_host` (`app_host/tests/brawl.rs`, beside `tests/characters.rs`). `S` = a bridge smoke over an
offscreen shell (`scripts/smoke_brawl.py`, added to `SMOKES` once green). In the smoke, a key is
`rpc.keyboard(code, key, down)` and time is `rpc.frame(n)`, which is 1/60 s and so two ticks.
*(Corrected 2026-11-10, measured in step 1: usually two. Now and then a frame runs one tick, when
the match's float carry falls a hair short, and the match then stays one tick behind. A
`keyboard` request is itself a driven frame plus 8 ms, so 2 or 3 ticks. `rpc.advance(secs)` runs
at most 8 ticks per paint (`CATCH_UP`), so it can't wait on a match. Use `frame(n)`.)*

**Units and order.** A tick is 1/120 s. One world unit is one unit of the robot's drawing
(the robot stands about 230 tall). +y is down, and the ground's top is y = 0. Each tick does, in
this order: read the input; set velocities (a jump sets `vy`); apply gravity; move; collide.
Tunables are stored **per tick** and chosen as dyadic numbers (multiples of 1/8), so every sum is
exact in binary. That is why the `P` tests below can say `==`, not "within ε". The numbers are a
first guess at the feel. The user retunes them by playing; tests name the tunable, and pin a
literal only where it proves the arithmetic.

| tunable | per tick | per second | meaning |
|---|---|---|---|
| `run_speed` | 7 | 840 u/s | top ground and air speed |
| `ground_accel` | 0.875 | 12 600 u/s² | 8 ticks from rest to `run_speed` |
| `ground_friction` | 1 | 14 400 u/s² | 7 ticks from `run_speed` to rest, no key held |
| `air_accel` / `air_friction` | 0.5 / 0.125 | | steering in the air; never pushes past `run_speed` |
| `turn_min` / `skid` | 4 / 1.5 | | reversing at ≥ `turn_min` skids (act `turn`) before facing flips |
| `gravity` | 0.25 | 3600 u/s² | |
| `jump_speed` | 13 | 1560 u/s | full jump: peak **331.5** above the ground |
| `jump_cut` | 7 | 840 u/s | release while rising faster than this: `vy = −jump_cut` |
| `double_jump_speed` | 12 | 1440 u/s | peak **282** above where it was used, from rest |
| `max_fall` / `fast_fall` | 14 / 20 | 1680 / 2400 u/s | |
| `coyote` / `buffer` | 8 / 10 ticks | 67 / 83 ms | |
| `dash_speed` / `dash_diag` / `dash_ticks` | 14 / 10 / 12 | | 168 units straight; no gravity while dashing; then `v × dash_keep` (0.5) |
| `wall_slide` | 3 | 360 u/s | fall cap while holding into a wall |
| `wall_jump` / `wall_lock` / `wall_jumps` | {8, −12} / 10 ticks / 3 | | away from the wall; steering locked; most per airtime |
| `drop_ticks` | 12 | | after Down on a platform, platforms are ignored |
| `respawn_ticks` / `stocks` | 120 / 3 | 1 s | |
| `body` | 80 × 200 | | the fighter's box, feet at its bottom centre (measured in step 4) |

### The stage and the rules (`fighter.lua`, `stages/yard.lua`)

| id | proof | step |
|---|---|---|
| P-B1 | the state is plain data by `net.match`'s rules: no functions, no metatables, no table held in two places, no cycles; `step` run on two deep copies of one state with one input gives equal states | 1 |
| P-B2 | a fighter spawned on the ground stands at `y == 0` with `vy == 0` and `grounded` for 600 ticks with no input | 1 |
| P-B3 | **one-way platforms:** a jump from below passes up through a platform, and the fighter lands on top of it (`y == top`) on the way down. A fighter whose feet were below a platform's top at the start of the tick is never snapped onto it | 1 |
| P-B4 | **drop through:** Down pressed while standing on a platform: the fighter falls below its top within 2 ticks and is not caught by any platform for `drop_ticks`. Down on solid ground does nothing | 1 |
| P-B5 | **walls:** moving into the side of the ground block stops the box at the wall line (`x` exact, `vx == 0`), and the state says which side is touching | 1 |
| P-B6 | **blast zone:** a fighter whose box centre leaves the blast rectangle on any side loses a stock, is `out` for `respawn_ticks`, then appears at the respawn point at rest with its double jump, dash and wall jumps refreshed. At 0 stocks it is `ko`; a restart press resets every fighter | 1 |
| P-B7 | the stage file is validated: unknown fields, a platform with no width, a spawn inside a solid, or a blast zone that doesn't contain the solids are errors naming the entry | 1 |

### Movement (`fighter.lua`)

| id | proof | step |
|---|---|---|
| P-M1 | **run:** from rest with Right held, `vx` after n ticks is `min(n × ground_accel, run_speed)`, exactly. On release it reaches 0 in 7 ticks and never goes negative | 1 |
| P-M2 | **turn:** at `run_speed`, holding Left: act `turn`, `vx` falls by `skid` a tick, and it clamps to 0 on the 5th tick. Facing flips on that tick, then it runs left. Reversing below `turn_min` flips facing at once with no `turn` | 2 |
| P-M3 | **full jump:** Space pressed and held from standing: the peak is exactly 331.5 above the ground, reached on tick 51 (the press tick is tick 1). The fighter lands on tick 103 | 1 |
| P-M4 | **short hop:** Space released on tick 2: the peak is exactly 107.25. Released on any later tick while rising, the peak lies between the two. With the input sent in frames of 4 ticks (as it is with others), a press and release inside one frame is still one short hop: a press is a count | 2 |
| P-M5 | **double jump:** one per airtime; it sets `vy = −double_jump_speed`; a third press does nothing; landing refreshes it. Act `jump2` | 2 |
| P-M6 | **coyote time:** walking off a ledge, a jump pressed k ticks after the last grounded tick is a ground jump when k ≤ `coyote` (the double jump is still there afterwards) and uses the double jump when k = `coyote` + 1 | 2 |
| P-M7 | **jump buffer:** a jump pressed k ticks before landing jumps on the landing tick when k ≤ `buffer`, and not when k = `buffer` + 1 | 2 |
| P-M8 | **fast fall:** a fresh Down press in the air while `vy ≥ 0` sets `vy = fast_fall` at once and caps the fall there until landing. While rising it does nothing. A normal fall caps at `max_fall` | 2 |
| P-M9 | **air dash:** `dash_ticks` at `dash_speed` toward the held direction (`dash_diag` on each axis for a diagonal; facing if none is held), gravity off; then the velocity is multiplied by `dash_keep`. One per airtime, refreshed by landing or by touching a wall. On the ground a dash is horizontal, with the same numbers | 2 |
| P-M10 | **wall slide:** in the air, touching a wall, holding toward it and falling: `vy ≤ wall_slide`. Not holding toward it: a normal fall | 2 |
| P-M11 | **wall jump:** a jump while sliding sets `v = wall_jump` (away from the wall). Steering toward the wall is ignored for `wall_lock` ticks. At most `wall_jumps` per airtime. The double jump is not spent | 2 |
| P-M12 | **snapshot, restore, replay:** a 1 200-tick script that uses every move above runs straight. A second run copies the state through a `net`-like encoder (numbers, strings, booleans, tables) at ticks 100, 400 and 777, throws the live state away, carries on from the copy, then rewinds to the tick-400 copy and replays to the end. The final states are equal in every field, compared with `==` | 1–2 |

### The camera (`camera.lua`)

*2026-11-11, P-C6 (shimmer after a stop):* the ease snapped at 0.05 units, so for ~1 s after a stop the camera moved sub-pixel and every layer and the robot resampled. Now the camera snaps within 0.5 screen px (`snap_px`), and the camera offset, robot, boxes and every layer are placed on whole pixels (`Camera.off`/`Camera.at`); the idle clip plants at one sole (`Look.STEADY`).

| id | proof | step |
|---|---|---|
| P-C1 | one fighter: the view contains its box plus `margin`, at least `min_view` wide, inside the camera bounds | 3 |
| P-C2 | **ready for two:** two fighters 2 000 apart are both inside the view with `margin`; the zoom never goes past `zoom_min`; the view never shows outside the camera bounds; a fighter that is `out` is left out of the framing | 3 |
| P-C3 | easing is a pure function `(camera, target, dt)`: the same dt sequence gives the same camera, and it converges on the target | 3 |

### The picture (`look.lua`, clips)

| id | proof | step |
|---|---|---|
| P-A1 | every action maps to a clip and a time `act_t / 120`, wrapped for looped clips and held for once clips; the table is total over the actions `fighter.lua` can produce | 4 |
| P-A2 | **blend:** a switch of action blends the old pose into the new over `blend_ticks`, computed only from state (`prev_act`, the tick of the switch). At the switch tick the pose equals the old clip's; at `blend_ticks` it equals the new one's; `rot` takes the short way round | 5 |
| P-A3 | the clip builder (`characters/robot/author.lua`): an empty clip equals the setup pose on every part that `setup.lua` tracks; a key is a change from the setup pose; key times rise strictly | 5 |
| P-A4 | **feet on the floor, checked by numbers:** for `idle`, `run`, `land` and `turn`, the lowest boot point (forward kinematics composed the way `robot/build.lua` composes the setup pose) is within ±6 units of the floor line at every 1/60 s. For `wall_slide`, the front hand or boot is within ±6 of the wall line | 5 |
| P-X1 | the copied files (`characters/paka/rig.lua`, `setup.lua`, the four reused clips, `characters/robot/*.lua`) are byte-identical to `character_lab`'s, as Jetpack's shared modules are checked | 4 |
| L-1 | the app opens through `LuaApp` with its files (PNGs as binary files): the console is clean, every clip file passes `gfx.clip`, each robot variant compiles, and the stage's pictures are counted against the budget (count and MiB in the step note) | 3–5 |

### Through the shell (`scripts/smoke_brawl.py`)

The app shows its state in a hidden text `fighter:state`: tick, x, y, vx, vy, act, act_t, face,
the double jump, dash and wall jumps left, stocks, and the highest point of the current airtime
(`peak`). Floats are printed with `%.17g` so Python reads them back exactly. It is never drawn,
like Jetpack's `match:checks`. Facts in the smoke don't depend on which tick a key lands on;
per-tick facts are `P`'s job.

| id | proof | step |
|---|---|---|
| S-1 | upload and open: the console is clean, the fighter stands on the ground (`y == 0`) with 3 stocks, and a screenshot is saved | 1 |
| S-2 | hold ArrowRight for 30 frames: `vx == run_speed` and `x` grew. Release for 10 frames: `vx == 0`. Hold ArrowLeft: `face == −1` after the turn | 1–2 |
| S-3 | Space held 40 frames: `peak == 331.5`. Space for one frame: `peak` lies between 107.25 and 331.5 | 1–2 |
| S-4 | jump up under the left platform: the fighter ends standing on it (`y == top`). ArrowDown: it falls through to the ground | 1 |
| S-5 | run off the stage edge and wait: stocks drop to 2, and after 1 s the fighter is back at the respawn point | 1 |
| S-6 | through keys: the double jump, fast fall, dash and wall jump each change their readout as P-M5–M11 say (counts and caps, not tick positions) | 2 |
| S-7 | across S-2–S-5 the fighter's frame (from `rects()`) stays inside the playfield. The camera readout's zoom stays within `[zoom_min, zoom_max]`. The dump lists the stage's pictures by path and their bytes | 3 |
| S-8 | **the robot is the fighter:** standing, running and in the air, the robot's frame is where the state says, mirrored when `face == −1` (the hovered part under a known point swaps sides), on the clip the act names. Screenshots go to `shots/brawl/` for the user | 4–5 |
| S-9 | **determinism through the real path:** the hidden `script` button switches the input to `script.lua`'s 1 200-tick timeline, a function of `match.frame` as in `mp_hockey`. Two runs (restart in between) give the same `match.checks` hashes and the same final `fighter:state`. That final state equals the one P-M12's straight run prints for the same script | 2 |
| S-10 | the existing smokes stay green: `smoke_characters`, all of Jetpack's, `smoke_mp`, `smoke_world`, `smoke_hero`; `character_lab` is unchanged except for the new clips it lists | all |

**Checked by eye, not by test:** each new clip on a contact sheet (`scripts/clip_sheet.py
<clip> <n>`: it drives `character_lab` offscreen through the clip picker and the exact time
field, screenshots the robot's slot at n times, and joins them with ImageMagick, as
`preview_character.py` does). How the movement feels: the user plays it and judges.

## 1. Decisions

1. **Movement is Lua over plain state, inside `net.match`'s `step`** — Jetpack's pattern, not
   `ui.world`. *Proposed.* The moves need per-tick control of velocity. A world controller moves
   at one fixed `speed` on its axes (`world/src/lib.rs` `Controller`), and `set` takes a
   velocity only for a loose body (`World2d::set`). One-way platforms need per-contact filtering.
   The world's Rapier step takes no physics hooks, and collision groups are fixed by the
   description. A driven world's agreement across x86 and arm has not been measured, since
   Rapier's `enhanced-determinism` is off (jetpack-mp.md step 6). In Lua, a box against a dozen
   rectangles is a few dozen comparisons a tick, it is exact, and it is already how Jetpack
   plays over the network (`world.lua`, `physics.lua`). Platforms, walls and blast zones are
   rectangles in the stage file. A platform blocks only a fighter moving down whose feet were at
   or above its top at the start of the tick, and only when not dropping through.
2. **Follow Jetpack's match pattern exactly.** *Proposed.*
   - `net.match{ id = "brawl", tick = 1/120, frame = 1 }` alone: no input delay, an input every
     tick. With others it will be `frame = 4`, Jetpack's numbers. The manifest declares
     `channel brawl` now.
     *(2026-11-10, step 1: a match alone sends nothing, so its rate is never checked
     (`net_match::create`). With others it needs `⌈1 / (tick × frame)⌉ + 2`, 32/s at `frame = 4`.
     The manifest declares `rate 60/s`, as Jetpack's `arena` does, not the 120/s §2 said.)*
   - What never changes goes in `setup`: the stage table, which is the stage file, and the
     tunables. `state` holds fighters, stocks and the tick. Rules reach the setup through a
     metatable that is set again at the top of `step`, because a rewound state is a fresh copy
     (Jetpack's `World.meta`).
     *(Revised 2026-11-10, step 1: no metatable. P-B1 forbids one on state. The `state`
     callback builds `rules = F.rules(setup)` from the setup it is handed (the starter's, with
     others) and keeps it in an upvalue, and `step` passes `rules` in: `F.step(state, inputs,
     rules)`. A rewind copies only state, so nothing needs setting again.)*
   - The input is a small plain table, quantized: `{ x = −1|0|1, y = −1|0|1, jump = held,
     jump_n, down_n, dash_n, restart_n }`. **A press is a count** (Jetpack's `pressed`), so an
     input applied to 4 ticks, or again after a rewind, acts once per press.
   - Keys come in through `on_key` on the playfield (with an `id`). A held set is kept, and
     `e.cancelled` clears it. `input()` folds the held set into the table (Jetpack's
     `controls.lua`).
   - `step` changes nothing but `state`. It iterates arrays, never `pairs`. It uses no `^` (it is
     constant-folded with the platform's `pow`), no `string.format` into state, no clock and no
     random numbers. The rules modules use only syntax stock Lua 5.4 also runs (no `+=`; a
     `math.clamp` shim), so `P` tests the real file.
   - The view reads `match.state` and draws moving things at `x + vx × match.ahead`. The camera,
     the pose cache and art are view-side and never feed `step`. A hot edit of `tune.lua`
     restarts the match (a reload starts matches over), which is how an agent or the user tunes
     it.
3. **No new Rust.** *Proposed.* Each thing the brief asked about, checked:
   - Mirroring: a posed frame drawn through `gfx.instance` with transform `{ −s, 0, 0, s, w, 0 }`,
     as Jetpack's pilots do.
   - Zoom: the `scale` prop on a `ui.frame` placed at `(x − cam.x) × scale`, as Jetpack's view
     does. The world camera has no zoom ("Not yet: zoom"), which is another reason not to use it.
     *(Checked 2026-11-10: `scale` scales about the element's own laid-out top-left
     (`runtime/src/layout.rs`), and its layout box stays unscaled. `rects()` reports the scaled
     box (an 80 × 200 fighter at scale 0.409 is 32.7 × 81.8), but only for an element with a
     pointer handler, so the fighter carries a no-op `on_hover`, as Jetpack's soldiers do.)*
   - Per-fighter scale: the same instance transform.
   - One-way platforms and walls: rules in Lua (decision 1).
   A Rust primitive would come up only if posing costs too much (decision 7), and that would be
   its own discussion.
4. **The picture is a function of the state.** *Proposed.* A fighter's state carries `act`
   (`idle`, `run`, `turn`, `rise`, `apex`, `fall`, `land`, `jump2`, `dash`, `wall`, `out`, `ko`),
   `act_t` (ticks in it) and `face`. The view picks the clip from `act` and samples
   `clip:at(act_t / 120)`, then `drawing:pose`, then mirrors by `face`. World clips are not
   used. They play on the render clock (`show_at`) and restart when the handle first appears, so
   after a rewind the picture would disagree with the state. Later, hitboxes that ride parts
   will be computed from that same `act_t`.
5. **Keep Paka's skeleton; hand-author new clips on it as changes from the setup pose.**
   *Proposed.* The robots are already built on it, and `idle`, `run`, `jump` and `fall_swim`
   play on them unchanged (S-C7). A simpler skeleton would mean redrawing three robots and
   re-authoring every clip, attack included, before anything moves. Two facts make authoring on
   this rig tractable:
   - The rest pose is not a pose (every bone unrotated), and `setup.lua` tracks 52 parts. So
     `author.lua` starts each clip from the setup pose and adds the author's keys, the same
     trick step 5 used to draw the robots.
   - The 12 bones Spine drove by IK or constraints are plain `rot` tracks in Paka's clips. A new
     clip sets them directly. Where feet must plant (`land`, `turn`, `wall`), `author.lua` gets a
     two-bone IK helper that runs at load time — Lua setting two rotations, which is decision 11
     of the characters plan. P-A4 checks the feet with numbers.
   A typical movement clip keys 10–15 bones.

   Clips needed: `rise`, `apex`, `fall` (or `fall_swim` if the user likes it), `land`, `jump2`
   (a tuck and spin on `root`), `dash`, `wall`, `turn`. Which of `jump`/`_no_weapon` variants is
   reused is picked by eye in step 4. **Revisit the skeleton** if authoring proves slow or the
   rig's extra bones keep getting in the way. The fallback is a robot skeleton of about 15 bones,
   with `run`/`idle`/`attack` retargeted by a script from Paka's.
6. **Transitions: short authored clips plus a short blend in Lua.** *Proposed.* The engine has
   no cross-fade, speed or layers ("transitions … are not built"). Switching handles restarts a
   clip, and `on_clip_end` reports the end of a once clip. None of that matters here, because
   decision 4 keeps time in state. `land` and `turn` are real clips. Other switches blend over
   `blend_ticks` (4 to start), lerping two `clip:at` tables. The source is `prev_act` and the
   switch tick, both in state, so the blend survives a rewind.
7. **Pose cost is measured, and cached.** *Proposed.* `clip:at` builds one Lua table per
   tracked part, and `drawing:pose` builds a new frame. The guide says to pose once at module
   scope, but a fighter can't. Clip time is quantized to 1/60 s (Paka's clips were authored at
   60 fps), and posed frames are cached per (robot, clip, frame index), as Jetpack caches its
   pilots. Blends bypass the cache. Step 4 records the cost per pose and the app thread's frame
   time with `OSVAULD_PERFORMANCE=1`. The gate is two robots under 4 ms of app-thread time per
   frame on the laptop.
8. **The stage is a Lua data file** (`stages/yard.lua`), validated by `fighter.lua` and sent in
   `setup`. *Proposed.* The first stage:
   - a solid ground block x −900…900, y 0…500 (its sides are walls);
   - one-way platforms at y −260 (x −620…−300 and 300…620) and y −520 (x −160…160);
   - spawn (0, 0) and respawn (0, −700);
   - blast zone x −2 100…2 100, y −1 700…1 100; camera bounds x −1 500…1 500, y −1 100…700.
   A full jump (331.5) clears a platform 260 up, and the top platform is reachable from a side
   one.
9. **Stage art from the CC0 pack, single PNGs from the 128 px set, no atlas.** *Proposed.* The
   set has 116 PNGs, 58 MiB as RGBA without mips. The budget counts mips (about ×1.33) and 64
   textures. A stage uses about 15 files:
   - `_tilemap_main.png` (1024², with ground and platform regions);
   - `platform_2/3`;
   - `background_mountain (1)/(2)`, `background_fog (1)`, `foreground_basic (1)`;
   - two trees, a bush, a rock, grass, two clouds.
   That is about 23 MiB, well inside 64 pictures / 128 MiB. `_tilemap_items.png` is 2048 × 2049,
   over the 2048-a-side limit, so it can't be loaded and isn't needed. Its items exist as single
   files anyway. The 256 px set costs four times as much for detail the camera won't show at
   zoom ≤ 1.

   The art frames are built once at module level and only placed each frame. Distant layers
   follow the camera by a parallax factor, computed in Lua. The pack's `License.txt` goes in with
   the PNGs. If the tilemap's ground regions don't tile without seams, the ground is a vector
   shape in the pack's colours with its grass strip on top.
10. **The camera is presentation, not state.** *Proposed.* It frames every fighter that isn't
    `out` plus `margin`, at least `min_view` wide (one fighter), zooms out to `zoom_min` for
    two, and stays inside the camera bounds. It eases on render time, as Jetpack's does. Built
    for N fighters now, so the second player only adds a box.
11. **Keys** (PC first, fixed for now). *Proposed.* A/D or ←/→ move. W/↑ and S/↓ give direction
    for the dash, and S/↓ is also fast fall and drop. Space jumps. Shift or K dashes. Enter
    restarts after `ko`. Up does not jump, so up plus dash isn't a jump.
12. **Name: `brawl`** (the app and this doc). *Proposed.* It is a working title; the user may
    want another.

## 1a. Where collision goes in the long run (agreed with the user, 2026-11-10)

Jetpack (`physics.lua`) and this app (`fighter.lua`) each hand-roll box-against-rectangle collision
in Lua. Two games copying it means a primitive is missing. The split: **Rust answers questions, Lua
makes decisions.**

1. **Rust: one small deterministic geometry module**, no game rules, no state. Sweep ("moving this
   box by this much, what do I hit first, where, and what is the surface's angle"), ray ("what is
   under this point, how far"), overlap ("which shapes does this box touch"; hitboxes need it too).
   Exact on every machine, so our own code in `dmath.rs`'s style, not a Rapier call.
2. **Lua: a shared movement library** on it: move and slide, stick to a slope, pass-through
   platforms. Jetpack and Brawl use it instead of their own copies; each keeps only its own rules
   (dash, wall jump, coyote time stay here).
3. **Lua: a shared character-drawing library**: foot planting and body lean from the ray query and
   a two-bone leg solve. View-only, set per character: upright with adapting legs (bipeds), a
   slight lean, or full slant (wheeled robots). The collision box stays upright in all three.

**Order.** This plan finishes on rectangles first (steps 3–5); the flat yard needs none of the
above. The geometry module and the movement library are the **next plan doc**, proven by a ramp in
the yard and by moving Jetpack onto the library with its tests unchanged. Foot planting follows,
once there is a ramp to stand on. Rejected: building the module first, which delays a playable
fighter and designs the query before two games can say what it needs.

One Rust change the user approved the same day: `net_match.rs` forgives 1e-9 s of clock rounding
(`CARRY_SLACK`), so two paints of 1/60 s are always two ticks of 1/120 s
(`every_second_paint_is_exactly_one_frame`). Step 1's note that `rpc.frame(1)` is occasionally one
tick describes the shell before that fix.

## 2. Where the code goes

- `demo_apps/brawl/` (new):
  - `manifest.osv` (`channel brawl { rate 60/s }`; corrected from 120/s, see decision 2), `theme.lua`.
  - `main.lua`: the match, keys, the view and the hidden readouts (`fighter:state`, `camera:view`,
    `match:checks`, the `script` button).
  - `input.lua`: held keys into the match's input, counting presses.
  - `tune.lua`: the §0 table.
  - `fighter.lua`: the rules, stage validation and collision. Pure; no `gfx`, `ui` or `net`.
  - `camera.lua`: pure.
  - `look.lua`: action to clip, blend, cache, mirror.
  - `stage_art.lua`: frames from the stage's art list.
  - `stages/yard.lua`, `art/*.png` and `art/License.txt`.
  - `script.lua`: the test timeline.
  - Copied from `character_lab`: `characters/paka/{rig,setup}.lua` and four of Paka's clips (no
    atlas: the robots name no picture), `characters/robot/{build,scout,brute,volt}.lua`.
  - New: `characters/robot/author.lua` and `characters/robot/clips/*.lua`.
- `demo_apps/character_lab`: the new robot clips are authored here, where the picker and the
  exact time field already are, and listed in `cast.lua`. They are copied into `brawl`, and
  P-X1 keeps the copies equal.
- `scripts/`: `test_brawl_rules.lua`, `test_brawl_moves.lua`, `test_brawl_camera.lua`,
  `test_brawl_look.lua`, `run_brawl_lua_tests.sh`, `smoke_brawl.py`, `clip_sheet.py`.
- `app_host/tests/brawl.rs`: L-1.
- Docs: `status.md` per step. `lua-apps.md` gains a short "a fighter drawn from match state"
  pattern (clip time from state, pose cache, mirror by instance), and the step-4 note's trap that
  `ui.state` is carried over after the new module runs.

## 3. Steps

1. **A box on a stage** (P-B1–B7, P-M1, P-M3, the straight half of P-M12, S-1, S-3's full jump,
   S-4, S-5). `fighter.lua` with run, jump, gravity, solids, one-way platforms, drop-through,
   walls, blast zones, stocks and respawn. `main.lua` with the match, the keys and the readouts.
   The stage and fighter are drawn as plain rectangles with Jetpack's simple fixed camera. It is
   playable at the end of this step. **Opus**: it sets the pattern every later step and the
   network plan follow.
2. **The full moveset** (P-M2, P-M4–M12, S-2, S-3, S-6, S-9). Turn, short hop, double jump, coyote
   time, jump buffer, fast fall, dash, wall slide and wall jump, and the script seam. The user
   plays and retunes `tune.lua`. **Sonnet** can do it: every rule and number is pinned by a
   test. An Opus review reads the state machine before the user does.
3. **The stage art and the camera** (P-C1–C3, L-1's pictures, S-7). The yard is built from the
   pack and the camera module frames N fighters. The budget numbers go in the step note, and
   screenshots go to the user. **Sonnet.**
4. **The robot is the fighter** (P-A1, P-X1, L-1, S-8). `look.lua`: Paka's `idle`, `run`, `jump`
   and `fall_swim` on the robot, chosen by `act`, mirrored, cached, and drawn at `match.ahead`.
   `body` is measured from the robot. The decision 7 cost is measured and recorded. **Opus**:
   the cost gate and the pattern that hitboxes will reuse.
5. **The robot's own clips** (P-A2–A4, S-8, the contact sheets). `author.lua` with its IK helper,
   then `rise`, `apex`, `fall`, `land`, `jump2`, `dash`, `wall`, `turn`, and the blend. **Opus**
   for `author.lua` and the first two clips. The rest are a good fit for **Sonnet** working from
   the contact sheet loop, one clip per subagent. The user judges each sheet and then the whole
   thing in play.

6. **A jetpack on the robot** (*proposed 2026-11-10, after the user asked for one "rigged … lifelike
   like what we have now"; tests not yet written*). A thrust rule in `fighter.lua` (hold to fly, a
   small fuel tank that drains in the air and refills on the ground, so it is a recovery tool, not
   free flight; numbers start from Jetpack's), the pack as a part on the robot's torso bone, and its
   own clips (ignite, hover, thrust forward, cut-off) with a flame scaled by thrust. After step 5,
   since it uses the same clip authoring.

Every step keeps `cargo test` (L-1 from step 3) and the smokes green (S-10).
The stage art and maps continue in [Brawl maps](brawl-maps.md) (2026-11-10), which replaces
decision 9 and `stages/yard.lua`'s `art` list.

After this plan, each its own: **attacks** (Paka's `attack` on the robots, then light and heavy
attacks and aerials authored with `author.lua`); **hitboxes that ride parts** (boxes on parts,
computed from `act_t`, so the same in a rewind); **knockback and damage** (percent, launch angles,
hitstun, dodge invulnerability); **the second player over the network** (Jetpack's `lobby`
roster, `frame = 4`, a second fighter in state; first, `net_sim` learns binary files and keys so
a two-player rollback test can run this app — today it loads only `.lua`/`.osv` and can only
click); **effects** (dust, landing puffs and dash trails as Lua particles, since world `emit`
isn't available on frames; opacity and tint are still missing in the engine); **sound**;
**gamepad**; off-screen indicators; more stages; ledge grab.

Not in scope: couch multiplayer, Rapier for fighters, touch controls, skinned meshes, a skeleton
of the robots' own (unless decision 5 is revisited).

## 4. Step notes

### Step 1 landed 2026-11-10 (branch `2d-characters`, not merged)

Built: `demo_apps/brawl/` (`manifest.osv`, `theme.lua`, `tune.lua`, `fighter.lua`, `input.lua`,
`stages/yard.lua`, `main.lua`); `scripts/test_brawl_rules.lua`, `test_brawl_moves.lua`,
`brawl_test_util.lua`, `run_brawl_lua_tests.sh`; `scripts/smoke_brawl.py`, in `SMOKES`. No Rust.

- **Passing:** P-B1–B7, P-M1, P-M3, and P-M12 over step 1's moves: run, reverse, jump onto a
  platform, drop through, run off the edge for a stock, restart. The copies at 100, 400 and 777
  and the rewind to 400 all match the straight run, compared with `==`. The final state is printed
  for S-9 to compare against later. These pass under `lua5.4` and under 5.5, and the same file
  runs in the app's Luau. The smoke passes S-1, S-3's full jump, S-4 and S-5.
- **The hand-worked numbers are right** under the stated order. The full jump peaks at exactly
  331.5 on tick 51 (and holds it on tick 52, where `vy` is 0) and lands on tick 103 with `y == 0`.
  It takes 8 ticks from rest to `run_speed` and 7 back to 0.
- **The respawn point is over the top platform.** A fighter respawned at (0, −700) lands on the
  top platform (`y == −520`), not on the ground. S-5 checks that, and that it was out for exactly
  `respawn_ticks`, read as `tick − act_t` at both ends.
- **Acts in step 1:** `idle`, `run`, `rise`, `fall`, `out`, `ko`. `act_t` is 0 on the tick an act
  begins. `apex`, `land`, `turn` and the rest come with their moves.
- **`peak`** is the height above where this airtime began: the ground, the platform walked off, or
  the respawn point. It is kept after landing until the next airtime.
- **The state** is `{ tick, fighters = { … } }`. Each fighter carries `x, y, vx, vy, face, act,
  act_t, grounded, on` (platform index, 0 if none), `wall` (−1/0/1), `drop, out, stocks`, the
  per-airtime `jumps2, dashes, wall_jumps` (refreshed by landing and respawn, spent from step 2),
  `take_y, top_y, peak`, and `seen`: the press counts already acted on. A restart keeps `seen`,
  so an old count doesn't fire again.
- **The match state is nil until the first paint**, alone too: the match starts in its first
  `advance`. The view draws the stage without a fighter until then.
- **Seams left for step 2:** `fighter.lua`'s velocity block (turn, short hop, double jump,
  coyote, buffer, fast fall, dash, wall slide and jump), the `dash` press already counted and
  ignored, and the per-airtime counts in state and readout. `tune.lua` holds only the numbers
  step 1 uses, plus the three per-airtime counts. The others come in with their moves, so tuning
  a number never does nothing.

### Step 2 landed 2026-11-10 (branch `2d-characters`, not merged)

Built: `fighter.lua` (turn skid, jump cut, double jump, coyote, buffer, fast fall, ground and air
dash, wall slide, wall jump), `tune.lua` (every §0 number), `script.lua` (the 1 200-tick timeline),
`main.lua` (the hidden `script` and `play` buttons, the `match:checks` readout, the dash/fast-fall
hint). `scripts/test_brawl_moves.lua` gained P-M2, P-M4–M11 and the real P-M12;
`scripts/smoke_brawl.py` gained S-2, S-3's short hop, S-6 and S-9. No Rust.

- **Every number in the plan was right** under the stated order, run rather than reasoned: short hop
  (release on tick 2) peaks at exactly 107.25, the double jump from rest at 282, the skid clamps to
  0 on the 5th tick (7 → 5.5 → 4 → 2.5 → 1 → 0), a straight dash covers 168 and leaves `vx = 7`.
  Nothing in step 1's tests changed. One step 1 smoke line did: S-4's jump was a tap and is now a
  short hop (it could not reach the platform), so S-4 holds Space.
- **Order of a tick** (the plan's "read, set velocities, gravity, move" with the velocity block
  spelled out): jump cut, steering (skid), jump (buffered or fresh), dash, drop or fast fall, gravity
  (wall-slide cap), move x, move y, wall read, blast check, dash end, per-airtime refresh, act.
- **Choices the plan left open, each pinned by a test** (the user may overrule any):
  - **The jump cut** applies to the ground jump only (including a coyote or buffered one), not to the
    double jump or a wall jump. It is judged on the tick *after* the jump, so a press and release
    inside one input frame (the press tick already reads "released") is the same 107.25 hop.
  - **The buffer** counts from the press to the first tick that *starts* on the ground, which is
    where the buffered jump happens (the landing tick itself has already moved). A press waits
    `buffer` ticks: pressed up to `buffer` ticks before that tick it jumps, `buffer + 1` does not.
    A press that something can jump from at once (a double jump left, a wall to jump from) is used
    at once and never kept, so a buffered press is always a ground jump or, near a wall, a wall jump.
  - **Coyote** is `coyote` ticks counted from the tick that walks off (that tick is still a plain
    ground jump): pressed up to `coyote` ticks after it is the ground jump with the double jump
    kept, `coyote + 1` is the double jump. A jump or a drop-through spends it.
  - **Skid** continues once started even below `turn_min`, ends if the key is let go, and the
    facing flips on the tick `vx` clamps to 0 (act `turn` through that tick, `run` after). In the air
    there is no skid.
  - **Wall jump** needs: airborne, touching a wall, *holding toward it*, and a wall jump left
    (falling is not required). With none left, or not holding toward, the same press is the double
    jump. `wall_lock` ticks ignore the held direction toward the wall (it counts as no key).
  - **Fast fall** persists to landing; a press while rising or while dashing is dropped, not kept.
    Sliding on a wall caps the fall at `wall_slide` even when fast-falling.
  - **Dash**: the press tick is the first of the 12; the velocity is set at once and `× dash_keep`
    is applied on the 12th tick's end. On the ground it is free (no cooldown, the air dash is
    kept); gravity stays on so the fighter keeps its footing, and a dash off a ledge keeps going and
    falls. In the air the direction is the held x and y (diagonal at `dash_diag`; y alone is
    vertical; none is the facing). No steering while dashing. A jump press cancels the dash and keeps
    its speed; a dash press on the same tick as a jump is ignored. A dash that runs into a wall
    stops dead (`vx = 0`) and ends, and one that lands ends too.
  - **Acts** added: `turn`, `jump2` (while the double jump still rises), `dash`, `wall`. A wall jump
    is `rise`. `apex` and `land` still wait for step 4.
- **New state**, all plain: `coyote` (ticks left), `buf` (ticks since an unused press, −1 none),
  `cut`, `ff`, `lock`, `lock_side`, `dash_t`, `dash_g`. `fighter:state` is unchanged.
- **The scripted run:** `script.lua` is built once from segments and read by tick. In the app the
  `script` button starts a *new* match whose input is `Script.at(match.frame + 1)` (an input for frame
  f is used for tick f + 1) and which stops stepping at tick 1 200, so the final state, and a
  `match:checks` hash at every 120th frame, are the same however many frames the driver runs
  after. Clicking `script` again is "restart in between". `play` goes back to the keys. A fresh
  match also gets a fresh `input` (press counts restart from 0 with the state's `seen`).
- **P-M12's script changed:** the new 1 200 ticks replace step 1's (which the 1 200 straight run
  and the copies at 100, 400 and 777 used). Drop-through and restart are still P-B4 and P-B6; the
  new script adds a stock lost to a wall jump out of the stage. The final state, printed by the
  test, is what S-9 compares the app's against.
- **Not checked:** the feel (the user retunes `tune.lua`), `look.lua`, and the Opus review the
  plan asks for before the user reads the state machine.

### Step 3 landed 2026-11-10 (branch `2d-characters`, not merged)

Built: `camera.lua`, `stage_art.lua`, `stages/yard.lua`'s `art` list and 12 PNGs in `art/` (with the
pack's `License.txt`), `main.lua` using both (the hidden `camera:view` and `art:loaded` readouts),
`scripts/test_brawl_camera.lua`, `app_host/tests/brawl.rs`, S-7 in `scripts/smoke_brawl.py`. No Rust
outside the new test file; every P-B and P-M test and S-1–S-6, S-9 pass untouched.

- **Budget (L-1):** 12 pictures, 12 textures of the 64, **14.33 MiB with mips** of the 128 (10.97 MiB
  of textures, 3.36 MiB of copied `src` regions). The plan's estimate was about 15 files and 23 MiB. The
  dump lists each by path and bytes (level 0 only: 9.2 MB).
- **Pack files used (128 px set):** `platform_4`, `background_mountain (1)` and `(2)`,
  `foreground_basic (1)` (the hedge), `tree` and `tree_2`, `bush`, `rock`, `grass_3`, `cloud` and
  `cloud_2`, and a 128 x 97 crop of `_tilemap_main.png` (its grass-topped ground block).
- **Corrections to decision 9:**
  - The tilemap's ground **does tile cleanly** (the 128 px column at x 896-1024 has matching edges), but
    the 1 MP tilemap alone takes a debug build's whole 1 s CPU budget to decode, so the crop is a file of
    its own (`ground_strip.png`). The vector fallback was not needed.
  - **`background_fog` is opaque white** in this pack, not mist (it reads as a white wall), so it is
    not used. The three back layers are the two mountains and the hedge.
  - Decoding is native work inside the calling entry, so the 12 pictures are built **a few per frame**
    (`StageArt.load`, about 400 000 pixels a frame; all 12 by the 7th view in a debug build, `art:loaded`
    reads `n/total`). Until then a layer is skipped, not an error. A release build could do it at once.
  - Layers that are bare silhouettes are **cropped by `src`** and continued downward by a plain block in
    the picture's colour (`body`), because the pack's hedge and fog pictures are solid down to their
    bottom edge and showed as a green wall past the stage edge. Copies of a tiled layer overlap by 4
    pixels: the edge pixels of a scaled picture are half covered, and abutting copies show a hairline.
- **Camera numbers** (`camera.lua`'s `params`): `margin` 260, `min_view` 1 300 wide (the top platform is in
  view from the ground), `zoom_min` 0.55 (the stage's bounds raise it to 0.65 at 900 x 662), `half_life`
  0.12 s. `zoom` is `ref_w / view.w` with `ref_w` 1 600. Easing is `k = 1 - 0.5^(dt / half_life)`, so
  a second is the same at any frame rate (tested), and views only interpolate, so every eased view is
  inside the bounds too. A fighter that is `out` or `ko` leaves the framing; with nobody in play the
  camera keeps its last view.
- **Added to the plan:** P-C4 (parallax: factor 1 stays on the map, 0 on the screen) and P-C5 (the art
  list against the files: sizes, `src` inside, factors in 0..1, layers wide enough for the bounds).
  `fighter.lua`'s stage check now allows an optional `art` table (the rules never read it).
- **Open:** off-screen indicators for a fighter outside the view are not built. The fighter is still
  the rectangle; `look.lua` is step 4. Two debug-build notes: the app thread's CPU budget is the
  reason for the incremental load, and the smoke's first 60 frames wait on `art:loaded`.
- **S-7:** 87 checks over S-2–S-5 that the fighter's frame is inside the playfield (skipped when out of
  play or below the camera bounds) and the zoom inside `[zoom_min, zoom_max]`; alone the camera sits on
  `zoom_max`. The smoke prints the dump's picture list size.

### Step 4 landed 2026-11-10 (branch `2d-characters`, not merged)

Built: `look.lua`; `characters/` copied from `character_lab` (`paka/rig.lua`, `paka/setup.lua`, clips
`idle`, `run`, `jump`, `fall_swim`, `robot/build.lua`, `scout.lua`, `brute.lua`, `volt.lua`; no atlas);
`main.lua` drawing the robot; `tune.lua`'s `body_h`; `scripts/test_brawl_look.lua`;
`scripts/measure_brawl_look.py`; L-1 in `app_host/tests/brawl.rs`; S-8 in `scripts/smoke_brawl.py`. No
Rust outside the test file.

- **Tests:** P-A1, P-X1 and the body measurement pass in `test_brawl_look.lua` (red first: no
  `look.lua`, then the body). Totality is checked two ways: the acts `fighter.lua`'s source sets or
  tests, and every act the 1 200-tick script reaches. L-1 passes (four clips at module scope, three
  robots built one a view by `look:warm()`, the first frame posed once and then cached). S-8 passes in the smoke with every earlier S check: the robot's
  frame is where `fighter:state` and the camera put it (to 1 pt), on the act's clip at the frame
  `act_t` gives, standing, running, rising and facing left; a 2 × 25-point hover scan facing right and
  facing left agrees mirrored on 26–27 of 29 hit points with 21 swapping sides. Screenshots:
  `shots/brawl/step4_{idle,run,air,mirrored,platform}.png` (checked by eye: on the ground, about 1.15
  boxes tall with the antennae, facing the right way).
- **The act table** (until step 5's clips): `idle`, `land` → `idle`; `run`, `turn`, `dash` → `run`;
  `rise`, `jump2` → `jump` (a once clip, held at its end); `fall`, `apex`, `wall` → `fall_swim`; `out`
  and `ko` are not drawn. The sheathed-sword variants (`idle`, not `idle_no_weapon`) were picked: the
  blade stays on the back. The seam for step 5 is `Look.ACTS` (a clip per act) and `L:frame` (a blend
  would bypass the cache there).
- **Time:** `Look.at` gives the exact time (`act_t % (length × 120) / 120` for a loop, `min(act_t /
  120, length)` for a once clip) and the frame index (`⌊act_t / 2⌋`, wrapped, or held). A once clip has
  one extra index for its held end, since Paka's `jump` is 0.6667 s, not a whole number of frames.
- **Mirroring is a root pose, not `gfx.instance`** (a change to decision 3): `x → −x`, `rot → −rot`,
  `scale_x → −scale_x` on the root, whose pivot is the drawing's centre line, mirrors the whole drawing
  in its own box. An instanced visual's part names are unreachable, and S-8's hover check and the
  hitboxes that ride parts both want them. Negative scale is accepted by `drawing:pose` unchecked.
- **`body` measured from the robot** (forward kinematics over the setup pose in the test): chassis and
  boots 81.5 wide, helmet top 210 above the feet (antennae, fins and dome left out). `body_w` stays 80,
  `body_h` becomes **208**. The cascade was one test literal (P-B6 placed feet at `body_h / 2` written as
  100); P-M12's final state did not change.
- **Feet on the floor** (the user, playing: the legs floated). Paka's clips move the root and legs for
  Paka's hooves, so a frame's soles are off the root's pivot by a different amount each frame (idle
  ≈ 241.1, run up to 243.3, against the pivot's 239). `Look.sole` runs forward kinematics over the
  boots' path points under the frame's own `clip:at` pose (composed as `runtime/src/drawing.rs` does;
  the test checks it against its own FK to 1e-9), once per cached frame. Acts marked `plant` (`idle`,
  `run`, `turn`, `dash`, `land`) are drawn with that lowest point on the feet line; air acts use the
  root's pivot. S-8 checks it by pixels: the lowest row of dark sole pixels is 0 px from the floor line
  on the ground, running and on the platform, and the drawn grass on the ground is 0 px under the soles.
  **Unfixed, by instruction:** on a platform the grass is drawn **6 px (7.5 units) under the soles**,
  because `platform_4.png` has 10 transparent rows on top and the yard's art `platforms.lift` is 4 (it
  would need 10). That is stage art, left to the map plan; S-8 prints that gap and does not assert it.
- **Pose cost (decision 7)**, `scripts/measure_brawl_look.py`, 900 × 700 offscreen, Intel Iris Xe, the
  scripted run with the hidden `pair` button on (a second robot, Brute, facing the other way), cold
  (filling the cache, 413 poses) then warm. App thread is the one whose `view_ms` is the app's:
  | build | robots | pose (miss) mean / max | app view p50 / p95, warm | app frame total p50 / p95, warm | cold total p95 / max |
  |---|---|---|---|---|---|
  | release | 2 | 1.00 / 2.12 ms | 0.82 / 1.34 ms | 4.70 / 6.88 ms | 8.31 / 14.22 ms |
  | release | 1 | 0.67 / 1.49 ms | 0.56 / 0.82 ms | 3.26 / 4.23 ms | 3.98 / 6.78 ms |
  | debug | 2 | 3.96 / 17.96 ms | 3.35 / 5.64 ms | 27.9 / 43.7 ms | 41.6 / 99.9 ms |
  **The gate as written (two robots, app thread under 4 ms a frame) fails in release: 4.70 ms p50.**
  Most of it is not the robots: one robot to two adds 1.44 ms p50 (0.26 ms of it view), so a robot
  costs about 0.7 ms a frame warm, and the rest is the stage's layout and paint. A cold frame poses
  each robot once more (1 ms each). There is no zero-robot baseline in the numbers. Debug is about
  six times slower and no gate applies to it. The pose cache stays bounded: 269 frames a face a robot.
- **Robots are built one a view, not at module scope.** With all three at module scope the debug
  `LuaApp` load took 0.56–0.82 s, and in the same run `character_lab`'s own three-robot load ran over
  its 1 s CPU budget once (`smoke_characters`, with a Jetpack smoke and a cargo test running beside
  it; `character_lab` is unchanged by this step). Now the module (clips and stage) loads in 0.28 s and
  `look:loaded` reads `robots=n/3`.
- **Open / unverified:** the whole-frame gate above (a decision for the user: count the robots' share,
  or trim the stage's frame cost); `character_lab`'s module load is near the debug budget under load
  (above); the platform art gap; `lua-apps.md`'s "a fighter drawn from match state" pattern is not written yet (§2's docs
  item), nor the root-pose mirroring note. `look:cost` uses `now()` (wall clock) for a diagnostic only.

### Step 5 landed 2026-11-11 (branch `2d-characters`, not merged)

Built: `characters/robot/author.lua` (clip builder, IK) and eight clips in `characters/robot/clips/`
(`rise`, `apex`, `fall`, `land`, `jump2`, `dash`, `wall`, `turn`), authored in `character_lab` (listed
in `cast.lua`'s `own`, so the picker and time field reach them) and copied into `brawl`; `look.lua`'s
act table on them, the blend; `fighter.lua`'s `land`, `apex`, `prev_act`, `prev_t`; `tune.lua`'s
`land_ticks` (12) and `apex_vy` (2); `scripts/test_brawl_clips.lua`; `scripts/clip_sheet.py`; P-A2
and the wider P-X1 in `test_brawl_look.lua`; S-8's step 5 checks in the smoke; L-1's counts. No Rust
outside the test file.

- **Tests:** P-A2, P-A3, P-A4 and the wider P-X1 pass under `lua5.4` (red first: no `author.lua`, then
  no clips, then the wall pose 9.25 past the line). L-1 passes (10 clips at module scope). The smoke
  passes with every earlier S check plus `land` (boots 0 px from the floor by pixels), `jump2` and
  `wall` (facing the wall) on their own clips, and `blend` only in an act's first 4 ticks at
  `act_t / 4`. `smoke_characters` and `smoke_jetpack` pass.
- **`author.lua`:** `A.clip{ length, loop, keys, ik }`. Every part `setup.lua` tracks starts at its
  setup value; a key on `x`, `y`, `rot` adds to it, on a scale multiplies it; times must rise strictly
  inside the length; an unknown part or property is an error naming it. The sword stays sheathed and
  the blinks shut unless keyed. `A.at` samples as `world/src/clip.rs` does and `A.world` composes as
  `drawing.rs` does, so the tests measure clips by numbers.
- **IK (a correction to decision 5):** the robot's legs are **one segment and a boot**, not two bones
  (`leg_*` → `hoof*`), so there is no knee to bend. Arms get the two-bone solve (`arm_front`,
  `arm_back`: shoulder and forearm to put the palm's pivot on a point, `bend` picks the elbow's side).
  Legs get a **piston**: the leg turns to the target ankle and stretches along its axis, and the boot
  keeps its setup world angle and size. Both are solved at load time at every 1/60 s, the targets
  eased between their keys. A clip may not key what its `ik` sets.
- **P-A4, as built:** measured on the clip itself against the root's pivot (the feet line before
  `Look.sole`'s planting): for `idle`, `land`, `turn` each boot's lowest point within ±6 (worst 2.4),
  for `run` the lower boot (worst 5.97: Paka's run, near the edge). `wall` faces the wall (a slide
  holds toward it, so `face` is the wall side): the most wall-ward point of the palms and boots is
  within ±6 of `x = centre + body_w / 2` (0.2).
- **Acts (rules changes, picture only, no velocity changes):** `apex` is the air with `|vy| ≤ apex_vy`
  between `rise` and `fall` (so a respawn, at rest in the air, starts in `apex`, not `fall`); `land` is
  a landing with no direction held, for `land_ticks`, ended by a direction (landing with one held runs
  at once). A switch keeps `prev_act` and `prev_t` (the old act's `act_t` had it gone on). One test
  literal changed: P-M7's too-early press now lands in `land`, not `idle`. P-M12's final state is
  unchanged. `fighter:state` is unchanged.
- **The blend (decision 6):** every switch between two drawn acts on different clips blends, `land`
  and `turn` included: `w = act_t / 4`, the old clip running on at `prev_t + act_t`, `rot` the short
  way, `show` switching at half, a part one pose lacks at rest in it. Blended frames are posed afresh
  (the sole too) and not cached. The scripted run is in a blend on 165 of its 1 200 ticks.
- **Paka's `jump` and `fall_swim` left `brawl`** (no act uses them); `Look.CLIPS` is `idle`, `run`.
  `dash` keeps `plant` in the air too.
- **Contact sheets** (`python3 scripts/clip_sheet.py [--n 8] [clip …]`, 8 frames each, feet line and
  wall line drawn) in `shots/brawl/clips/`, each looked at and reworked: `rise` was a two-arms-up
  cheer (now the far arm reaches up, the near one swings back), `dash`'s near arm pointed straight
  back (now down and back), `wall` hid its hand behind the helmet (now it leans back 18° and the far
  hand and boot are on the line; the near arm hangs behind), `jump2` spun about the chassis (now
  95 above the feet, the robot's weight, since the helmet is heavy). Sword sheathed in all (tested).
  In-game: `shots/brawl/step5_{land,jump2,wall}.png`. The sheets show the Brute slot's edge at their
  right (the lab's slots overlap).
- **Pose cost (decision 7)**, release, 900 × 700 offscreen, Iris Xe, `measure_brawl_look.py`, two runs
  each, warm (cache full, no misses):
  | robots | pose miss mean | blend pose mean | app view p50 / p95 | app frame total p50 / p95 |
  |---|---|---|---|---|
  | 2 | 0.33 ms | 0.54 ms | 0.30 / 1.50 ms | 1.67–1.74 / 2.92–3.03 ms |
  | 1 | 0.36–0.40 ms | 0.64–0.67 ms | 0.36–0.37 / 1.09 ms | 2.16–2.20 / 3.03–3.04 ms |
  **The 4 ms gate now passes with two robots** (p50 1.7 ms, against step 4's 4.70). The robots' share
  is the view's: 0.3 ms p50, 1.5 ms p95 (a blend frame poses each robot afresh). A miss is a third of
  step 4's (the robot's clips are small; Paka's carry a key per frame). The rest of the drop is the
  stage, which the map plan changed meanwhile; not measured here. Unexplained: one robot's whole frame
  is 0.5 ms *slower* than two, both runs. L-1's first pose in a debug build is 1.6 ms.
- **Open / unverified:** the feel of the clips in play (the user judges); the piston leg shears the boot
  a little when it stretches and turns (not visible on the sheets); `smoke_mp`, `smoke_world` and
  `smoke_hero` (S-10) were not run (this step touches none of their files).

### Maps step 2 landed 2026-11-11: decision 9 and `stages/yard.lua` are replaced

The yard is now `maps/yard.lua` placing assets from the library ([Brawl maps](brawl-maps.md) step 2).
`stages/yard.lua` is gone; its collision data lives on as `scripts/fixtures/brawl_yard_v1.lua`, which
P-K2 compares the compiled yard to, field by field and in order. `stage_art.lua` is `scene_art.lua`
and draws a view-only `scene`; the stage in the match's setup holds no art, and `fighter.lua` no
longer takes an `art` field (P-B7 now expects `unknown field art`). Every P-B and P-M test runs on the
compiled yard, and P-M12's final state is byte-identical. The test helper's `U.yard` is the compiled
stage. P-C5 (art list against the files) moved: sizes against files are P-L1's, and the coverage
check reads the scene. Decision 9's text above stays as history.

### Smaller robot 2026-11-11: the fighter is drawn and collides at 75%

The user played it: "we need a smaller character". `Look.SCALE = 0.75` (look.lua); main.lua draws
the frame at `k * Look.SCALE`, placed so the planted sole (and the root pivot in the air) lands on the
feet line, and `tune.lua` `body_w` 80 → 60, `body_h` 208 → 156. Speeds, gravity and jump impulses are
unchanged, so the jump is 25% higher *relative to the body*. **P-M12's final state moved**, as it should
(the hitbox touches walls and ledges at different x/y): was `tick=1200 x=-738.5 y=-240.5 vx=-7 vy=3
act=fall act_t=3 …`, now `tick=1200 x=-759.5 y=-226 vx=-7 vy=4 act=fall act_t=7 …` (other fields the
same); `test_brawl_maps.lua` P-K2 and S-9 carry the new value. Tests that pin the size
(`test_brawl_look` measure, `test_brawl_clips` wall line, `smoke_brawl` S-8 rect, feet and sole pixel
checks) take the scale; all Lua tests, `smoke_brawl`, `smoke_brawl_maps` and `smoke_characters` pass,
foot gaps 0-1 px. Twin Isles' tests (bridge, pit, boulder) pass unchanged.
