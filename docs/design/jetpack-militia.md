# Jetpack militia — Mini Militia–class combat for Jetpack Arena

Status: **planned 2026-10-26** (long-running; user: "we are looking to replicate mini militia").
Follows [Jetpack editors](jetpack-editors.md), whose steps 1–4 (maps, pilots with arms, bake)
are built. That doc's step 5 (guns), 5b (sound) and 7 (feel) are absorbed here.

The bar is the feel of Mini Militia (Doodle Army 2): a small rigged soldier whose hands really
hold each gun, guns that each look, sound and kill differently, pickups, reloads, grenades,
melee, dual-stick touch controls, bots worth fighting. Gameplay is the reference; **art, names
and sounds are our own** (generic gun names: pistol, SMG, rifle, shotgun, sniper, rocket…).

## End-to-end acceptance tests

Through a fresh shell (offscreen), real bridge, uploaded Lua, isolated vault — each step adds
its rows to `smoke_jetpack.py` / `smoke_jetpack_editors.py` or a new smoke in `SMOKES`, plus
pure-Lua model tests (`scripts/test_jetpack_*.lua`, stock `lua`).

1. **Rig.** A pilot is a rig: head, torso, jetpack, two legs, and per side an upper arm,
   forearm and hand with a trigger finger and thumb. Pure test: two-bone IK puts each hand on
   its target (gun grip / support) within 0.5 units for every aim in a sweep, elbows bend the
   natural way, an unreachable target leaves the arm straight toward it. Head turns with aim,
   clamped. Editor smoke: each part selectable and drawable; reopen persists.
2. **Hands hold the gun.** In game the gun draws between the arms: free arm behind, gun, gun
   hand over the grip with fingers wrapped. Screenshot: hand-colour pixels overlap the gun's
   grip region (model-computed) for a one-handed and a two-handed gun.
3. **Firing animation.** Pure test over the fire timeline: trigger finger squeezes, gun recoils
   along the barrel by the gun's `recoil` and returns within `fire_interval`, flash shows for
   the flash time, a casing ejects from `eject` with outward velocity. Smoke: frame-stepped
   screenshots show the flash at the model's muzzle position and the casing moving.
4. **Guns.** Gun editor round-trip (draw, points `grip`/`support`/`muzzle`/`eject`/`mag`,
   stats); at least six guns baked: pistol, SMG, rifle, shotgun, sniper, rocket. Model test per
   gun: shots to kill a 100-hp target, rate, spread cone, pellets, mag size, reload time,
   rocket splash + knockback, range falloff — all as the asset says.
5. **Weapon spawns, pickups and loadout.** The map editor places weapon spawn points, each
   with a gun (or a weighted pool of guns) and a respawn time. The game spawns them at match
   start; a taken gun respawns after its time; a dead soldier drops what they carried (it
   despawns after a while); everyone starts with the map's starting loadout. The player walks
   over a gun and presses swap: the HUD names the new gun, the old one drops in its place.
   Two carried guns; the swap button toggles. Model test drives a whole spawn → take →
   respawn → death-drop cycle on the virtual clock; smoke picks one up.
6. **Reload and ammo.** Mag empties → auto reload (animation: mag drops from `mag`, free hand
   fetches) → mag refills after `reload_time`; HUD ammo counter matches the model.
7. **Grenades and melee.** Frag grenade: thrown along aim, bounces, explodes after its fuse
   with splash damage and knockback; HUD count. Melee: close range swing (gun-butt) kills in
   the gun's melee hits. Model tests for both; smoke throws one.
8. **Controls (dual stick).** Left stick: walk, up = jetpack (proportional to stick height),
   right stick: aim, fires when pushed past the ring (auto weapons hold). Buttons: swap,
   grenade, melee, reload, zoom. Multi-touch smoke drives both sticks at once; the player is
   never under a control (layout check from `rects`).
9. **Camera and maps.** Camera follows both axes with look-ahead toward aim; sniper zoom
   widens view. Maps larger than the screen in both directions. Smoke: camera keeps the
   player inside the safe band across a scripted flight.
10. **Bots.** Bots pick up guns, reload, take cover, use jetpacks over obstacles, throw
    grenades, and are beatable: a scripted model run of N minutes, bot accuracy and kill
    rates inside bands.
11. **Sound.** Runtime mixer + Lua `sound.play`; generated effects (sound editor, sfxr-style),
    each gun names its shot/reload sounds. Test: the play log (bridge) lists the expected
    sounds for a scripted fire/reload/explode. Needs its own plan doc before code.
