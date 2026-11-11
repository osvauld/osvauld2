# Brawl maps: an asset library first, then maps built from it

Status: **steps 1–4 built (2026-11-11, branch `2d-characters`, not merged): the library, P-L1–L3 and L-3 (revised), the yard on the library (P-K1–K5, L-1 revised, S-M1), then the full library, the sheet and the overlay (P-L3 full size, P-V4, S-M2, S-M3), then backgrounds that read (P-V1–V3, S-M6), then Twin Isles, map switching and live edit (P-K6, P-K7, L-2, S-M4, S-M5); all five steps built.** The user answered the §4 questions on 2026-11-10 (Twin Isles, the sheet inside Brawl, the `fore` layer, soft SVG clouds); the other decisions in §1 are *Proposed* and are being built as written. It follows [Brawl movement](brawl-movement.md): steps 1–3 there are built, and this plan replaces its decision 9 (stage art) and the `art` list in `stages/yard.lua`. **No Rust.** Everything here is Lua data, Lua modules, PNGs and test scripts.

Why: after playing, the user said *"backgrounds need some work. need a detailed plan on the map as well, make map assets first then the map that uses those assets rather than one map file."* Today one file, `stages/yard.lua`, holds the collision rectangles and a separate `art` list. Nothing ties a platform's picture to its collision rectangle. Pictures are named by file path inside the map. The back layers run out at the stage edge (`shots/brawl/step3_edge.png`: the hedge ends in a green wall over pale sky). The clouds are flat white blobs. The mountains are as sharp and saturated as the ground, so there is no depth.

**The one idea:** an **asset** is a named, reusable piece of map: its picture(s), size, anchor, draw layer and, optionally, a collision template. Assets are defined once in `maps/assets/*.lua`. A **map** only places assets by name. `map.lua` compiles a map into two things: the plain `stage` that `fighter.lua` already checks (`{name, solids, platforms, spawns, respawn, blast, camera}`), and a view-only `scene`. Collision and art come from the same placement, so they can't drift apart.

```
library (maps/assets/*.lua) ──┐
                              ├─ map.lua: check + compile ─┬─ stage  → net.match setup → fighter.lua (rules)
map (maps/yard.lua) ──────────┘                            └─ scene  → scene_art.lua (pictures) + depth.lua (placement)
```

## 0. End-to-end tests (written first, red until their step lands)

Test kinds:
- `P` is pure Lua under stock `lua5.4`, run by `scripts/run_brawl_lua_tests.sh` like the existing `test_brawl_*.lua`.
- `L` is a real Luau VM through `app_host/tests/brawl.rs`.
- `S` is a bridge smoke over an offscreen shell, in a new `scripts/smoke_brawl_maps.py` that joins `SMOKES` once green.
- Screenshots go to `shots/brawl/` for the user.

### The library (`library.lua`, `maps/assets/*.lua`)

