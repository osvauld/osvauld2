# Jetpack editors — maps and art authored on desktop, baked into the game

Status: **steps 1–2 built 2026-10-26** (map format, map editor, game import; tests 1, 2, 6);
**step 4 (pilots, with arms) built, awaiting review**; then guns (5) and sound (5b). Follows [Jetpack arena](jetpack-arena.md).
Jetpack is the first app we release, so its content is made with real editors, not hand-typed
tables. Editor pipeline first; feel fixes (thrust, camera, size) after, tuned inside it.

## End-to-end acceptance tests

Through a fresh shell, real bridge, uploaded Lua and an isolated vault:

1. **Map editor round-trip.** Open the map editor; drive real pointer hits (via `rects`) to set
   map size, add a platform, drag it, resize it, place player and bot spawns, delete one. The
   workspace asset `jetpack/map/<name>` holds exactly that. Reopen the editor: same map.
2. **Game reads the edited map.** Open Jetpack in the same workspace: platforms collide where
   the editor put them, the player spawns at the edited spawn. Inspect screenshot pixels.
3. **Character editor round-trip.** Recolour/reshape the pilot; mark `grip` and `jet` points.
   Jetpack draws the edited pilot (screenshot) — player and bot variants from one asset.
4. **Weapon editor round-trip.** Edit the gun's look and its `grip`/`muzzle` points. In game the
   gun sits in the pilot's grip, and shots leave from the muzzle point (model-checked position).
   Make a second gun with different stats: picking it up on the map changes the HUD gun name,
   and a model test shows its damage, rate and spread applied (kills take the expected shots).
5. **Bake.** `scripts/bake_jetpack.py` reads those assets over the bridge and writes
   `demo_apps/jetpack/content/*.lua`. A fresh vault with **no assets** runs the game from the
   baked files and looks identical to test 2–4 (pixel compare). This is how content reaches the
   phone until asset sync exists.
6. **Fallback is safe.** A missing or malformed asset never breaks the game: it falls back to
   the baked content and the console names the bad field.
7. Existing `smoke_jetpack.py` and `test_jetpack_model.lua` stay green.

## Decisions (talked through 2026-10-26)

- **One editor per asset kind**; code is cheap, each editor stays small and specific:
  map, pilot, gun, **effects** (jet flame, muzzle flash, impact, death burst — short clips),
  **props** (decor and cover pieces a map places), **backdrop** (parallax sky/hills layers).
- **Import is the last step of every kind**: editor saves an asset → the game loads it →
  the bake writes it into the game's source for the release.
- **One shared format module** (`content.lua`, copied into each app until shared modules exist)
  defines and validates each kind. Editors write through it; the game and bake read through it.
- **Delivery = bake into source.** Desktop asset sync is not built; the release ships baked
  content. Game load order: workspace asset if present and valid → baked file.
- ~~**Bake needs a read-only bridge op**~~ *Corrected 2026-10-26:* `rpc.read_data(item)` already
  returns an open app's assets as `asset:<name>`, so the bake reads them through the editor.
- The asset root holds containers only: the format version is `info = { version = 1 }`.
- ~~Gameplay rules (damage, fire rate, thrust, bots) stay Lua in the game; assets carry looks,
  geometry and attachment points only.~~ *Revised 2026-10-26: the user wants many guns, each
  with its own damage and design.* A gun asset carries its **stats** as well as its look; the
  game's Lua keeps the **rules** (how damage, cooldown, spread and ammo are applied). Thrust and
  bot behaviour stay in the game.
- **Many guns.** The gun editor keeps a list of guns, each editable and previewed firing.
  The player spawns with the default gun; the map editor places **gun pickups** that swap it.

## Formats (first cut)

- **map**: `size {w,h}`, `platforms [{id,x,y,w,h,kind="ledge"|"cover"}]`,
  `spawns [{id,team="player"|"bot",x,y}]`, `theme` (palette key). Bounds walls implicit.
- ~~**pilot**: drawing (parts/shapes, as `gfx.drawing`), `vars` for team colours,
  points `grip`, `jet`, `eye`; collider `{rx,ry}`.~~ *Revised 2026-10-26 (talked through: shape
  editor, many pilots with team colours):* asset `jetpack-pilots` =
  `{ info = { version = 1 }, pilots = [{ id, name, size = {w,h}, vars, parts, points }] }`.
  `size`/`vars`/`parts` are a `gfx.drawing` module, except that every shape carries an `id` (the
  editor addresses it); `F.drawing(pilot)` strips ids and the extra keys. Parts are exactly
  `leg_back`, `leg_front`, `body` (legs swing about their pivots). `vars.main`/`vars.dark` are
  the team slots the game replaces per team; other vars are the pilot's own palette.
  `points = { grip = {x,y}, jet = {x,y} }` — gun mount and flame top. Eye and collider wait for
  the feel step (collider stays game physics). The player flies `pilots[1]`; bots cycle the list.
  *Revised 2026-10-26 (user: "the free hand and the gun hand"):* parts are
  `arm_free, leg_back, leg_front, body, arm_gun` (draw order) and points `grip` (gun hand, on
  `arm_gun`), `hand` (free hand, on `arm_free`), `jet`. Arms are drawn reaching forward; the game
  turns the gun arm to the aim with the gun at its hand, and the free arm reaches for a spot on
  the barrel (`F.arms`, pure and tested). One-handed guns will let it hang (step 5). A pilot
  saved before arms still loads, armless at its grip; the editor adds empty arms on open.
- **gun**: `id`, `name`, drawing + points `grip`, `muzzle`; stats `damage`, `fire_interval`,
  `shot_speed`, `spread`, `pellets`, `ammo` (nil = unlimited), `range`; `effect` ids for flash
  and tracer. Validated with bounds (no 0-interval or 10000-damage guns).
- **map** also gets `pickups [{id,gun,x,y,respawn}]`.

## Steps

1. Tests 1–2 written and failing; `content.lua` map format + validation (pure Lua test).
2. Map editor app; game loads map asset with baked fallback. Tests 1, 2, 6 pass.
3. Bridge `read_asset` op + `bake_jetpack.py`; test 5 for maps.
4. Pilot editor + game draws pilot asset; test 3. Editor: pilot list (new = copy, delete keeps
   one), part picker, canvas at 10× with named shapes; tools select / rect / ellipse /
   triangle add to the active part; drag moves a shape, the box grip scales it, path points
   drag; palette panel edits vars (hex inputs) and the selected shape's fill picks a var;
   grip/jet markers drag; live previews at game size for both teams. Bake gains
   `jetpack-pilots` and `--item` repeats. Game caches poses by (pilot, team, leg step).
   Smoke: visor var → `#ff00ff`, shape moved, rect added, grip moved, second pilot, reopen
   persists; game screenshot has `#ff00ff` pixels on the player.
5. Gun editor + grip/muzzle attachment; test 4; bake covers all three.
5b. Sound (talked through 2026-10-26: generated sounds for now, audio files later): a runtime
   mixer and a Lua `sound` call, a sound editor of generated effects (sfxr-style settings),
   baked like the rest; each gun names its shot sound. Needs its own plan before code.
6. Props editor (map places props by id), backdrop editor, effects editor; each imported and
   baked, each with an editor round-trip + in-game screenshot test.
7. Then the feel plan (separate doc section): proportional thrust, visible jet state, zoomed
   camera with vertical follow, larger maps, players never under controls.
8. Full smokes, fresh review, upload baked game to the Pixel.