12. **HUD and match.** Health and jet-fuel bars, ammo, grenades, kill feed, score/timer,
    match end screen with K/D. Smoke reads the labels.
13. **Modules.** The game is split by concern; each rules module is pure Lua with its own
    stock-`lua` test, and only the view modules touch `gfx`/`ui`. Every content kind the
    editors make (maps, pilots, guns, later sounds/props/effects) reaches the game through
    `format.lua` with baked fallback — nothing drawn or tuned inline that an editor owns.
14. Existing jetpack smokes, the format/model tests and `cargo test` stay green every step.

## Decisions

- **One rig, data-driven.** The pilot asset (format v2) keeps parts as a `gfx.drawing` with
  `parent` links; the game poses it procedurally (IK, aim, walk cycle, recoil). No authored
  keyframe clips yet — procedural poses read better on a twitchy shooter and are testable.
- **Gun between arms.** A pilot compiles to two drawings (back: free arm, legs, jetpack,
  torso, head; front: gun arm) so the gun draws between them.
- **Detail via resolution.** Pilots are drawn at 2× (e.g. 80×104 units) with a `scale` the
  game applies, so fingers are shapes, not single pixels.
- **Guns are assets** with look + points + stats; the game's Lua keeps the rules.
- **Format bump.** `info.version = 2` for pilots; v1 pilots upgrade by mapping `arm_*` to
  upper arms with empty forearms/hands. Old map assets stay v1 (pickups are an optional list).
- **Modules** (user, 2026-10-26: "properly modularize"). Rules: `world.lua` (state, step
  order), `physics.lua` (moves, collisions, sweeps), `weapons.lua` (fire, shots, damage,
  reload), `spawns.lua` (soldiers and weapon spawns, drops, respawn timers), `bots.lua`,
  `grenades.lua`, `rig.lua` (IK and poses, no gfx). Views: `view.lua` (scene),
  `pilot_art.lua`, `gun_art.lua`, `hud.lua`, `controls.lua`, `camera.lua`. `main.lua` only
  wires input, clock and view. Shared copies with the editors stay checked identical.
- **Sub-agents.** Steps are built by Sonnet sub-agents with explicit file scopes, one step
  in flight per shared file; the lead designs, reviews, runs the smokes and reports.

## Steps

0. Modularize the game along the split above, no behaviour change: the existing model test
   and both jetpack smokes pass unchanged; new per-module tests. (In parallel with step 1 —
   disjoint files.)
1. Rig: format v2 pilot (parts tree with parents, points), pure IK + pose module
   (`rig.lua`, stock-Lua testable), scout redrawn at 2×, pilot editor gains the parts tree.
   Tests 1–2 (two-handed with the current gun).
   *Built 2026-10-26 (deviations):* the scout palette gains a `glove` var (hands need their own
   colour); `F.versions = { maps = 1, pilots = 2 }` replaces `F.version`; `F.arms` is gone (rig.lua
   replaces it); `R.pose` takes `opts.drop` (free palm below the barrel, so it shows under the gun);
   `rig.lua` uses `math.atan2 or math.atan` because Luau's `math.atan` ignores a second argument.
