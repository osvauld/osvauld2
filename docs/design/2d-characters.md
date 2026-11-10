# 2D characters — a bought rig as the answer key, our own beside it

Status: **plan drafted 2026-11-10; steps 1–5 built in the worktree, unreviewed;
`scripts/smoke_characters.py` (S-C1–S-C7) passes on the laptop.** Worktree `.claude/worktrees/2d-characters`,
branch `2d-characters`. Decisions in §1 marked *agreed* were made by the user that day; the rest
were proposed, and the user said to go ahead with the plan as written the same day (a
long-running task, built step by step by subagents, no `pi` reviews).

Why: the 2D world has the netcode of a platform fighter (rollback, a fixed step, sensors) and
none of its characters. Art at that level is bought or commissioned as a **bone rig with
animations**, and ours can't play one back: a pose is `x, y, rot` and one `scale`, easing is
`linear`/`in_out`, a part can't be hidden on a key, and a 2D drawing can't show a PNG. Grows out
of [2D world](2d-world.md) §2, [SVG drawings](svg-drawings.md) (its "not in scope: raster
images") and [Blender import](blender-import.md) (binary item files, the texture budget).

**The one idea:** Rust gains four small, generic things a pose and a drawing can say. Everything
that knows what a character is — the Spine reader, the rig, the clips, the second character —
is a script and Lua data. **A character is Lua files plus its PNG, in the app's own folder**
(*agreed*): every player has the same files, so a match agrees on them without any sync.

```
characters/paka/
  rig.lua          -- size, parts (bones and slots), pivots, draw order, image regions
  clips/run.lua    -- one clip per file, the table gfx.clip takes
  atlas.png        -- binary item file
```

The test rig is **Paka** from Dumivid's *Scavenge Legends — Animated Heroes* (CC0): Spine 3.8.75
JSON, 58 bones, 31 slots, 25 animations, three skins, and every animation pre-rendered at 60
frames a second. Those frames are the answer key (S-C4). Measured from the file: 87 of 257 scale
keys differ in x and y; 2,053 of 3,416 keys are bezier, 8 stepped; attachment keys only show or
hide (blink, sword); 2 IK and 8 transform constraints; no draw-order, colour, deform or event
keys; 23 of 30 meshes are a rectangle bound to one bone (an affine fit of position on UV is
within 0.007 units), 7 are skinned across 3–4 bones; 12 atlas regions are stored turned 90°.

## 0. End-to-end tests (written first, red until their step lands)

`U` = Rust unit tests in `runtime`/`world`; `L` = a real sandboxed Luau VM through `app_host`;
`C` = the converter run on the fixture (`scripts/test_spine_to_lua.py`, stdlib only);
`S` = bridge smoke over an offscreen shell (`scripts/smoke_characters.py`, into `SMOKES` once
green).

### What a pose and a clip can say

| id | proof | step |
|---|---|---|
| U-C1 | a pose's `scale_x`/`scale_y` scale a part along its own axes about its pivot, after its rotation; `scale` still scales both and multiplies them; a child follows, sheared where its parent's two scales differ (plain affine product) | 1 |
| U-C2 | easing `"step"` holds the previous key's value until the key's own time, on any property | 1 |
| U-C3 | easing `{ x1, y1, x2, y2 }` is CSS `cubic-bezier`: sampled values match a table computed independently (±1e-6); `x1`/`x2` outside 0–1 are refused; the solve uses only `+ − × ÷` so two machines agree bit for bit | 1 |
| U-C4 | a `show` track is boolean and always stepped; a hidden part paints nothing and is never hit, and **its children are still placed and drawn** (a bone is not its picture) | 2 |
| U-C5 | a world snapshot taken mid-clip and restored draws the same frame as the world that kept running (poses come from the clock; nothing new is stored) | 2 |
| L-C1 | the surface is strict: `drawing:pose` and clip tracks take `scale_x`, `scale_y`, `show`; a non-boolean `show`, an easing on a `show` key, a bezier with three numbers, and an unknown field are errors naming the clip, part and key | 1–2 |
| L-C2 | `clip:at(t)` reads the new fields back, so a plain `ui.frame` and a world draw the same pose | 2 |
| S-C1 | a square part with a `scale_y` key of 0.5 is half as tall and as wide as before, at the frame `rpc.advance` lands on (pixels and `rpc.rects()`) | 1 |
| S-C2 | a part hidden from 0.20 s to 0.40 s: the pixel under it is the background at 0.25 s and its own colour at 0.15 s and 0.45 s | 2 |

### Pictures in a drawing

| id | proof | step |
|---|---|---|
| L-C3 | a drawing shape `{ image = "characters/x/atlas.png", src = {x, y, w, h}, transform = {…} }` is accepted; a missing file (the error lists the app's `.png`/`.jpg` files), a `src` outside the picture, a shape with both `image` and `path`, and a picture past the texture budget are errors naming the shape | 3 |
| L-C4 | one file named by forty shapes is decoded once and counted once against the budget; a failed reload leaves the running drawing's pictures alive | 3 |
| S-C3 | a four-colour test PNG: one quadrant drawn turned 90° and scaled ×3 shows only its own colour edge to edge (no neighbour bleeding in), at the place `rpc.rects()` reports; the part answers `on_hover` over its rectangle; the dump lists the picture's path, `src` and bytes | 3 |
| S-C5 | `gfx.image` on a plain `ui.frame` draws a prop PNG at its natural size (the stage pack's platforms need nothing else) | 3 |

### The rig, through the real path

| id | proof | step |
|---|---|---|
| C-1 | `spine_to_lua.py` on the Paka fixture writes `rig.lua` (58 bone parts with no shapes, 31 slot parts in slot order, each a picture on its bone) and 25 clip files that `gfx.drawing`/`gfx.clip` accept unchanged | 4 |
| C-2 | its report names everything it changed: the 12 bones under an IK or transform constraint are **baked** (sampled at 60 a second), every other key is kept as authored with its curve; the 7 skinned meshes are **drawn rigid** on their heaviest bone; nothing is dropped silently, and an input feature it doesn't know (a deform, colour or draw-order key, a path constraint) is an error, not a guess | 4 |
| C-3 | rotation keys are unwrapped so no part spins the long way round between two keys, and the Y axis is flipped once (Spine is y-up): checked on hand-built two-bone fixtures | 4 |
| S-C4 | **the answer key.** `run`, `attack`, `jump` and `idle` at eight times each, drawn at the pack's 0.2 scale, against the pack's own frame for that time: silhouette overlap (alpha > ½, intersection over union) and mean colour distance inside the overlap, each within a threshold fixed in step 4 from what the rigid-drawn parts cost. The numbers per frame go in the step's note | 4 |
| S-C6 | **hot edit**: `edit_file` on `clips/attack.lua` changing one key (bridge, `read_file_versioned` + exact replace) reloads the app and the sword is elsewhere at that frame | 4 |
| S-C7 | **our own**: a second character, vector shapes only, written by an agent with Paka's part ids, plays Paka's clip files unchanged beside Paka in one world; the dump shows the same pose for the same part in both | 5 |
| S-C8 | the existing drawing, clip, SVG, Hillclimb and Jetpack smokes stay green (new fields default to today's behaviour) | all |

## 1. Decisions

1. **Lua files are the asset** — not a workspace asset document. *Agreed 2026-11-10.* Workspace
   assets don't sync yet and two open copies lose edits; app files are the same for every player.
2. **PC first; each player on their own device; gamepad later.** *Agreed 2026-11-10.* Nothing
   here needs input.
3. **Scale.** `scale_x` and `scale_y` beside `scale`, multiplied: old data means what it meant.
   No `shear`: Paka has none, and a parent's unequal scale already shears its children through
   the affine product, which is how Spine composes a bone in its default mode. *Proposed.*
4. **Hiding.** `show` hides the part's own shapes, not its children: imported bones are parts
   without shapes and their pictures are separate parts, so "hide the sword" must not hide the
   hand. *Proposed.*
5. **Bezier easing in the engine**, not baked. *Proposed — this revises what was said in
   discussion ("bake the curves").* Baking every track at 60 a second turns 3,416 authored keys
   into roughly 100,000 that nobody can read or edit, and the second character and the editor
   both want Paka's real keys. The cost is one small solve, and it is CSS's own easing, so it is
   a generic primitive. Only the 12 constrained bones are baked.
6. **A key's easing still shapes the segment arriving at it.** Spine stores a curve on the key
   that starts the segment; the converter moves it one key along.
7. **Pictures are named by path in drawing data** (`image = "…png"`), keeping drawings pure
   data; `gfx.image` on a frame takes the handle `gfx.load_texture` already returns. One handle
   and one budget for 2D and 3D (64 pictures, 128 MiB, 2048 px a side — Paka's atlas is
   1858 × 910). *Proposed; settled in step 3 on reading how `TextureData` holds level 0.*
8. **A picture is hit as its rectangle**, not by its alpha. *Proposed.* Fighters hit boxes, not
   pixels.
9. **Skinned 2D meshes are not in this plan.** The 7 parts are drawn rigid and judged by eye
   against the pack's frames in step 4. If they are needed, that is its own plan: Vello has no
   textured-triangle primitive we know of (to be confirmed against 0.9), so it is a render pass,
   not a field.
10. **The converter is a Python script** in `scripts/`, run by an agent or a person, never by the
    app. Reading Spine's exported JSON with our own code is not using Spine's runtime; whether
    Spine's terms say anything about that is unchecked and must be read before a game ships with
    a converted rig. The art itself is CC0.
11. **IK is not physics.** The two leg chains are a two-bone triangle solve done in the
    converter. Live IK, if a game wants it, is Lua setting two rotations — no Rapier, no Rust.

## 2. Where the code goes

- `runtime/src/drawing.rs`: `Pose` gains `scale_x`, `scale_y`, `show`; `pose` skips a hidden
  part's shapes. `runtime/src/frame.rs`: an `Image` item (picture, source rectangle, transform),
  painted with the frame's alpha and hit as its rectangle. The runtime still knows nothing of Lua
  or of characters.
- `world/src/clip.rs`: `Prop::{ScaleX, ScaleY, Show}`, `Easing::{Step, Bezier}`.
- `app_host/src/gfx.rs` and `gfx/texture.rs`: the strict parsing, `gfx.image`, pictures in
  drawing shapes read through the binary-file reader, the shared budget, the dump.
- `scripts/spine_to_lua.py` and `scripts/test_spine_to_lua.py`; the Paka JSON, atlas and 32
  reference frames under `scripts/fixtures/paka/` with the pack's licence file.
- `demo_apps/character_lab` (new): both characters side by side, a clip picker, a time readout.
  It is the repro app and what the smoke drives.
- Docs: `lua-apps.md` (Drawings, Clips, Frame visuals), `status.md`.

### Step 1 landed (2026-11-10, in the worktree, unreviewed)

- As planned: `Pose { scale_x, scale_y }`, `Prop::{ScaleX, ScaleY}`, `Easing::{Step, Bezier}`.
  The bezier is a fixed 60-step bisection on the curve's time, then its value: only `+ − × ÷`
  and comparisons. The test pins three results to the bit; the bits were computed by the same
  algorithm in Python and matched the Rust run on the mac peer.
- `Easing::bezier` refuses `x1`/`x2` outside 0–1 or a non-finite `y`; `Clip::new` refuses a
  hand-built bad `Bezier` too (`ClipError::KeyEasing`).
- Clips have no name, so errors name the track and key (`clip track arm.scale_y key 2: …`), as
  the existing clip errors do. `drawing:pose` errors still say only `pose:` (an existing test
  pins that text).
- S-C1's app is `demo_apps/clip_scale` (a world entity with a `scale_y` step key and a plain
  frame posed with `scale_x`). The smoke and the S-C8 sweep were not run: no Linux build machine
  was reachable (nixos offline; the mac can't build `webrtc-audio-processing-sys`).

### Step 2 landed (2026-11-10, in the worktree, unreviewed)

- `Pose.show: bool`; `Drawing::pose` skips a hidden part's group (so it is neither painted nor
  hit) while `world` still composes its transform for children and `part_at`.
- Representation: a boolean is not an `f64`, so `show` is its own track kind: `ShowTrack { part,
  keys: Vec<ShowKey { time, show }> }` held beside `tracks`, attached with `Clip::with_shows`
  (`Clip::new` is unchanged). `Clip::parts()` includes show-track parts. Before the first key a
  show track holds that key's value, as `step` and every other track do.
- Errors: Lua names the track and key (`clip track slot:sword.show key 2: show must be a boolean,
  got number`; an easing on a show key is `a show key is {time, boolean} and always steps`);
  `drawing:pose` still says only `pose:`.
- U-C5 needed nothing stored: the animator's start and the clock are not in the snapshot, and the
  test proves restore + `show_at` redraws the straight run's frames and digest.
- S-C2's app is `demo_apps/clip_show` (a hidden square with a child that stays drawn); the smoke
  is `show_main` in `scripts/smoke_characters.py`. Not run: nixos still offline.

### Step 3 landed (2026-11-10, in the worktree, unreviewed)

- **The item.** `runtime::frame::Picture` (shared pixels, a `src` region, a caller's name for the
  dump) and `Item::image(picture, transform)`: drawn with Vello 0.9's `Scene::draw_image` (a
  rectangle filled with an image brush) under the frame's transform and alpha (`ImageSampler`
  alpha); hit as its rectangle through the inverse transform (decision 8, as proposed). The
  runtime still knows nothing of files or Lua.
- **No bleeding.** Vello clamps every sample to the image's own extent in its atlas, and an
  `ImageData` has no offset or stride, so a sub-rectangle can't be a view of the file's pixels. A
  `src` region is therefore **copied into its own image**; drawing the whole atlas clipped to the
  region was rejected because bilinear sampling at the clip edge reads the neighbour. The GPU
  test `a_turned_region_shows_no_neighbour` (ignored without an adapter; passed on the laptop)
  proves both: the copied region turned 90° and ×3 is pure green to its edge pixels, and the
  clipped atlas is not.
- **Decision 7, settled:** one handle and one budget. `TextureData` keeps its mip chain as
  `peniko::Blob`s, so a picture of the **whole** file draws level 0 itself — the same bytes the
  3D upload reads, one Blob id, so Vello's image cache uploads it once. A region costs a copy of
  its own pixels, counted against the same 128 MiB (not the 64). A drawing's `image` path is read
  through the binary-file reader and decoded once per VM while anything holds it (a weak cache by
  path; paths are pinned to the VM's revision), and pictures are cached by (texture, `src`), so
  forty shapes on two regions of one file are one decode, one texture and two copies. A failed
  reload builds and drops a separate VM, so the running drawing's pictures (plain `Arc`s) stay.
- **The shape contract** is the converter's, honoured exactly: `transform` is six numbers in
  kurbo order, the same form `gfx.group`/`gfx.instance` already take; path shapes have no
  `transform`, so there is one form. `gfx.image({ texture, src, transform, id })` is the plain
  frame's item. The dump puts `pictures: [{ path, src, bytes, shared }]` on a `frame` node
  (`bytes` drawn; `shared` when it is the texture's own pixels). World entities' pictures are not
  dumped (their drawings aren't either).
- **Converter fixture:** one shape copied verbatim from `spine_to_lua.py`'s Paka output (the
  sword) loads over the pack's own atlas PNG and is hit where its transform puts it. The full
  `rig.lua` loads unchanged once `characters/paka/atlas.png` is a binary file of the app and the
  rig a source file; it was not committed here (it is generated).
- **Errors** name the shape: `drawing part 3 shape 1.image: no characters/x/atlas.png in this app.
  It has: …` (the app's `.png`/`.jpg`/`.jpeg`), `….src: picture src {…} is empty or outside the
  WxH picture`, `…: a shape has image or path, not both`, `….image: <path>: textures exceed the
  VM's 64-texture or 128 MiB budget (holding N, M MiB)`.
- **Decision 9, a finding (untested):** Vello 0.9 has no textured-triangle call, but `Scene::fill`
  takes any path with an image brush and a `brush_transform`, so a triangle filled with its own
  affine map of the atlas is expressible without a render pass; seams between antialiased
  triangles are the open question.
- S-C3/S-C5's app is `demo_apps/clip_picture` (the smoke writes its two PNGs with stdlib Python);
  the smoke is `picture_main` in `scripts/smoke_characters.py`. An app_host test opens that app
  through `LuaApp` and checks its pictures. Not run: nixos offline, so shell2 (and its dump
  change) is unbuilt.

### Step 4 landed (2026-11-10, in the worktree, unreviewed)

- **Generated:** `spine_to_lua.py` on the fixture, skin `classic`, `--scale 0.2` (one unit is a
  pixel of the pack's frames; box 396 × 239), into `demo_apps/character_lab/characters/paka/`.
  Looped: the 14 clips whose last pose is their first (idle, run, walk, talk, fall_swim and their
  variants); attack, interact, jump, defeat and blink are once. 1.2 MB of clip Lua, 13 KB rig,
  141 KB atlas. The converter was not changed.
- **The app** is a list of characters `{ rig, clips }` by path, all on one clip name. Paused, each
  is a plain frame posed from `clip:at(t)` (exact, which S-C4 needs); playing, a world plays the
  clips. Clips load on first pick. Its state is `ui.state` read in `view`: a reload carries
  `ui.state` over *after* the new module has run, so a table taken at module scope is a fresh one
  (S-C6 caught it; the guide does not say so).
- **C-1, engine half** (`app_host` `tests/characters.rs`): the app through `LuaApp` with its
  files; rig and 26 clip files unchanged; 225 poses; 42 points (head, both hooves, the sword's
  far corner at 3 times in `run` and `attack`) within 6.3e-7 of `test_spine_to_lua.py`'s
  evaluator, whose bone origins match Spine's solve. Debug, laptop: open (rig, atlas, one clip)
  0.39–0.51 s, the other 24 clips 0.32–0.41 s and +7.3–7.8 MiB of Lua heap.
- **S-C4:** the fixture's 32 frames against the shell's pixels. Alignment is fitted per animation
  on its first frame (silhouette box centres, then the best IoU within 4 px) and held for its 8;
  the fitted canvas origins lie within ~8 px of the converter's `reach`. IoU 0.971–0.991, mean RGB
  distance over the overlap 3.4–9.2; thresholds set to 0.96 and 12. The worst frames (attack
  15–20) are the rigid-drawn hair crest and ears; the tie's shading differs a little too.
  `scripts/preview_character.py` composites the same thing with ImageMagick, engine-free (0.971–
  0.990, 7.9–9.8).
- **S-C6:** `sword_forward`'s rot key at 0.3333 s turned 90° over the bridge; at 8/60 s the sword
  leaves its old place (stage colour there, not hit) and is hit where the evaluator puts it.
- Picking needs the point inside the frame's box: a posed part reaching outside its drawing's box
  is painted but not hovered.

### Step 5 landed (2026-11-10, in the worktree, unreviewed)

- **Generic robots, not Paka in metal.** `characters/robot/build.lua` returns a drawing module
  from a variant `{ body, trim, accent, visor?, ink?, head = "antenna" | "fins" | "dome" }`; three
  one-line variants (`scout`, `brute`, `volt`). The skeleton is **derived from Paka's rig data**:
  the builder `require`s `characters/paka/rig` and copies every part's id, parent, pivot and
  order, replacing only the slot parts' shapes. Nothing of Paka's pictures is used. Slots with no
  robot part (`tie`, `head_nose`, `path_death`, and the crest or ears where a head kind has none)
  keep the part with no shapes.
- **Drawn in the setup pose.** The rest pose has every bone unrotated, so a head drawn there lies
  on its side. Pieces are authored where they stand on screen in `setup.lua`'s pose and mapped
  into rest space through each slot's inverse setup transform, composed in Lua the way the engine
  composes a pose (rigid, so curves stay exact). This is the trick that made the art tractable.
- **Rigid is fine.** The 7 parts Paka skins are a robot's rigid plates here. Limbs are capsules
  whose round ends are centred on the joint pivot, so no gap opens at any angle in `run`,
  `attack` or `jump` (checked by eye on a sheet of 18 times).
- **Style:** two-tone plating and dark trim, each a shadow, base and highlight from one colour
  (light from the top left, the lit part pulled toward it, outline last); one accent for the lens
  eyes, chest light, mouth and the energy blade (radial gradients, `#rrggbbaa` glow). Blink
  shows shutters over the lenses; the back sword is a sheathed hilt, the in-hand one a blade.
- **The lab:** robots get 226-unit slots and `offset` their frame 84 units left (the drawing
  box's left 100 units are empty); their frames keep the drawing's full width, since the column
  stretches a leaf to its slot and only a point inside the box is picked. Paka's slot is
  unchanged, so S-C4's numbers are the same as step 4's. Hover readout is per frame (overlapping
  boxes no longer clear each other).
- **S-C7:** the robot sources name no `image`; their frames dump no pictures; at `run` 0.1/0.4
  and `attack` 8/60, a point of the robot's head, body, front boot or blade (taken in the setup
  pose, posed by the converter test's evaluator) reports the same part under the pointer on Paka
  and on every robot whose point is on stage, the robot's pixel there is not the stage; the
  boot's 0.1 s place is not the boot at 0.4 s; playing, one world holds all four on one clip
  clock with an empty console (a track missing its part would say so). `app_host`
  `the_robots_play_pakas_clips_as_vector_shapes`: same parts as Paka, vector shapes only, Paka's
  own clip handles, 375 poses, and every part's transform equal to Paka's at 4 times of `run`,
  `attack`, `jump`.
- **Gaps met:** no clip paths, so shading is built from shapes inset inside their outline
  rather than clipped to it; no blur except rounded-rect shadows, so a glow is a translucent
  shape (it reads teal over the cyan stage). Showcase in `shots/character_lab/robot*.png`.

## 3. Steps

1. **Scale and easing** (U-C1–C3, L-C1, S-C1): the two scales, `step`, bezier.
2. **Show** (U-C4, U-C5, L-C1, L-C2, S-C2).
3. **Pictures** (L-C3, L-C4, S-C3, S-C5): the frame item, the drawing shape, the budget.
4. **Paka** (C-1–C-3, S-C4, S-C6): the converter, `character_lab`, the frame comparison;
   screenshots and the per-frame numbers to the user, who judges the rigid parts.
5. **Our own** (S-C7): an agent writes a vector character on Paka's skeleton and it plays Paka's
   clips. The user judges how it looks beside Paka.

After this plan, each its own: the asset editor as a character viewer (open by path, a timeline
to scrub, a reference frame under the pose), then hand editing (which needs an app to write Lua
source — unchecked); clip events and boxes that ride parts (hitboxes); opacity, tint and additive
blending for effects; sound files; a fighter's missing moves, which Paka's pack doesn't have (one
attack, no hurt, dash or aerial).

Not in scope: skinned meshes, draw-order and colour keys (Paka has none), Spine binary `.skel`,
DragonBones, mesh deformation, gamepads.