| id | proof | step |
|---|---|---|
| P-L1 | every asset passes `Library.check`. Its `kind` is known and it has the fields that kind needs. Each `image` is under `art/` with `size` equal to the PNG header (read as P-C5 does today). `src` and `anchor` lie inside the picture. A collision box has `w, h > 0` and lies inside the picture. The default `layer` is allowed for the kind, and `opacity` is in 0..1. *(Added 2026-11-11, after movement step 4's 6 px platform gap:)* a ground or platform asset's `lift` equals its picture's first opaque row, within 1 px, measured with ImageMagick (skipped, and said so, where `magick` is missing) | 1 |
| P-L2 | bad assets are refused with an error naming the family, the asset and the field: an unknown field, an unknown kind, a wrong `size`, an anchor outside the picture, a platform template with an `h`, or one name defined in two families | 1 |
| P-L3 | the library is closed and fits. Every PNG in `art/` is named by some asset, and every named file exists. `art/License.txt` is present and names Dumivid and CC0. The whole library is at most 56 textures and at most 64 MiB, estimated as `w·h·4·4/3` plus copied `src` bytes. The count and MiB are printed for the step note | 1, 3 |
| P-V4 | the asset sheet layout (`Library.sheet`) places every asset exactly once, sorted by family then name. Cells don't overlap, and every asset fits its cell at scale ≤ 1 | 3 |

### Maps (`map.lua`, `maps/*.lua`)

| id | proof | step |
|---|---|---|
| P-K1 | maps only reference assets. `image`, `size`, `src` or any other unknown field in a map is an error. So are an unknown asset name (the error names the piece's index and id), a layer the asset's kind can't use, and a collision-bearing piece with no `id` | 2 |
| P-K2 | **the yard is unchanged for the rules.** `Map.compile(maps/yard, lib).stage` equals the old `stages/yard.lua` (kept as `scripts/fixtures/brawl_yard_v1.lua`) field by field and **in order**. Order matters because a fighter's `on` is a platform index held in state. Every P-B and P-M test runs on the compiled yard, and P-M12's printed final state is byte-identical to the one printed before this plan | 2 |
| P-K3 | collision comes from templates. A ground piece gives the solid `{x, y, w, h}`, and a platform piece gives `{x, y, w}`. A prop with `collide.solid` at `(x, y, scale, flip)` gives its box exactly, mirrored about the anchor when flipped. A derived box off the 1/8 grid is refused, naming the piece. A spawn inside a derived solid is refused (by `fighter.check_stage`, through `compile`) | 2 |
| P-K4 | **art and collision cannot drift.** Shifting a collision-bearing piece by 64 units in a copy of a map moves its solid or platform and its scene item by exactly 64. For every collision-bearing piece of every map, the drawn surface top is the collision top minus the asset's `lift × scale` | 2 |
| P-K5 | the stage handed to `net.match` holds no art. Its keys are exactly fighter's stage shape, and `fighter.lua` no longer accepts the optional `art` field | 2 |
| P-K6 | **two maps from one library.** `yard` and `isles` both compile and check. They share at least 6 assets, and each uses at least 3 that the other doesn't | 5 |
| P-K7 | **isles plays.** A fighter spawns standing on the left island. Dropping through the bridge into the pit loses a stock. The respawn lands on the top platform (`y == −420`). On an island's inner wall the fighter can wall slide and wall jump. The boulder stops a run (`vx == 0` at its side), and the fighter can stand on it | 5 |

### Depth and backgrounds (`depth.lua`, pure)

| id | proof | step |
|---|---|---|
| P-V1 | **depth zoom placement.** `Depth.place(cam, f, home, x, y, k1)` with `f = 1` equals the stage's own mapping exactly. `f = 0` doesn't depend on the camera. Between them, offset and scale are monotone in `f` | 4 |
| P-V2 | **no layer shows its edge.** Test every map with cameras at the 4 corners of its bounds, at `zoom_min` and `zoom_max`, at aspects 900/600 and 16/9. Every backdrop layer covers the view's width, every layer with a `body` reaches the view's bottom, and the sky fills the view. This is the step-3 bug, as a test | 4 |
| P-V3 | cloud drift is a pure function of render time. It moves at the placement's `drift` speed and wraps inside its `span` without a jump. The same total time gives the same x whatever the frame steps | 4 |

### The app

| id | proof | step |
|---|---|---|
| L-1 (revised) | the default map opens with a clean console. The PNGs in the view are **exactly those its assets name**, no longer "every PNG in `art/`". It stays within budget | 2 |
| L-2 | each map in turn: `maps/index.lua` is reordered in the source doc before opening so that map comes first. The console is clean, and the map uses at most 32 textures and at most 48 MiB with mips and copies. Counts are printed per map | 5 |
| L-3 | a broken asset (the `size` of `tree_2` edited in the source doc before opening) puts `Library.check`'s error, naming `props.tree_2.size`, on the console *(revised 2026-11-11: in the hidden `library:problem` readout, and printed; an app's `print` never reaches the console, step 1 note)*. The app still opens and draws the map with plain rectangles (`plain`, as today) | 1 |
| S-M1 | upload and open: the console is clean, `map:state` reads `yard`, `art:loaded` reaches n/n, and the fighter stands at the spawn. Screenshot `maps-yard.png` | 2 |
| S-M2 | **the asset sheet.** Clicking `assets` (position from `rects()`) makes `gallery:state` read `n/n` loaded, with `n` = the library size. The dump lists at most 56 textures. Screenshot `assets.png`: the contact sheet the user judges | 3 |
| S-M3 | **collision overlay.** Toggling `overlay` draws `col:<id>` for every solid and platform. Each one's `rects()` box matches its piece frame `piece:<id>` within 1 px. Screenshot `maps-yard-overlay.png` | 3 |
| S-M4 | **map switch.** `map:next` makes `map:state` read `isles`, puts the fighter at isles' spawn and keeps the console clean. Switching 4 times keeps the dump's texture count at or below the library's (56) and leaves the console clean | 5 |
| S-M5 | **live edit, hillclimb style.** `read_file_versioned` plus one `edit_file` moves yard's `right` platform by +64 x. After the reload, the fighter can jump onto it at the new place (`on` is set), and `piece:right` moved by `64·k` px. A broken edit (asset `"wood_ledgee"`) puts the error, naming the piece, on the console. Fixing it brings a clean console back | 5 |
| S-M6 | **backgrounds at the edges.** The fighter is driven to the left and right stage edges and to the top platform. The bottom row of the view is never the sky's top colour, so no sky shows below the stage. Screenshots `maps-yard-{idle,edge,top,wide}.png` (wide uses the `pair` button for two fighters) | 4 |
| S-10 | `smoke_brawl.py`, `smoke_characters`, Jetpack's and Hillclimb's smokes stay green. S-9's final state is unchanged | all |

**Checked by eye, not by test:** the asset sheet, and whether the backgrounds read well at idle, at the edge, zoomed out and on isles. The user judges the screenshots and then plays.

## 1. Decisions (all *Proposed*)

1. **Two layers of files: a library, then maps.** Assets live in `maps/assets/<family>.lua`, one file per family: `sky`, `backdrop`, `ground`, `platforms`, `props`. Each file returns a table of `name = asset`. Maps are `maps/<id>.lua`, and `maps/index.lua` lists them (`return { "yard", "isles" }`). A Lua app can't list its own files; Hillclimb has the same `MAPS` list. *Rejected:* one file per asset. That means about 50 tiny files plus a hand-kept index, and it's harder for an agent to see a family at a glance. *Rejected:* keeping art inside the map file, as today. That is the thing the user asked to remove.

2. **Asset kinds.** Every asset has `kind`. Pictures are given as `image` plus `size` (checked against the file). Distances inside an asset are **picture pixels**. A placement's `scale` turns them into world units.

   | kind | what it is | collision |
   |---|---|---|
   | `ground` | a stretchable solid block: a `top` strip tiled along it, optional `left`/`right` caps, a `body` colour below, `lift` | always the placement's `{x, y, w, h}` |
   | `platform` | a stretchable one-way ledge: ends kept and middle tiled (`slice = {l, r}` columns, 3-slice through small `src` regions), `lift` | always `{x, y, w}` |
   | `prop` | one picture with an `anchor`, default `scale` and default `layer` | optional `collide = { solid = {x, y, w, h} }` or `{ platform = {x, y, w} }`, relative to the anchor |
   | `layer` | a wide backdrop picture: tiled across, `body` colour continuing it downward, default depth | none |
   | `sky` | vector: gradient stops, optional sun glow (radial) | none |
   | `haze` | vector: a band of colour whose alpha goes from `from` to `to` over `height`, then stays at `to` down to the view's bottom | none |
   | `ridge` | vector: a hill silhouette (Jetpack `scenery.lua`'s `hills`, copied) in one colour, for depth the pack lacks | none |

   ```lua
   -- maps/assets/props.lua
   return {
   	tree = { kind = "prop", image = "art/tree.png", size = { 440, 603 }, anchor = { 222, 598 }, scale = 0.95, layer = "stage" },
   	boulder = { kind = "prop", image = "art/boulder.png", size = { 234, 234 }, anchor = { 117, 230 }, scale = 0.75,
   		collide = { solid = { -100, -200, 200, 200 } } }, -- picture px from the anchor; × 0.75 = a 150 × 150 box
   	grass_3 = { kind = "prop", image = "art/grass_3.png", size = { 116, 119 }, anchor = { 58, 111 }, layer = "front" },
   	cloud = { kind = "prop", image = "art/cloud.png", size = { 500, 167 }, anchor = { 250, 83 }, layer = "far", opacity = 0.8 },
   }
   -- maps/assets/ground.lua
   return {
   	grass_ground = { kind = "ground", top = { image = "art/ground_top.png", size = { 128, 97 } }, lift = 3,
   		left = { image = "art/ground_left.png", size = { 64, 128 } }, right = { image = "art/ground_right.png", size = { 64, 128 } },
   		body = "#463c56", edge = "#3a3149" }, -- edge: the vector cap used when a side has no picture
   }
   -- maps/assets/platforms.lua
   return {
   	wood_ledge = { kind = "platform", image = "art/platform_4.png", size = { 256, 79 }, lift = 4, slice = { 40, 216 } },
   	bridge = { kind = "platform", image = "art/bridge.png", size = { 467, 180 }, lift = 20, slice = { 60, 407 } },
   }
   -- maps/assets/backdrop.lua
   return {
   	mountain_far = { kind = "layer", image = "art/background_mountain_1.png", size = { 1025, 405 }, body = "#0d949b", layer = "far" },
   	hills_light = { kind = "layer", image = "art/foreground_basic_3.png", size = { 1025, 300 }, body = "#3e933b", layer = "mid" },
   	mist = { kind = "layer", image = "art/background_fog_2.png", size = { 1025, 250 }, body = "#ffffff", layer = "mid", opacity = 0.35 },
   	haze_day = { kind = "haze", color = "#d6ecec", from = 0, to = 0.55, height = 320 },
   	ridge_pale = { kind = "ridge", color = "#9cc7d3", h = 260, step = 140 },
   }
   -- maps/assets/sky.lua
   return {
   	day = { kind = "sky", stops = { { 0, "#4fa3cc" }, { 0.6, "#bfe3e6" }, { 1, "#eef5df" } }, sun = { x = 0.8, y = 0.16, r = 160, color = "#fff4c8" } },
   	golden = { kind = "sky", stops = { { 0, "#3d5a8a" }, { 0.55, "#e9a77a" }, { 1, "#f8deb0" } }, sun = { x = 0.25, y = 0.5, r = 220, color = "#ffe1a8" } },
   }
   ```
   The anchors, `slice` columns, `lift` values and crop sizes above are examples. The real ones are measured in step 1 from each picture's opaque box (`scripts/brawl_pack.py` prints it). The default anchor is the bottom centre of that box.

3. **The map format is plain data that only places assets.** It contains no `require` and no computed values, so an editor can round-trip it. *Rejected:* Hillclimb's "a map file can compute" (its decision 4). It suits seeded courses but blocks an editor.
   ```lua
   -- maps/yard.lua
   return {
   	name = "Yard", sky = "day", home = { x = 0, y = -100 },
   	back = { -- back to front; tiles are computed (decision 7), never written
   		{ asset = "ridge_pale", y = -560 },
   		{ asset = "cloud", x = -300, y = -640, drift = 6, span = 3000 },
   		{ asset = "mountain_far", y = -460, scale = 1.8 },
   		{ asset = "haze_day", y = -420 },
   		{ asset = "mountain_near", y = -250, scale = 1.6 },
   		{ asset = "mist", y = -230, scale = 1.4 },
   		{ asset = "hills_light", y = -260, scale = 1.2 },
   		{ asset = "haze_day", y = -120, opacity = 0.5 },
   		{ asset = "hedge", y = -240, scale = 1.1 },
   		{ asset = "haze_abyss", y = 60 }, -- mist below the stage's top, so past the edge reads as depth, not sky
   	},
   	pieces = { -- the map plane, in draw order within each layer
   		{ asset = "grass_ground", id = "ground", x = -900, y = 0, w = 1800, h = 500 },
   		{ asset = "wood_ledge", id = "left", x = -620, y = -260, w = 320 },
   		{ asset = "wood_ledge", id = "right", x = 300, y = -260, w = 320 },
   		{ asset = "wood_ledge", id = "top", x = -160, y = -520, w = 320 },
   		{ asset = "tree", x = -790, y = 0 },
   		{ asset = "tree_2", x = 770, y = 0, scale = 0.9 },
   		{ asset = "bush", x = -330, y = 0, scale = 0.8 },
   		{ asset = "rock", x = 400, y = 0, flip = true },
   		{ asset = "grass_3", x = -200, y = 8, scale = 0.9 },
   		{ asset = "grass_3", x = 540, y = 8, scale = 0.8, flip = true },
   	},
   	spawns = { { x = 0, y = 0 } }, respawn = { x = 0, y = -700 },
   	blast = { x = -2100, y = -1700, w = 4200, h = 2800 }, camera = { x = -1500, y = -1100, w = 3000, h = 1800 },
   }
   ```
   A piece may set `x, y, w, h` (ground), `w` (platform), and `scale, flip, layer, f, opacity` (where the kind allows them). Pieces that produce collision need an `id`. Those ids become the solid and platform ids, and errors name them. `shapes = { solids = {…}, platforms = {…} }` is allowed for invisible collision (kill walls, say). The overlay (S-M3) shows it, because invisible collision is the one place art and collision may disagree on purpose.

4. **Collision is derived, exactly.** `Map.compile` turns ground and platform placements into their rectangles, and turns prop templates into `anchor-relative px × scale`, mirrored about the anchor when `flip` is set. Results must lie on the 1/8 grid. Tunables are dyadic, so `fighter.lua`'s `==` tests stay exact. A box off the grid is an error saying which piece and scale. The order is the pieces' order, because `on` in state is a platform index. Solids that touch are kept apart, not merged. `fighter.lua`'s wall test is strict (`f.y > s.y`), so a fighter walking across a seam with equal tops isn't stopped. P-K7 walks one. Slopes are out of scope; they wait for the geometry-module plan (movement §1a).

5. **The stage in `setup` carries rules only.** `compile` returns `{ stage, scene }`. `net.match` gets `setup.stage = stage`, which is small and travels to others. With others, the starter's map wins. The `scene` never leaves the view. `fighter.lua` drops its `OPTIONAL = { stage = { art = … } }`.

6. **Layers and depth.** The draw order is fixed by layer, and by list order inside a layer:

   | layer | default `f` | allowed `f` | relative to the fighter |
   |---|---|---|---|
   | `sky` | 0 (vertical 0.05) | — | behind |
   | `far` | 0.1 | 0.02–0.2 | behind |
   | `mid` | 0.3 | 0.2–0.5 | behind |
   | `near` | 0.6 | 0.5–0.85 | behind |
   | `stage` | 1 | 1 | behind (ground, platforms, props) |
   | `front` | 1 | 1 | in front (grass tufts) |
   | `fore` | 1.25 | 1.1–1.5 | in front, only near the view's bottom corners (open question 3) |

   **Depth zoom** (new, `depth.lua`): a layer's px per unit is `k1 · (1 + (zoom − 1)·f)`, with `k1 = W / ref_w`. Its position is `centre + ((x − home.x) − (cam_c − home.x)·f) · that`. Today's far layers zoom out as much as the ground does, which shows their edges and makes them look near. With this, the mountains barely move or shrink while the stage pulls back. P-V1 pins it, and `Camera.parallax` becomes a special case of it.

7. **No layer ever shows an edge.** `compile` computes each `layer` asset's tile count from the map's camera bounds and zoom range, using the P-V2 cases. A `body` colour continues every silhouette layer down to the blast zone's bottom. Below the stage's top, a `haze_abyss` band (vector, from transparent to `#e4efec`) turns "past the edge" into mist, where today it shows sky. Seam overlap stays as step 3 found it: 4 px between copies of a tiled layer, and 1.5 units between ground tiles.

8. **The background recipe uses the pack honestly, with vector where it falls short.** What the pack has, measured from the zip:
   - `background_mountain (1)` (1025×405) and `(2)` (1025×241): teal silhouettes, mean colour about (61, 169, 175).
   - `foreground_basic (1)/(2)/(3)` (1025×593): greens from dark to light (centre colours #337830, #44a140, #3e933b).
   - `background_fog (1)/(2)/(3)`: **pure white silhouettes with the exact alpha footprint of `foreground_basic (1)/(2)/(3)`** (mean alpha 0.7296/0.5072/0.6115 against 0.7299/0.5071/0.6111). They are masks meant to be drawn translucent, not mist pictures. Drawn as they are, they read as a white wall, as step 3 found.
   - `foreground_lineart (1)–(3)`: outlined versions, which don't match the flat props, so they are unused.
   - 4 clouds: flat white.

   The recipe, back to front:
   - a **vector sky** with 3 stops and a radial sun glow;
   - a **vector `ridge`** in pale blue (the pack has nothing that far away);
   - far clouds, drifting 4–10 u/s at opacity 0.7–0.85;
   - `background_mountain (1)`, then a **haze** band (translucent sky colour; this is the depth cue, since the engine has no picture tint);
   - `background_mountain (2)`, then **`background_fog (2)` at element `opacity` 0.35** (a soft mist bank);
   - `foreground_basic (3)`, then a lighter haze;
   - `foreground_basic (1)` (the hedge) at `near`;
   - the stage;
   - grass at `front`.

   Each haze band lowers contrast toward the sky colour, so the layers step back in depth. Isles uses the `golden` sky and a warm haze (`#f4c99a`) over the same greens, so its backdrop is colour-graded through overlays, not through new pictures. The clouds stay flat pack clouds with opacity and size telling far from near (open question 4).

   *Unverified, checked in step 4:* that `opacity` on a `ui.frame` dims a picture. Alpha colours already work (`#rrggbbaa` in Jetpack). Fallback without Rust: `brawl_pack.py` bakes the alpha into the file (fog × 0.35).
   *(2026-11-11, step 4: verified, it dims a picture, so nothing is baked. It applies item by item, not to the element as one layer, so overlapping tiles of a translucent layer show a lighter stripe; such a layer overlaps its copies by 1 px, not 4. See the step 4 note.)*

9. **Ground and platform edges.** `_tilemap_main.png` is laid out on a 128 px grid. Row 0 has separate cells at x 384, 512, 640 and 768, and column x 896 is the ground that already tiles (step 3). Step 1 catalogues the grid on a contact sheet (each cell labelled col,row with its opaque coverage) and crops the cap cells into `ground_left.png` and `ground_right.png`. If no cell joins cleanly, the cap is vector: the `edge` colour band plus the grass strip rounded over the corner. Platforms are 3-sliced, so a wide ledge keeps its ends instead of stretching. Today a 320-wide ledge squashes a 256 px picture.

10. **The art files, and where they come from.** The 128 px set. File names are the pack's name without `_recommended`, with `(n)` written as `_n` and spaces as `_`, all lower case. So `tree_recommended.png` becomes `tree.png`, and `Depth Elements/background_fog (2).png` becomes `background_fog_2.png`. Today's files are renamed to match: `tree_1.png` → `tree.png`, `cloud_1.png` → `cloud.png`, `grass.png` → `grass_3.png`, `mountain_1.png` → `background_mountain_1.png`, `hedge_1.png` → `foreground_basic_1.png`, `ground_strip.png` → `ground_top.png`. An asset's name defaults to its file stem, with a suffix for a second use of the same file (`cloud_far`).
    - `art/SOURCES.txt` has one line per file: `out ← pack path [crop WxH+X+Y] [alpha a]`.
    - `scripts/brawl_pack.py <zip>` rebuilds `art/` from it with ImageMagick and prints sizes and opaque boxes. The zip stays outside the repo, and `art/License.txt` stays beside the PNGs.
    - **Large layers are cropped in the file, not by `src`.** That saves the copied-region bytes (3.36 MiB today) and the decode time. `src` is kept for small regions under 64 000 px, which means only the platform slices.

11. **Budgets.**
    - **Per map:** at most 32 textures and 48 MiB (mips plus copies). The yard's estimate is about 20 textures and about 17 MiB, 3.3 MP to decode, which is about 9 frames at today's 400 000 px a frame in a debug build.
    - **The whole library:** at most 56 textures and 64 MiB. The plan is about 48 textures, 6.5 MP, about 33 MiB. That lets the sheet load everything in one VM, and it covers map switching even if old textures aren't freed when collected (S-M4 measures this). It also leaves 8 textures for effects and the HUD later. The robots use none.
    - The library's planned contents, all 128 px: depth elements (mountain 1–2, `foreground_basic` 1–3, fog 1–3), clouds ×4, `tree`, `tree_1`, `tree_2`, `bush`, `bush_1`–`bush_3`, rocks ×3, boulders ×4 (`boulder`, `_2`, `_3`, `_5`), grass ×6, `platform` and `platform_1`–`4`, `bridge`, `sign`, `flag_1`, `chest`, `vase`, and about 6 tilemap cells.

12. **The asset sheet lives inside Brawl.** An `Assets` button (id `assets`) swaps the playfield for a grid. Each cell shows the asset at its natural scale or smaller, its name and kind, a cross on its anchor, its collision template (solid in red outline, platform as a yellow line), and its default layer. Pictures load a few per frame, like maps (`gallery:state` reads n/total). This is the clips' contact sheet for map pieces. *Rejected for now:* a separate `brawl_assets` app. It would copy the library and need byte-equality checks, and it is more useful later as the asset editor (open question 2).

13. **A collision overlay** (hidden button `overlay`, key F2) draws every derived solid and platform over the art as `col:<id>`. It is how the user and S-M3 see that art and collision agree.

14. **Picking a map.** A `Map: Yard` button (id `map:next`), as in Hillclimb, restarts the match with the new setup. `map:state` is a hidden readout. The art of maps not in view is dropped from the cache, apart from textures shared by path.

15. **Where an editor attaches later** (not planned here). A `brawl_map_editor` app, shaped like Jetpack's map editor, edits a map's `pieces` and `back` lists. Every piece is a flat table of numbers with an id, so a drag is `set x, y`. The library is its palette, with thumbnails from the sheet code. It stores maps in the workspace asset `brawl-maps` (`{ info = { version = 1 }, maps = { … } }`, the same tables as `maps/*.lua`) and checks them with a copy of `map.lua`. The game's load order is the asset if valid, else the file, with the error on the console (Jetpack's fallback). A `bake_brawl.py` writes the maps back to `maps/*.lua`. Editing the library itself (anchors, templates) is a later editor still.

16. **Rust.** None is needed. Two wishes are recorded, not planned: a picture tint/opacity in `gfx` (element `opacity` and haze overlays stand in for it), and a confirmed release of textures on collection (S-M4 measures it; the library cap makes it harmless either way).

## 2. Where the code goes

- `demo_apps/brawl/`:
  - `library.lua` (new, pure): loads the families, `check`, `get`, and `sheet` (the grid layout).
  - `map.lua` (new, pure): `check(map, lib)` and `compile(map, lib, aspect_range)` → `{ stage, scene }`.
  - `depth.lua` (new, pure): `place` (depth zoom), `drift`, and `cover` (tile counts and coverage).
  - `stage_art.lua` → `scene_art.lua`: frames per (asset, flip), built once and cached by path. It also holds the vector recipes (sky, haze, ridge, caps), 3-slice, the incremental load, `under`/`over` elements, ids `piece:<id>`, and `plain` (no art).
  - `gallery.lua` (new): the Assets view.
  - `camera.lua`: `parallax` is kept and calls `Depth.place`.
  - `fighter.lua`: drops the `art` optional field. Nothing else changes.
  - `main.lua`: map choice, `assets`, `overlay`, `map:next`, and the readouts `map:state`, `gallery:state`.
  - `maps/index.lua`, `maps/yard.lua`, `maps/isles.lua`, `maps/assets/{sky,backdrop,ground,platforms,props}.lua`.
  - `art/*.png` (renamed and extended), `art/License.txt`, `art/SOURCES.txt`.
  - `stages/yard.lua` is removed.
- `scripts/`:
  - New: `brawl_pack.py`, `test_brawl_library.lua`, `test_brawl_maps.lua`, `test_brawl_depth.lua`, `fixtures/brawl_yard_v1.lua` (today's yard, for P-K2), `smoke_brawl_maps.py`.
  - Changed: `brawl_test_util.lua` (`U.yard` = the compiled yard); `test_brawl_camera.lua`'s P-C5 moves into P-L1 and P-V2.
- `app_host/tests/brawl.rs`: L-1 revised, L-2, L-3.
- Docs: this file; `brawl-movement.md` gets a dated note that decision 9 and `stages/yard.lua` are replaced; `status.md` is updated per step; `lua-apps.md` gets a short pattern, "a map from an asset library: art and collision from one placement", and a note that element `opacity` dims pictures, once verified.

## 3. Steps

Every step keeps `cargo test` (via loadtable), `run_brawl_lua_tests.sh` and the smokes green (S-10).

1. **The library, first** (P-L1–L3, L-3). **Opus**: it sets the format the maps, the sheet and the future editor all read.
   - Write `library.lua` and `art/SOURCES.txt`.
   - Write `brawl_pack.py` and print the tilemap grid sheet. **Screenshot for the user:** `shots/brawl/tilemap-grid.png`, to agree the cap cells.
   - Define assets for the 12 pictures in use today, renamed, plus the vector `sky` and `haze`.
   - The game still runs on `stages/yard.lua`. Nothing visible changes.
2. **The yard on the library** (P-K1–K5, L-1 revised, S-M1). **Sonnet**: P-K2 pins every number.
   - Write `map.lua` and `maps/yard.lua` using step 1's assets.
   - Rename `stage_art.lua` to `scene_art.lua`, drawing from the scene.
   - Setup carries only the stage. The old yard becomes a fixture.
   - **Screenshot:** `maps-yard.png`, which should look like today's yard. That shows the refactor changed nothing on screen.
3. **The full library, the sheet and the overlay** (P-L3 at full size, P-V4, S-M2, S-M3). **Sonnet.**
   - Pack in about 48 pictures, with their anchors, templates (boulder solid, bridge platform) and 3-slices measured.
   - Add `gallery.lua` and the overlay.
   - **Screenshots:** `assets.png`, which the user judges asset by asset, and `maps-yard-overlay.png`.
4. **Backgrounds that read** (P-V1–V3, S-M6). **Opus**: camera math plus visual judgement, iterated against screenshots.
   - Write `depth.lua` and computed tiles.
   - Add haze, fog at opacity (or baked alpha), drifting clouds, the pale ridge, the abyss mist, ground caps and 3-slice platforms.
   - Rebuild yard's `back` list to the §1.8 recipe.
   - **Screenshots:** `maps-yard-{idle,edge,top,wide}.png`, set beside step 3's for comparison.
5. **A second map, switching and live edits** (P-K6, P-K7, L-2, S-M4, S-M5). **Sonnet**, with an Opus review of the layout's playability numbers. `maps/isles.lua` ("Twin Isles", golden hour):
   - islands are solids x −1100…−260 and 260…1100, y 0…600, with a pit between them;
   - a `bridge` one-way platform across the pit at y 0, x −260…260 (Down drops into the pit);
   - `wood_ledge` side ledges at y −280, x −900…−620 and 620…900;
   - a top `platform_1` at y −420, x −160…160;
   - a `boulder` solid on the left island at x −480 (150 × 150);
   - spawns (−700, 0) and (700, 0), respawn (0, −700);
   - blast x −2200…2200, y −1700…1100; camera x −1500…1500, y −1100…700.

   Also: the `Map:` button, per-map budgets and the live edit. **Screenshots:** `maps-isles-{idle,pit,top}.png`.

After this plan, each is its own plan: the map editor (§1.15), slopes on the geometry module (movement §1a), more maps, and a foreground layer if wanted.

## 4. Questions, answered by the user 2026-11-10

1. **Second map: Twin Isles.** "fine".
2. **Asset sheet: whichever is easier for now.** So the hidden `Assets` view inside Brawl (decision 12).
3. **Foreground layer: "fine".** Read as yes: soft bushes may pass in front of the fighters at the
   bottom corners only (`fore`, decision 6). *My reading; the user may overrule it on the screenshots.*
4. **Clouds: soft, and SVG may be used.** Clouds become a `cloud` asset kind drawn from `.svg` files
   through `gfx.svg` (as Hillclimb's `art/*.svg`), soft-edged (layered translucent lobes), not the
   pack's flat white PNGs. Vector assets (`ridge`, clouds, and any piece the pack lacks) may be SVG
   files in `art/` rather than Lua-drawn shapes, whichever reads better; both are counted in L-1.

The other §1 decisions stand as proposed; nothing above overrules them.

## 5. Step notes

### Step 1 landed 2026-11-11 (branch `2d-characters`, not merged)

Built: `demo_apps/brawl/library.lua`; `maps/assets/{sky,backdrop,ground,platforms,props,clouds}.lua`;
`art/SOURCES.txt`; `art/cloud_soft_1.svg`; `stage_art.lua` checks the library once per open;
`scripts/test_brawl_library.lua` (P-L1–L3, picked up by `run_brawl_lua_tests.sh`);
`scripts/brawl_pack.py`; L-3 in `app_host/tests/brawl.rs` (its loader also takes `.svg` as source and
can edit a file before opening). No Rust outside the test file. `shots/brawl/tilemap-grid.png` is the
tilemap contact sheet. The game still draws `stages/yard.lua`; nothing on screen changed (S-10's
`smoke_brawl.py` passes).

- **The library, 15 assets:** `sky.day` (today's two stops), `backdrop.{mountain_far, mountain_near,
  hedge}` (`layer`, today's `src` crops and bodies) and `haze_day`, `ground.grass_ground`,
  `platforms.wood_ledge`, `props.{tree, tree_2, bush, rock, grass_3, cloud, cloud_2}` and
  `clouds.cloud_soft_1`. **Budget (P-L3):** 13 files, 12 textures, **14.35 MiB** estimated (mips plus
  copied `src`), against L-1's measured 14.33 MiB for the same pictures.
- **Format choices** (the user may overrule): a sixth family, `clouds`, holds the new kind `cloud`
  (`svg`, `size`, `anchor`, `scale`, `layer`, `opacity`), per §4 answer 4. The pack's flat clouds stay
  `prop`s on layer `far`. Each kind lives in one family (a `layer` in `props.lua` is an error). Each
  kind's allowed layers are listed and the first is its default (`prop`: stage; `layer`, `cloud`:
  far; `haze`: mid). Asset names are global, which is how "one name in two families" is caught.
  `Library.check` returns the first problem, in family then name order. `get` returns a copy with
  `layer`, `scale`, `opacity` and `anchor` filled (the anchor defaults to the picture's bottom
  centre, which for today's props is also their opaque box's). `src` has no 64 000 px limit yet:
  decision 10's file crops come with step 3's pack-in.
- **Lifts measured, not guessed:** `platform_4.png` has 10 transparent rows, so `wood_ledge.lift` is
  **10** (the yard's art uses 4: the 6 px gap movement step 4 found). The ground strip is opaque from
  row 0, so `grass_ground.lift` is **0** (the yard uses 3). Step 2's yard on the library therefore
  moves the platform grass up 6 units (the fix) and the ground grass down 3.
- **The renames of decision 10 wait for step 2.** `smoke_brawl.py` (another agent's file at the time)
  lists today's file names, so renaming now breaks S-10. Asset *names* are already the planned ones
  (`tree`, `grass_3`, `cloud`); only `image` paths say `tree_1.png`, `grass.png`, `cloud_1.png`, etc.
  Step 2 renames the files and updates `SOURCES.txt`, the assets, `stages/yard.lua` and the smoke's
  `ART` set together.
- **The in-app check.** The VM can't read PNG headers, so sizes against files are P-L1's job (stock
  Lua reads the header). In the app, `Library.check` runs structurally, and every SVG goes through
  `gfx.svg` and `gfx.drawing` with dropped features refused: that is the SVG asset path, exercised
  in L-1, L-3 and the smoke on every open.
- **The console:** an app's `print` goes to the shell's stdout, not to `read_console` (probed: L-3's
  console stayed empty while the line printed). `lua-apps.md` said otherwise and is corrected. So L-3
  reads the error from a hidden `library:problem` readout that `StageArt.under` adds only when there
  is a problem, and the line is printed too. Getting it on the console needs Rust (`print` into the
  app's console), which this plan rules out: a wish for the user.
- **The soft cloud** (`cloud_soft_1.svg`, 480 × 220): eight lobes and a base ellipse, drawn as three
  faint white rings (opacity 0.12/0.16/0.22, each one path so overlaps fill once), a pale core and two
  highlight passes. Checked by eye only as ImageMagick renders it; not yet drawn by the game (step 4).
- **Tilemap caps** (`tilemap-grid.png`, cells col,row at 128 px): the ground block's top-left cap is
  **0,0** and its top-right cap **2,0**, with **0,1** and **2,1** the side walls below them; **1,0**
  is the top strip that matches them. Today's `ground_strip.png` is a crop of cell 7,2 (128 × 97 at
  x 896, y 283), a different block with a plainer body, so the caps may not join it: step 4 should crop
  0,0/2,0 and judge against 1,0 as the top. Rows 3–4 are slopes (out of scope), 6,4/7,4 a small
  legged ledge, row 7 a long fenced bridge, column 7 rows 0–2 a ladder.
- **Test order:** `library.lua` was written before `test_brawl_library.lua` (not tests first). The
  test then failed red on today's platform lift (4 against the measured 10) before the asset took 10.
  L-3 was red (no check in the app) before `stage_art.lua` was changed.
- **Unverified:** that the soft cloud reads well in the game; `brawl_pack.py`'s write path (only
  `--check` was run: all 12 files match the zip's pixels by ImageMagick's pixel signature).

### Step 2 landed 2026-11-11 (branch `2d-characters`, not merged)

Built: `demo_apps/brawl/map.lua`, `maps/yard.lua`, `maps/index.lua`; `stage_art.lua` renamed
`scene_art.lua`; `stages/yard.lua` removed (`scripts/fixtures/brawl_yard_v1.lua` keeps its collision);
`fighter.lua` lost its optional `art` field; `library.lua` gained `files_of`; `main.lua` compiles the
first map in the index, hands `net.match` only the stage, and has hidden readouts `map:state` (`yard`)
and `map:art` (the PNGs the compiled scene names); art files renamed per decision 10 (`SOURCES.txt`,
the assets and `smoke_brawl.py`'s `ART` updated together, `License.txt` kept). Tests:
`scripts/test_brawl_maps.lua` (P-K1–K5, the stock-Lua half of L-1), `brawl_test_util.lua`'s `U.yard` is
now the compiled yard, so every P-B and P-M test runs on it; `app_host/tests/brawl.rs` (L-1 revised);
`scripts/smoke_brawl_maps.py` (S-M1, in `SMOKES`). No Rust.

- **Order of the tests:** the fixture and `test_brawl_maps.lua` were written first and failed (no
  `map.lua`); then `map.lua`. P-K2's P-M12 check runs `test_brawl_moves.lua` as a child and compares
  its final-state line with the literal printed before the plan
  (`tick=1200 x=-738.5 y=-240.5 vx=-7 … drop=0`): identical.
- **`map.lua` API:** `check(map, lib)` -> true | nil, error (first problem: map fields, then `back`,
  then `pieces`, as `pieces[8] "left".x: missing`); `stage(map, lib, {body})` (collision only, then
  `fighter.check_stage`); `scene(map, lib, stage)`; `compile` = check + stage + scene, erroring with a
  string. `main.lua` calls `Map.stage` alone when the library is broken (L-3: collision doesn't need
  the pictures), so the match still has its stage and the stage is drawn plain.
- **Placement semantics:** a `layer` back entry's `y` is its picture's top (as today's art list); a
  prop's (in `pieces` or `back`) is its anchor, so the three clouds' `y` are today's tops plus
  `h × scale` (e.g. -495.4). Pieces take `scale`, `flip`, `layer`, `f` (fore only) where the kind allows;
  a ground takes `x y w h`, a platform `x y w`. Flip mirrors the picture about the anchor in `scene_art`
  (a transform on the image) and the collision template about it in `map.lua`; the yard flips nothing,
  so the picture side is untested by pixels.
- **Decisions the plan left open:**
  1. *Order.* The plan's yard lists the platforms before the props; today's art draws the props first
     and the platforms over them (a tree's canopy meets the left ledge). To keep the look, `yard.lua`
     lists ground, props, then the three ledges. The stage's platform order (left, right, top) is
     unchanged, which P-K2 needs for `on`.
  2. *`bottom`.* Back layers' body colour stops at y = 40 today. That is a map field `bottom = 40` until
     step 4's abyss mist replaces it. Ground bodies go down to the camera bounds' bottom (700, as before).
  3. *Tiles.* Back layers keep today's 3 copies (`TILES` in `map.lua`); step 4 computes them. Ground
     tiles are `round(w / 128)` = 14 for the yard.
  4. *Not accepted yet* (unknown field or "can't be placed"): `opacity`, `drift`, `span`, and `haze`,
     `ridge` and `cloud` assets in `back`. They need step 4's drawing and its opacity verification;
     accepting what nothing draws would be silent. The sky's `sun` is carried in the scene but not drawn.
  5. *Names.* The map's `name` is `"yard"` (the stage name P-K2 pins), shown by `map:state`; a display
     name is the `Map:` button's business (step 5).
  6. *P-C5* (the old art list against the files) is gone from `test_brawl_camera.lua`: sizes against
     files are P-L1's, and its coverage check now reads the scene's tiled layers (`w >= bounds.w`) and
     the picture count (12).
- **The platform gap is fixed.** Lifts come from the library (platform 10, ground 0), so the ledge's
  grass is 7.5 units higher than before (`lift` 4 to 10, times 1.25) and the ground's 3 units lower.
  `smoke_brawl.py` now asserts the first grass row under the soles is within 1 px (`SURFACE_GAP_MAX`) in
  every planted check, platform included (before: the platform case was only printed): idle, land, run,
  mirrored 0 px, platform 1 px.
- **L-1 revised:** `app_host/tests/brawl.rs` compares the pictures in the view with the PNGs `map:art`
  names, and those with the files in `art/`. 12 pictures, 12 textures, 14.33 MiB with mips (3.36 MiB
  of it copied regions): the same as before, since the yard draws the same pictures.
- **Perf (release shell, offscreen 1280x720, scripted 1200-tick run, `scripts/perf_brawl.py`):** work p50 2.10 ms, p95 2.84 ms, max 3.48 ms, 0 frames over 16.7 ms.
- **Screenshots looked at:** `shots/brawl/maps-yard.png` against the earlier `step4_idle.png`, and
  `maps-yard-platform.png` against `step4_platform.png`: the same yard; the ground grass sits a few
  pixels lower and the ledges' grass a few higher, with the robot's soles on the grass on the ledge.
  The pale strip left of the ground at the screen bottom in the platform shot is step 3's known edge
  (step 4 of this plan fixes it).
- **Unverified:** flipped props in pixels; the `fore` layer in pixels (nothing uses it); the
  `brawl_pack.py` write path (`--check` was run against the renamed files: all 12 same as the zip's).

### Step 3 landed 2026-11-11 (branch `2d-characters`, not merged)

Built: 36 more pictures in `art/` (48 PNGs, `SOURCES.txt` and `brawl_pack.py`'s write path, then
`--check`: all 48 same as the zip's pixels); `scripts/brawl_soft_clouds.py` writes `cloud_soft_2.svg` and
`_3.svg` (the same ring recipe as `_1`); assets in `maps/assets/*.lua` (51 in all); `library.lua`
`extent` and `sheet`; `gallery.lua`; the overlay and `assets` button in `main.lua`; `piece:<id>` ids in
`scene_art.lua`. Tests: P-L1 (rail-aware lift), P-L2 (rail, slice), P-L3 (at least 44 textures,
templates, caps, clouds) and P-V4 in `scripts/test_brawl_library.lua` (written first and red: no `sheet`,
no `rail`); S-M2 and S-M3 in `scripts/smoke_brawl_maps.py` (red: no `assets` button). No Rust.
Results: lua tests, `cargo test -p app_host --test brawl` (2 passed), `smoke.py` with `smoke_brawl`,
`smoke_brawl_maps`, `smoke_characters` all green.

- **Budget (P-L3 prints):** 51 assets, 51 files, **48 textures, 32.45 MiB** (mips plus copied `src`), against
  56 and 64. S-M2's dump with the whole sheet loaded plus the yard: 48 textures, 20.3 MiB of picture bytes
  (the dump counts decoded pixels without mips). The yard's own L-1 numbers are unchanged (12 textures).
  Release perf (`perf_brawl.py`, 1280x720): p50 2.14 ms, p95 2.92 ms, max 4.52 ms, 0 frames over 16.7 ms.
- **Names.** New assets use the file stem (`background_fog_1`, `foreground_basic_2/3`, `tree_1`, `bush_1`,
  `boulder_5`, `platform_3`, `bridge`, `chest`, ...). Yard's old names stay (`mountain_far`, `mountain_near`,
  `hedge` = `foreground_basic_1`, `wood_ledge` = `platform_4`, `grass_ground`) so `maps/yard.lua` is
  untouched. The pack's four flat clouds are `cloud`, `cloud_1`, `cloud_2`, `cloud_3`; the soft SVG ones
  `cloud_soft_1..3` (family `clouds`).
- **Crops.** The five new large layers (`background_fog_1..3`, `foreground_basic_2/3`) are cropped to
  their opaque box in the file (decision 10), so their picture's top is the silhouette's top and no `src`
  copy is needed (the yard's three old layers keep their `src`).
- **The bridge is a platform asset with a `rail`.** Its picture's first opaque row is the fence posts
  (row 0); the deck's walking surface is row 93. P-L1 said `lift` = first opaque row, which can't hold for a
  fenced bridge, so a platform may say `rail = N` (picture rows of decoration above the surface) and P-L1
  now checks `lift == first opaque row + rail` (bridge: 93 = 0 + 93). Every other platform has no rail and
  is checked as before. Isles' bridge can therefore be any width (3-sliced `{60, 407}`), where a prop with a
  platform template would have a fixed one.
- **Platform measurements.** Lifts (first opaque row): `platform` 1, `platform_1/2/3` 0, `platform_4`
  (`wood_ledge`) 10, `bridge` 93. Slices are `{40, w − 40}` for the ledges (end supports kept), `{60, 407}`
  for the bridge; they are read by step 4's drawing, not yet drawn (the yard draws the ledge stretched, as
  before).
- **Boulder templates** (picture px from the anchor; a solid box inside the picture, chosen by eye on the
  sheet): `boulder` (234 px, default scale 0.75) `{-100, -200, 200, 200}`, anchor (117, 233): 150 x 150
  units, as the isles plan wants; `boulder_2` (scale 0.75) `{-110, -220, 220, 220}`; `boulder_3`
  and `boulder_5` (scale 1) `{-52, -104, 104, 104}`. They are discs and rounded squares, so the box meets
  the picture's corners only roughly; the overlay shows it. Only `vase` (42, 120) and `boulder` have an
  anchor other than the picture's bottom centre (opaque-box measured: every other picture's opaque box
  ends within 1 px of the picture bottom).
- **Ground caps (judged in step 4).** `grass_block` is a ground asset (`ground_block_top` = cell 1,0, lift
  2; `ground_cap_l` = 0,0 and `ground_cap_r` = 2,0 as `left`/`right`; body `#463c56`); the side walls
  (cells 0,1 and 2,1) are the props `ground_wall_l/r`. A cap's opaque part is inset (cap_l starts at column
  18, cap_r ends at column 113), which step 4 must place for. `grass_ground` (the yard's) is unchanged.
- **The sheet.** `Library.sheet(lib, {w, h})` lays out cells of `Library.CELL` (168 x 176, 8 px padding, a
  34 px label) over pages; the picture box is the cell less padding and label, and an asset is drawn at
  `min(1, box/extent)`, so the scale does not depend on the window. Extents: a picture or its `src`;
  a ground's left cap, top and right cap side by side; a 160 x 100 swatch for sky, haze and ridge. A page of
  1800 x 1100 holds the 51 assets on one page (P-V4 checks 1800x1100, 900x700 and 500x400: 1, 4 and 13
  pages); `gallery.lua` draws the page and adds `assets:prev` / `assets:next` when there are several.
  Each cell shows the picture, the anchor as a pink cross, a solid template as a red outline and
  translucent fill, a one-way platform as a yellow line, a platform or bridge's `lift` as that yellow line
  across the picture and its 3-slice columns as cyan lines, a ground's solid top as a red box; the label is
  `name` / `kind · layer · size`. Pictures load a few per frame (the same 400 000 pixels as the map), sharing
  texture handles with `scene_art` (`SceneArt.texture`). The sheet's shot is taken at 1800 x 1200 so the
  whole library and every name read on one image.
- **The overlay.** `overlay` (hidden button) or F2 toggles it; `overlay:state` reads `on`/`off`. Each
  `col:<id>` is a frame at the derived collision box: a solid is the whole box (red outline, translucent
  fill), a platform a 6-unit yellow bar hanging from the line (a line has no height, and a zero-height
  element wouldn't be found by `rects()`). It is drawn above the art and the fighter. Collision-bearing
  pieces now have `piece:<id>` frames (with a no-op hover so `rects()` lists them).
- **S-M3, what "within 1 px" means.** For ground and ledges the frame and the box share `x` and width, and
  the picture's surface is on the collision top: `piece.y + lift × scale × k == col.y` (yard: ground lift 0,
  ledge lift 10, ledge scale 320/256, constants in the smoke). The ground's frame also runs on to the
  view's bottom (its body), so only its top is compared with the box's. Props with templates would be
  checked inside their frame, not equal; the yard has none.
- **Screenshots looked at:** `shots/brawl/assets.png` (1800 x 1200, one page, names legible; the anchors sit
  on the trunk, grass and rock bases, the vase's on its foot; the yellow lines sit on the first opaque row of
  every ledge and on the bridge deck; the boulder boxes sit inside the discs; the soft clouds draw through
  `gfx.svg`; fogs are white masks, as known) and `maps-yard-overlay.png` (the red box's top on the ground's
  grass, yellow bars on the three ledges' grass). Nothing read wrong that I could fix in this step.
- **Unverified:** the 3-slice and cap pictures *placed* in a map (step 4); the sun glow on a `sky` swatch
  (no sky has a sun yet); the `ridge` swatch (no ridge asset yet); the sheet's paging buttons
  (`assets:prev/next`, a single page at the smoke's size); S-M4's texture release after the sheet closes
  (the sheet's frames stay cached once built).

### Step 4 landed 2026-11-11 (branch `2d-characters`, not merged)

Built: `demo_apps/brawl/depth.lua` (pure: `shift`, `scale`, `place`, `visible`, `cases`, `cover`, `tiles`,
`drift`); `camera.lua`'s `parallax` is now `Depth.shift`; `map.lua` computes every back item's tiles, width and
body depth from the views the camera can take, accepts `haze`, `ridge` and `cloud` assets and `opacity`,
`drift`, `span` in `back` (and `scale` on a platform piece), and drops the map field `bottom`; `scene_art.lua`
draws the vector sky (3 stops and a sun glow, screen-fixed, rebuilt when the playfield's size changes), hazes,
the ridge, soft SVG clouds that drift on render time, translucent layers by element `opacity`, the capped
ground (caps, walls, stretched top) and 3-sliced platforms, all placed by depth zoom. Library: the ground
kind takes `left_wall`, `right_wall`, `inset` and `rail`; new assets `haze_far`, `haze_abyss`, `ridge_pale`,
and the `day` sky got its third stop and sun. `maps/yard.lua`'s `back` is decision 8's recipe and its ground
is `grass_block`. `main.lua` has a hidden `wide` button (view only: the camera pulls all the way out).
Tests: `scripts/test_brawl_depth.lua` (P-V1–V3, new); P-L1 measures the walls' insets, P-L2 refuses unpaired
caps and walls and bad insets, P-K1 refuses the new back fields' misuse, L-1's count is 19 (16 PNGs, 3 SVGs),
P-C5 counts 5 tiled layers (coverage moved to P-V2); S-M6 in `scripts/smoke_brawl_maps.py`. No Rust.

- **Tests first:** `test_brawl_depth.lua` was written before `depth.lua` and failed (no module); P-V1's f = 0
  check was then wrong in the test (it compared views of two shapes, whose centres differ) and was fixed to
  compare a moved and zoomed view of the same shape. P-V3 went red on the yard's first clouds (spans of 4 200
  wrapped in view on the left) and the spans became 5 000. S-M6 was written before the yard's new `back` but
  run after it, so it was never seen red against step 3's yard.
- **Results:** `sh scripts/run_brawl_lua_tests.sh` all green, P-K2 and P-M12's final state unchanged
  (`tick=1200 x=-738.5 y=-240.5 … drop=0`); `cargo test -p app_host --test brawl` 2 passed (via loadtable);
  `smoke.py` with `smoke_brawl`, `smoke_brawl_maps`, `smoke_characters` green (`smoke_brawl` after one fix,
  below). P-V2 tries 504 layer-and-view pairs (7 aspects, 1 to 2.4).
- **Budget:** the yard draws 18 pictures, **16 textures, 18.04 MiB** with mips (3.44 MiB of it copied regions),
  against 32 and 48 per map. The library is 54 assets, 51 files, 48 textures, 32.45 MiB (no new files).
- **Perf (release shell, offscreen, `perf_brawl.py`):** 1280x720 p50 2.15 ms, p95 2.89, max 3.97; 1920x1080
  p50 2.26, p95 2.87, max 5.52; 0 frames over 16.7 ms (step 3: p50 2.14).
- **Depth zoom, as decided:** a layer's px per unit is `k1·(1 + (zoom − 1)·f)`, it follows the camera by f
  about `home`, and f = 1 takes the stage's own mapping exactly (so `piece:<id>` frames still meet S-M3).
  `Depth.place` has an optional px-per-unit argument; with the stage's k it is the old parallax (P-V1 checks
  it against `Camera.parallax`).
- **Decisions made in the step** (the user may overrule):
  1. *Coverage is computed, with a margin.* `map.lua` takes the union of what the views of `Depth.cases`
     (bounds' corners × nearest and furthest zoom × aspects 1, 4/3, 1.5, 16/9, 2, 2.4) show of each layer,
     plus 64 units, and sizes tiles, haze and ridge widths and body depths to it. A window narrower than
     1:1 is not covered by construction.
  2. *A back entry's `x` is optional* (0, its centre); a haze or ridge is as wide as the coverage, wherever x is.
  3. *Clouds drift on render time* (`on_frame`'s `elapsed`), centred on their placed x, wrapping in `span`;
     P-V3 asserts every drifting cloud wraps outside the widest coverage, so the jump is never seen.
  4. *The sky is screen-fixed* (no 0.05 vertical follow): its gradient spans the playfield's height, and the
     sun's `r` is px on a 1 600 px playfield; the glow fades out to 4 r.
  5. *Platforms keep the yard's look.* A platform piece may say `scale` (default 1); the 3-slice keeps the
     ends at that scale and stretches the middle. The yard's ledges say 1.25, which at 320 wide is the
     picture's own proportions, so the ledges look as before; the 3-slice shows on wider ledges (isles).
  6. *Ground caps join cleanly, so the yard uses `grass_block`* (the tilemap block judged by eye on a
     composite: caps, top strip and walls line up) rather than the vector edge, which is not built. The
     caps and walls sit `inset` columns outside the solid (walls 29 and 25), the top strip runs 2 px under
     each cap (abutting showed a hairline), walls repeat down to the camera's bottom. The block's grass has
     tufts above its surface, so the ground kind took `rail` as the bridge did: `lift` 24 = first opaque
     row 3 + rail 21 (P-L1 checks it). The collision is unchanged (P-K2); only the art moved.
  7. *No `fore` bushes.* The recipe didn't need them and they would hide the fighters near the corners; the
     layer stays available.
  8. *A hidden `wide` button* stands in for the plan's "wide uses the `pair` button": `pair` only draws a
     second robot, the camera frames one fighter, so it never zooms out. `wide` adds the bounds' corners to
     the camera's boxes (view only); the shot uses both.
- **Element opacity, verified:** it dims a picture (the fog at 0.35 reads as mist, no white wall), so no alpha
  is baked. But it applies item by item: the fog's tiles overlapping 4 px showed a lighter 4 px stripe.
  A translucent layer now overlaps its copies by 1 px (the pack's pictures repeat every 1 024 of their 1 025
  columns, and the 1 025th is near transparent); a faint 1 px line is left at 4x zoom. `lua-apps.md` says so.
- **`smoke_brawl.py` fix:** the yard now takes one more frame to load its art, and the frames spent in the
  art wait were counted as a 4-tick frame; the smoke now resets its tick count after that wait (as it already
  did after pointer moves). Its `ART` set is the yard's 16 PNGs.
- **Look-adjust rounds** (900x700 offscreen, `maps-yard-{idle,edge,top,wide}.png`, against step 3's
  `maps-yard.png` and `step3_edge.png`):
  1. First build: the edge already showed mist below the hedge instead of sky, the ground had its rimmed
     walls and the clouds were soft. But the far and near mountains were the same saturated teal (no depth
     between them), and the far haze (`haze_day` from 0) left the peaks untouched.
  2. A new `haze_far` (from 0.3) over the far range: the far mountains went pale and stepped back, but its
     top drew a hard horizontal line across the sky.
  3. `haze_far` from 0, placed higher (y −640), so it fades in above the peaks: no line. The hedge (f 0.6 to
     0.75), the abyss mist (to 0.75) and the light hills (0.4 to 0.45) came nearer, so at the top platform
     they rise less and the lower view is less of a green wall.
  4. Seams: a light stripe in the fog (opacity per item, above: overlap 1 px) and a hairline between the
     ground's left cap and its top strip in a 1600x700 zoomed-out shot (strip under the caps by 2 px). Both
     gone at 4x zoom.
- **Screenshots looked at:** `maps-yard-idle.png`, `-edge.png`, `-top.png`, `-wide.png` each round, a 1600x700
  zoomed-out shot (not kept), zooms of the edge, the seams and the caps, `assets.png` (54 assets, the ridge and
  hazes as swatches).
- **Unverified / left:** windows narrower than 1:1 (not in the cases); at very wide windows zoomed out
  (1600x700) the far haze covers most of the sky and the top reads washed; the robot over the abyss mist has
  less contrast than over grass (its outline carries it); the vector cap (`edge`) is still unbuilt; S-M6 checks
  bare sky by colour (the sky's first and last stops), so a backdrop colour close to either would hide a gap.

### Step 5 landed 2026-11-11 (branch `2d-characters`, not merged)

Built: `maps/isles.lua` (Twin Isles) and `maps/index.lua` = `{ "yard", "isles" }`. `main.lua` loads the map by
index (`load_map`), has a visible `Map: <name>` button (id `map:next`, top right) that compiles the next map,
restarts the match, drops the old art and camera, and calls the new `SceneArt.release(images)` so the old
map's textures (not those the new map shares) can be collected. Library: a `golden` sky, `haze_gold_far`,
`haze_gold`, `haze_abyss_gold` and `ridge_dusk`: vector only, no new texture. No Rust.
Tests: `scripts/test_brawl_isles.lua` (P-K6, P-K7, new), P-V2/V3 run on both maps through the index,
L-2 (`each_brawl_map_opens_clean_inside_its_own_budget`) in `app_host/tests/brawl.rs`, S-M4 and S-M5 in
`scripts/smoke_brawl_maps.py` (shots `maps-isles-{idle,pit,top,wide}.png`).

- **Tests first:** `test_brawl_isles.lua` failed on the index, then on the missing map, before `isles.lua` was
  written. Two test bugs were fixed against the fighter's real rules: the boulder jump held right all the
  way up and overshot (or landed on `ledge_l`), so the test lets go of right, and of jump, once the feet clear
  the top. S-M4/S-M5 passed on their first run once the map existed (they were never seen red).
- **Results:** `run_brawl_lua_tests.sh` green (P-K2 and P-M12's final state unchanged; P-V2 now 1008 layer-and-view
  checks); `cargo test -p app_host --test brawl` 3 passed (job 2878 on nixos): **L-2 yard 16 textures 18.04 MiB,
  isles 22 textures 19.82 MiB** (caps 32 and 48); S-M4: each switch is clean, 22 and 16 textures in the dump, never
  above the library's 56; S-M5 passes (below).
- **The layout differs from the plan's in one number:** the boulder is `boulder_5` (a grass-topped square) at scale
  1.5, a **156 x 156** solid at x −558…−402, not `boulder` at 150. The round boulders in the pack are dark discs
  (they read as tyres or holes beside a square box); `boulder_5` fits its box. The rest is the plan's: islands
  x −1100…−260 and 260…1100 (y 0…600), `bridge` platform x −260…260 at y 0, `wood_ledge` at y −280 (x −900 and
  620, w 280), `platform_1` top at y −420 (w 320), spawns (±700, 0), respawn (0, −700), the plan's blast and camera.
- **P-K7 as built:** a fighter spawns standing on the left island; Down on the bridge drops it into the pit, the
  blast takes a stock, the respawn lands on `top` at `y == −420`; at both inner walls (x −220 and 220, y 300) it
  slides at `wall_slide` and wall jumps; it runs into the boulder (`vx == 0` at x −598) and, jumping, stands on it
  (`y == −156`, `on == 0`). P-K6: 10 shared assets, yard-only 10, isles-only 15 (the sky and hazes count).
- **S-M5 as built:** the live edit moves yard's `right` ledge from x 300 to 364 (+64) through `read_file_versioned`
  and `edit_file` on `maps/yard.lua`; the shell reloads by itself. `piece:right` moves 44.0 px for 64 units at
  k 0.69. The fighter, jumping at x 650 (only the new place is under it), stands on it at y −260. The broken edit
  (`wood_ledgee`) puts `reload error: ... pieces[9] "right".asset: unknown asset wood_ledgee` on the console and the
  old app keeps running; the fix adds nothing more. The state readout has no `on`, so "`on` is set" is read as
  `y == −260, vy == 0`.
- **Decisions made in the step** (the user may overrule): (1) the `Map:` button is visible, not hidden, since a player
  can use it; (2) a switch resets the match (a new match and camera), it does not carry the fighter; (3) the isles'
  warm colour grading is four new vector assets rather than an opacity trick on the day ones; (4) the isles have
  cliffs to the camera's bottom, not floating undersides, since the pack has no island underside.
- **Look-adjust rounds** (900x700 offscreen, `maps-isles-*.png`): round 1: the round boulder read as a tyre and the
  bush overlapped it, and the far mountains went muddy khaki under the peach haze. Round 2: `boulder_5` at 1.5, the
  bush moved to the far end behind the tree, and `haze_gold_far` warmer and stronger (to 0.8): the mountains are a
  warm tan against the peach sky. Looked at each round: idle, pit, top, and wide. S-M6's rule held on both: P-V2 on isles
  and the bottom row of every isles shot is not the golden sky's first or last colour (S-M6 in `smoke_brawl_maps.py`).
- **Perf: not measured.** The release build passed on nixos (job 2879) but the fetch of `target/release/shell2`
  timed out (link lost), a retry (job 2880) never started (nixos unreachable for 10 minutes), and the file at that
  path is a play window's binary anyway. The map adds no per-frame cost the yard didn't have (22 textures against 16).
- **Weak spots:** the pit below the bridge is a flat peach mist (readable, not rich); the isles' tree_1 is pale lilac
  next to the greens; the clouds sit over the HUD's "Stocks" text at the top left; the `Map:` button resets the match
  (the fighter is not carried over).