2. Gun format + gun editor app + six baked guns; gun drawn between arms. Test 4 (look + points).
   *Built 2026-10-26 (deviations):* `format.lua` gains `F.versions.guns = 1`, `F.gun`/`F.guns`/`F.load_guns`/
   `F.gun_drawing` and `F.stat_limits`; `gun_art.lua` (view) draws the gun, and the pure placement is
   `rig.lua`'s `R.gun_hold` / `R.gun_point` / `R.gun_transform` (`scripts/test_jetpack_gun_pose.lua`: every baked
   gun, aim sweep ±85°, both facings: palms within 0.5 game units of grip and support). `pilot_art` takes the
   gun (`P.actor(a, time, pilot, team, gun)`); the placeholder constants are gone. Gun editor:
   `demo_apps/jetpack_gun_editor` (asset `jetpack-guns`; baked by `bake_jetpack.py` into the game and the
   pilot and gun editors). **Scale decision:** `scale` stays per gun but is no longer 0.5. The scout's arms reach
   ~44 pilot units from the shoulder with the grip 21 out, so a free hand can only hold a support point within
   ~20 pilot units of the grip: two-handed guns are drawn at 0.34 game units per drawing unit (support 28 drawing
   units from the grip; the rifle is 34 units long, ~2.4 torsos, the sniper 45), the one-handed pistol at 0.4.
   Bigger guns need longer arms or a `hold` closer to the body in the pilot asset; the gun editor warns when the
   free hand cannot reach the support. Guns have no team colours (`gun_art` compiles one drawing per gun table).
   Test-only seam until step 5: `spawns.lua` gives everyone `S.default_gun = "rifle"`, and an asset
   `jetpack-loadout` holding `loadout = { player, bot }` (gun ids) overrides it (`smoke_jetpack_guns.py` uses it for
   the pistol). `smoke_jetpack_guns.py` (in `SMOKES`) checks the editor round-trip, the game importing the
   edited gun, glove pixels on the grip and support (both facings; none at a rifle's support for the pistol),
   the bake and the baked fallback. `gen_guns.py` now writes per-gun scales and slightly nearer supports.
   *Revised 2026-10-27 (arms retune, step 2b):* the scale decision above is superseded. The scout canvas is now
   104 wide (`size = {104, 104}`, art shifted 8 right so the body keeps its place against the frame centre), upper arm +
   forearm 21+19 (gun) and 27+25 (free); the shoulders stay put, `hold` moved to (66,60), so the gun elbow bends ~103
   degrees at worst (was ~130). Every gun is back at `scale = 0.5`, pistol included. The free hand reaches a support at
   most 24 drawing units from the grip (reach, not taste, is the limit: grip + support must stay within the free arm's
   length at every aim, and the sweep is +-85); supports sit 24 forward and 3-7 below the barrel so the free fist shows
   under the handguard instead of hiding behind the gun. Palms are >= 12 game units apart at a level aim
   (`test_jetpack_gun_pose.lua`: MIN_GAP 12, scale >= 0.45, fold <= 115). Rifle handguard, shotgun forend and sniper fore-end were
   drawn back to meet the nearer support; the rocket lost its foregrip (the hand is the foregrip). `smoke_jetpack_guns.py`
   now reads `camera:view` and re-aims while the camera settles.
3. Firing animation + casings + tracers + flash per gun. Test 3.
   *Built 2026-10-27 (decisions):* `fire_fx.lua` is the pure timeline (`scripts/test_jetpack_fire.lua`): from a gun and the
   time since its shot, `recoil` kicks the gun back along the barrel by `recoil` x `scale` game units (exactly, at age 0) and
   raises the muzzle `clamp(0.7 x recoil, 1, 8)` degrees, both easing out `(1-u)^2` to 0 at `min(fire_interval, 0.15)`; the
   trigger is full for a quarter of that then released; the flash style comes from the stats (rocket kind: front puff +
   back-blast at `eject`; pellets > 1: wide; shot_speed >= 1200: long; else small), 0.05-0.12 s. Casings pop up and back in
   the gun's own frame (so facing left mirrors), spin, fall at 720, bounce once (second contact rests flat), fade over the
   last 0.4 of 1.5 s, cap 40; none for rockets; the shotgun's is a red shell. `rig.lua` `R.pose` takes `opts.kick` (pilot
   units) and `opts.climb` (radians): the grip target moves back and the arm lifts, so both hands follow; the body, head
   and facing keep the true aim. `pilot_art` `P.actor(a, time, pilot, team, gun, fx)` draws it and returns the muzzle/eject
   points; without `fx` the editors' `a.flash` previews work as before. `fx_art.lua` (game only) draws tracers (bullets, by
   speed), rockets with a smoke trail, casings and the rocket blast (fireball, ring, debris); `art.lua` now just wires
   them. Weapons records `a.fired_at` / `a.shot_n` per soldier and steps `w.casings`; the view spawns a shot's casing /
   back-blast smoke the first time it draws the soldier after it (only the rig knows the eject point), purely cosmetic.
   `W.effect` takes an optional blast `radius` (such an effect lives 0.55 s). Shots are drawn once 24 units clear of the
   soldier (they are born at his centre). **Smoke caveat:** a screenshot costs the virtual clock a frame or two, so frame-
   stepped samples are ~0.04 s apart and a 0.05 s flash is seen in the first sample only; `smoke_jetpack_fire.py` retries
   the shot up to 3 times, and `JETPACK_FIRE_SOFT=1` collects failures to look at the sheets (`shots/jetpack-fire-*.png`).
4. Gun stats in the model: damage, rate, spread, pellets, mag, reload, rocket splash. Tests 4, 6.
   *Built 2026-10-27 (decisions):* `World.new(map, guns)` takes a validated gun list (default the baked six). Each
   soldier's `a.gun` picks its stats; `weapons.lua` owns the rules (`W.pull` is the gated trigger, `W.shoot` just
   spawns the shot(s)). **Falloff:** `range` is where damage starts to fall; it falls linearly to 40% of full at
   1.5 x range, where the shot expires (so `range` is not a hard cap; the sniper flies 2100). Falloff is by distance
   flown to the target's hitbox. Rockets never fall off; they explode at `range`. **Splash:** `damage` at the centre,
   linear to 0 at the `splash` radius, measured to the soldier's centre; a direct hit adds the full direct damage,
   so a rocket kills a 100-hp target in one shot when it hits (80 + about 50 splash). Knockback is `W.knockback`
   (560 units/s) x the same factor, straight away from the centre (up when exactly on it). **Self-splash is allowed
   at full value** (rocket jumping); a blast spares the owner's teammates (a bot's rocket does not hurt other bots).
   **RNG:** one minstd generator per world (`w.rng`, seed `Weapons.seed`), exact in doubles, so the same fire
   sequence gives the same pellets in tests and on replay; spread is uniform in +-`spread`. **Ammo:** one round per
   trigger pull (a shotgun's pellets are one round); an empty mag auto reloads, `input.reload` (consumed per step)
   reloads early, no firing while reloading, a gun change or respawn refills the mag. **Semi:** once per press; a
   press inside the interval waits for it, then fires once. The fire clock keeps its remainder so the rate does
   not drift with the 1/120 step. **Bots:** their gun's interval, with a reaction delay (`Bots.reaction`, 0.3 s +
   0.1 per index) after the line clears, bursts of about half a second of fire, a pause between
   (`Bots.pause`), and `damage_scale` 0.5 of the gun's damage; `Bots.interval` is gone, `C.bot_interval` unused.
   HUD: `hud:ammo` (`12/12`) and `hud:reload` (`RELOADING`, only while reloading). `input.thrust` is now a
   boolean or 0..1 (force and fuel drain scale; below 0.64 the pack cannot beat gravity). Tests:
   `scripts/test_jetpack_weapons.lua`, `smoke_jetpack_combat.py`.
5. Weapon spawns (map editor places them, pools, respawn, death drops) + two-gun loadout + swap. Test 5.
   *Built 2026-10-27 (decisions):* **Loadout** = optional map key `loadout = { in hand, on the back }` (1-2 gun ids,
   validated by `F.map`); the default is **rifle + pistol** (Mini Militia starts a main gun and a sidearm), the arenas
   carry it explicitly, the yard does not. Everyone, bots included, starts and **respawns** with it, full mags (a respawn
   no longer keeps the last gun: `test_jetpack_weapons.lua` was adjusted). The `jetpack-loadout` asset stays a test seam
   (one gun id each, no second gun). **Soldiers** carry `a.gun` (in hand) and `a.other` (on the back, `a.other_ammo` its
   rounds, nil = never used = full); `swap` (button `button:swap`, key `Q`) toggles them, keeps each mag, cancels a
   reload and costs `Spawns.swap_time` = 0.3 s with no firing (`Weapons.pull` checks `swap_left`). **Weapon spawns**: a
   map's `weapon_spawns` are pads in `w.pads`; the gun is rolled from the `pool` when the match is built and again
   `respawn` seconds after it is taken. A pool's weights are repeats of an id (`{"rifle","rifle","smg"}` is 2:1); the
   roll uses the world RNG (`Weapons.random`, only drawn when a pool has two or more known guns), so a match is
   reproducible. **Loose guns** (`w.loose`, `Spawns.update_pickups`) lie 14 units above the surface (`Spawns.lie`);
   drops fall (gravity 900) and rest, pad guns hang where the author put them. **Pickup**: the nearest loose gun within
   44 x 40 units of the soldier's centre; a separate **pickup button** (`button:pickup`, key `E`) appears *beside* swap
   (its own slot, `Sticks.slots.pickup`, left of the swap slot) only while one is in reach, showing the gun's icon
   (`pickup_art.icon`), so swap stays usable over a gun; the HUD says `PICK UP <GUN>` (`hud:pickup`). Taking a gun puts
   it in hand and **drops the one that was in hand where the taken one lay** (with the back slot empty the old gun
   moves there instead). A spawn's gun is **full**; a dropped gun keeps the rounds it had (the design line "picked guns
   come full" is true of spawns only). A picked gun also costs the 0.3 s equip. **Death drops**: the gun in hand, with
   its rounds, where the soldier fell; **despawns after `Spawns.drop_ttl` = 20 s**; at most `Spawns.max_loose` = 12 dropped
   guns (the oldest goes first; pad guns never count). **Bots** take a loose gun within reach whose damage per second beats
   their own by 15 % (`Bots.better`; no rockets or zoom guns), and walk toward one in line of sight within 220 x 110
   (`Bots.seek`); the rule is deliberately dumb until step 8. **View**: `pickup_art.lua` (one call in `view.lua`) draws
   loose guns with `gun_art`, bobbing over a glow, the one in reach lit; an empty pad shows a ring that fills as its
   respawn nears. HUD: `hud:gun` (name), `hud:other` (`Q  PISTOL 12`) in a pill top-left, `hud:ammo` unchanged. **Camera**
   zones now include the swap and pickup slots. **Map editor**: tool `tool:weapon`; a spawn is a diamond (drag to move,
   `selected:delete`); the side panel shows a row per gun (`pool:<id>:minus|plus|count`, weight = how often it is
   listed, the last gun cannot be removed) and `weapon:respawn` (1-600 s); `jetpack_map_editor` now ships `baked/guns.lua`
   (bake writes it) and reads the `jetpack-guns` asset for names. Tests: `scripts/test_jetpack_pickups.lua` (spawn -> take ->
   respawn -> death drop -> despawn on the virtual clock, swap ammo, weighted determinism, cap, bots),
   `test_jetpack_format.lua` (loadout), `test_jetpack_controls.lua` (slots), `smoke_jetpack_pickups.py` (editor by pointer,
   then the game: prompt, button via `rects`, take, swap, `Q`/`E`, at 1200x900, 960x540 and 393x780).
6. Grenades + melee. Test 7.
7. Dual-stick controls, camera, bigger maps. Tests 8–9.
   *Built 2026-10-27 (deviations):* `camera.lua` is now a stateful pure module (`Camera.new(map)`, `:snap`, `:update(dt, w, h,
   px, py, aim, zoom)`, `:geometry`, `Camera.to_world`): the view fills the playfield, 210 world units along the short side
   (240 in portrait), so a 52-unit soldier is ~110-135 px on a phone-size landscape view and ~85 px in portrait; it eases at 8/s,
   leads 16 % / 14 % of the view toward the aim, holds the map edges, and a **safe band** (x 0.28-0.72, y 0.18-0.66 of the
   playfield) beats the edges: at a wall the view overscrolls just far enough to keep the soldier clear of the sticks, which is
   how "never under a control" holds at every map corner. `zoom` = the player's gun's `zoom` stat (>= 1) widens the view
   smoothly. The game publishes the camera in an invisible text `camera:view` ("x y scale") for smokes.
   `sticks.lua` (pure) holds the stick maths: the left stick walks (deadzone, 0.8 of the ring is full) and its push up is the
   throttle `0.5 + 0.5 * t^1.5` past a 0.12 deadzone (`t` = stick height 0..1), so half stick (0.5) is the hover throttle
   (gravity/thrust = 0.64) and full is a climb; the right stick aims past 0.2 of the ring and fires past 0.6 (a drawn ring).
   Sticks float (the base is drawn where the thumb landed) and are 110-150 px (0.3 of the short side). **The JET button is
   gone**: both thumbs are on the sticks, so a third-finger boolean button was the "hard to control" part. Slots for swap,
   grenade, melee, reload, zoom are reserved (`Sticks.slots`, right edge above the aim stick, outside the band), not drawn.
   Maps: `gen_maps.py` writes the three arenas plus the yard to `baked/maps.lua` (game and map editor, identical;
   `maps_next.lua` is gone); `F.map` accepts and validates optional `theme` and `weapon_spawns` (rules still ignore the
   latter). The game opens on the first map; the start and pause overlays carry map buttons (`map:pick:<id>`, `menu.lua`).
   `scenery.lua` draws each map's theme (`yard`, `cliffs`, `underground`, `city`): a world-anchored sky gradient, parallax
   silhouette layers that scroll slower on both axes, then the platforms; the area beyond the map is the theme's rock colour.
   Tests: `test_jetpack_camera.lua`, `test_jetpack_controls.lua`, the map lines of `test_jetpack_format.lua` /
   `test_jetpack_maps.lua`, and `smoke_jetpack.py` (two-finger flight, band + not-under-a-control checks through a scripted
   flight on every arena at 960x540 and 393x780, proportional-thrust ordering, map choice). The map editor still edits the
   yard only (`M.map_id`).
   *Revised 2026-10-27 (framing):* the safe band is gone as the hard rule. The camera is held to the map on both axes
   (no rock shown below the floor or above the ceiling) and rests the soldier low (anchor y 0.55) so the ground sits near
   the bottom; a loose band (x 0.12-0.88, y 0.14-0.86) only stops easing lag from losing him, and the map wins over it. The
   one hard rule is that the soldier's 52-unit box never overlaps a control zone (`Camera.zones`: both stick boxes and the
   reload slot): the camera slides sideways, overscrolling a wall only as far as needed, and starts easing there while he is
   still 0.3 of the height above the zone. In portrait this makes the left wall overscroll visibly on the floor (the gap
   between the sticks is ~137 px). Also: reload button (`button:reload`, the reserved reload slot) and `R`; the start overlay
   names the dual sticks; the portrait "rotate" hint is dropped and the status line wraps to two rows below 500 px.
   *Built 2026-10-27 (zoom levels, "like Mini Militia"):* the single `zoom` stat is a list **`zooms`** (first 1, strictly ascending, at
   most 6, each <= 4; `F.zooms_ok`, in all four `format.lua` copies); a v1 `zoom = z` upgrades to `{1, z}` (`{1}` when z <= 1) and
   `zooms` wins when both are present. Baked: pistol/SMG/shotgun `{1, 1.25}`, rifle/rocket `{1, 1.5, 2}`, sniper `{1, 2, 3, 4}`
   (`gen_guns.py`; the sniper starts at 1x now, its old fixed 1.6 is the scope's second level). A level > 1 **widens** the view by that factor.
   **Cap:** `Camera.max_zoom(w, h)` stops the view at the factor that keeps the soldier `Camera.min_px` = 30 px tall: about 3.8 at
   960x460, 2.84 on a 393 px wide phone upright, so the sniper's 3x and 4x look the same on that phone (the label still says 4x).
   **Camera:** zoom eases at `Camera.zoom_rate` 12/s (~95 % of a level change in 0.25 s, position still 8/s); the look-ahead grows by
   `Camera.lead_gain` 0.35 per unit of zoom past 1 so the extra view lies in front; the zoom slot joined `Camera.zones`, so the
   never-under-a-control sweep now runs at every level (and mid-ease). **Controls:** the zoom state is view state in `controls.lua`
   (`Sticks.zoom_new/track/step/label`, pure, tested in `test_jetpack_controls.lua`): `button:zoom` (slot under reload; `/Nx` under the level
   names the gun's widest; dimmed with one level), key `Z`, and the wheel on the playfield (`on_wheel`: down = wider, stops at the ends;
   the button wraps to 1). A gun change, a swap or pick-up (even of the same gun id: the swap clock jumps up) or a death puts the level back to 1.
   HUD `hud:zoom` (`2x`) beside the gun name while > 1. **Gun editor:** a "zoom levels" row (`zoom:add` / `zoom:remove`, number fields
   `stat:zoom:<n>` for levels 2+), refused with a notice unless the list is legal. **Skipped:** pinch (a playfield `on_touch` would turn the
   playfield into a touch surface and stop its synthesized pointer/hover aim on touch devices; not worth the risk here). **Deviation:**
   `bots.lua` read `stats.zoom` (leave "zoom guns" alone); one line now reads `zooms[#zooms] > 2` (the sniper only, as before).
   Tests: camera (levels, easing, 30 px at 4x, clear at every level, look-ahead), controls, format (v1 upgrade, validation), guns;
   `smoke_jetpack_pickups.py` (`run_sniper`: 1-2-3-4-wrap through `button:zoom` and `camera:view`; `run_rifle`: button, Z, wheel, wrap,
   swap resets, at 960x540 and 393x780); `smoke_jetpack_guns.py` (editor zoom rows). Shots: `shots/jetpack-zoom-*.png`.
   *Revised 2026-10-27 (landscape only, user: "it would be played in landscape mode"):* the game is Mini Militia-style landscape-only.
   A playfield taller than it is wide (`rotate.lua` `R.portrait`: `h > w`, so a desktop window can trigger it too) **pauses** a running
   match (`main.lua` `frame`, before the world advances), hides the scene, sticks, buttons, overlays and keys, and shows `rotate:card`
   (a drawn phone-to-landscape icon, "Rotate your phone to play") over the whole playfield; the bar keeps its readouts but loses
   pause/restart. The invisible `rotate:clock` ("State m.time") lets smokes see the clock stand still. Back in landscape the normal
   pause overlay shows (a Ready match shows the start overlay); resuming is the player's tap. **Removed:** the portrait view size
   (`Camera.units` is one number, 210; `max_zoom` has one branch), the HUD's below-500 px two-row status wrap, and the portrait smokes.
   **Kept/changed for landscape phones (780x360):** sticks stay 110 px (the clamp floor); the button slots start at y 10 and shrink
   from 44 to as little as 36 px so the whole column clears the aim stick (`Sticks.slots`); the pause overlay drops its flavour lines
   below 420 px tall (and the stick hints in a small window). Tests: camera/controls now sweep 780x360 (never under a control, sticks
   fit, slots clear the stick and are thumb-sized); `smoke_jetpack.py` plays at 960x540 and a 780x440 window (playfield 780x360) and
   has the portrait rows (393x780 from the start: card, no controls, Enter does nothing, clock fixed; a 400x460 window is landscape
   in the shell and portrait in fullscreen, which is how the smoke rotates mid-match: pauses, card, clock fixed, back -> pause overlay,
   still Paused, resume by tap); `smoke_jetpack_pickups.py` uses 780x440 where it used 393x780. **Follow-up: orientation lock.** The
   card only handles a phone that is already upright; an OS-level lock (Android `screenOrientation=sensorLandscape` / a runtime
   `window.orientation` request) is a separate runtime/shell feature, not attempted here.
   *Built 2026-10-27 (mouse fire and calmer mouse aim; user: "we also need mouse for firing ... mouse is too sensitive"):*
   **Cause, measured** (stock-lua sim of the old code, fixed cursor, 960x480): the aim -> look-ahead -> view -> world point of a
   fixed screen point loop is stable but slow: the aim kept drifting 1-4 degrees over the 1-2 s after the cursor stopped, and
   the angle is `1/r` per pixel, so near the soldier a 1 px move is ~1 degree and over him it is a spin; nothing smoothed it.
   **Fix:** the aim is the angle from the soldier's *screen* position (muzzle, `y - 4`) to the cursor (`Sticks.mouse_step`), with a
   dead radius of one soldier tall (52 units * scale) where it holds, and exponential smoothing `Sticks.mouse_tau` = 0.04 s
   (90 % of a step in 0.09 s). The camera look-ahead for the mouse comes from the cursor's screen position alone
   (`Camera.cursor_lead`, a vector of length <= 1 from the anchor to the cursor over half the view; `update`/`snap` take it as
   an optional `lead`), never from the aim, so the loop is cut. Sticks and keys still lead by the aim. The world's `input.pointer`
   path is no longer used by the game. **Fire:** the primary button held on the playfield (`on_hover` `e.down`: press = down after
   not-down, off every `Camera.zones` box, while Playing) sets `input.fire`; a click quicker than a world step is latched until a
   step runs (`after_advance`); the rules do semi/auto from the edges. **Touch:** the playfield now has a no-op `on_touch`, so a
   finger off the sticks is swallowed and cannot become a synthesized press/hover (this also stops touch from aiming by "hover";
   it does not cost the sticks, which sit above it; it does cost a future playfield pinch, noted above). **Crosshair:** `crosshair`
   frame at the cursor while the mouse steers, and `system_cursor=false` on the playfield then; hidden over controls. Right button:
   nothing (hover events carry the primary button only; the wheel already zooms). Hidden text `aim:angle` for smokes.
   Tests: `test_jetpack_controls.lua` (smoothing 90 % in 0.1 s, no overshoot, dead radius, aim settles < 1 degree over 0.5 s for six
   cursors, cursor lead bounded), `smoke_jetpack_combat.py::run_mouse` (rifle: held fires and keeps firing, release stops, quick
   click fires; pistol: once per click; buttons and a touch fire nothing; resting-cursor aim stable).

8. Bots upgrade. Test 10.
9. Sound (own plan doc). Test 11.
10. HUD and match flow. Test 12.
11. Full smokes, fresh review, Pixel upload.

## Gun format (v1, drafted 2026-10-26)

Provisional until step 2 validates it with the gun editor. Baked as `demo_apps/jetpack/baked/guns.lua`
(`scripts/gen_guns.py`, checked by `scripts/test_jetpack_guns.lua`).

`jetpack-guns` = `{ info = { version = 1 }, guns = [ gun ] }`, a gun =
`{ id, name, size = {w,h}, scale, vars, parts, points, stats }`.

- **Drawing** like a pilot: `size`, `vars` (own palette: `ink`, `metal`, `wood`/`grip`, `accent`, …),
  `parts = [{ id, pivot, shapes = [{ id, fill, path }] }]`, list order is draw order. `fill` is `#hex`
  or `@var`; paths use `move/line/quad/cubic/close` as `gfx.path` takes them, no drawing after `close`
  until the next `move`; shape ids are unique per gun. Drawn at 2× detail pointing right,
  `scale = 0.5` (game units per drawing unit).
- **Part ids:** `body` and `mag` required; `slide` or `bolt` (kicks back on fire), `barrel` optional.
  `mag` is the detachable magazine and drops during reload (shotgun: shell carrier; rocket: the
  warhead showing at the muzzle).
- **`points`** (drawing units, inside `size`): `grip` (palm of the gun hand), `support` (free hand;
  omitted = one-handed), `muzzle` (shots leave here), `eject` (casing port; the rocket's is its back-blast
  vent), `mag` (magazine well).
- **`stats`:** `damage` (per pellet), `fire_interval` (s), `auto` (hold to fire), `shot_speed` (units/s),
  `spread` (rad, half-angle), `pellets`, `mag_size`, `reload_time` (s), `range` (units before falloff),
  `recoil` (drawing units kicked back), `zooms` (camera factors, ascending from 1; v1 had one `zoom`), `kind` (`"bullet"|"rocket"`),
  `splash` (radius, rockets only), `melee` (gun-butt damage).

Balance against a 100-hp soldier (shots-to-kill at full damage; time-to-kill = (shots−1) × interval):

| gun | dmg | interval | auto | pellets | mag | reload | range | spread | STK | TTK | niche |
|---|---|---|---|---|---|---|---|---|---|---|---|
| pistol | 22 | 0.28 | no | 1 | 12 | 1.1 | 360 | 0.02 | 5 | 1.12 s | one-handed, precise, fast reload |
| smg | 10 | 0.07 | yes | 1 | 30 | 1.5 | 260 | 0.09 | 10 | 0.63 s | best close DPS, wild at range |
| rifle | 16 | 0.12 | yes | 1 | 30 | 1.9 | 520 | 0.035 | 7 | 0.72 s | all-rounder, long-ish range |
| shotgun | 14 | 0.90 | no | 8 | 6 | 2.4 | 170 | 0.14 | 1 point-blank (8×14), 2 at range | 0–0.9 s | close burst, useless far |
| sniper | 80 | 1.50 | no | 1 | 5 | 2.8 | 1400 | 0.002 | 2 | 1.5 s | range + scope 1/2/3/4x |
| rocket | 80 + splash 70 | 1.60 | no | 1 | 2 | 3.0 | 900 | 0.01 | 2 (1 with direct + splash) | 0–1.6 s | slow shell (480), area denial |

## Maps

Drafted 2026-10-26 (staging, not loaded by the game): `scripts/gen_maps.py` writes
`demo_apps/jetpack/baked/maps_next.lua` with three arenas, each larger than a 960×540 screen both ways:
**Cliffside Outpost** 4400×2000 (stepped cliffs, a spire, a high bridge), **The Catacombs** 4000×2200
(seven stacked levels joined by shafts, a double-height crypt, arches and mounds) and **Ruined Rooftops**
4800×2200 (towers, fire escapes, a ruined shell, a street in the alleys). Each has 3 player and 8 bot
spawns and 12–13 `weapon_spawns = [{id, x, y, pool, respawn}]` (a key `F.map` does not know yet — step 5
adds it). Reach arithmetic (theme.lua): net climb accel 1120−720 = 400, cruise 190/s, a full tank (100 at
32/s) lasts 3.1 s, so one tank climbs about 550; hops are kept to ≤ 420 up and ≤ 480 across.
`lua scripts/test_jetpack_maps.lua` checks format, geometry, spawns, weapon spawns and that every
platform is reachable from the player spawn and can get back (no traps); `--broken` shows it failing.
