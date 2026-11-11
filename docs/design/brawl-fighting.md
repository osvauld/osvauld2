# Brawl fighting: a dummy, then a CPU, and the jetpack as recovery

Status: **steps 1-6 built 2026-11-11 (branch `2d-characters`, not landed).** Step
notes in §5. The opponent order, the moveset,
the jetpack as recovery, the rigging scope and the 75% robot are the **user's decisions of
2026-11-11** (§1, *Agreed*). Everything else in §1 is *Proposed*. It follows [Brawl movement](brawl-movement.md)
(steps 1–5 built; its step 6, "a jetpack on the robot", is folded in here) and [Brawl maps](brawl-maps.md).
**No Rust** beyond `app_host/tests/brawl.rs`.

Why: the robot runs, jumps, dashes and wall jumps, and nothing can hit it. The user wants
Brawlhalla's core: light and heavy attacks, neutral/side/down on the ground and in the air, dodges with
invulnerability, damage percent, knockback that grows with percent, launch angles, hitstun, and ring-outs
for stocks. The jetpack is the recovery move. The first opponent is a training dummy that takes hits and
shows its percent and knockback. A simple CPU follows. Two players over the network come after this plan.

**The one idea:** an attack is **frame data plus hitboxes baked from its clip**. Both are plain data
indexed by `act_t`. The rules look up "which circles are live this tick" and never run forward kinematics.
So a hit is exact on every machine, cheap enough to replay under rollback, and in the same place as the
fist that is drawn. The dummy and the CPU are fighters in the same `net.match` state. The CPU's input is a
pure function of that state, computed inside `step`, so a rewind replays it too.

```
moves.lua (frame data) ─┐
clips/*.lua (author.lua)┴─ scripts/bake_brawl_moves.lua ─→ moves_baked.lua (circles per tick, angles, reach)
                                                              │
input / bot.lua ─→ fighter.lua ─┬─ combat.lua (moves, hits, knockback, dodge) ←┘
                                └─ jet.lua (ignite, hover, thrust, cut, fuel)
state ─→ look.lua (act → clip, flame level) ─→ the posed robot with its pack
```

## 0. End-to-end tests (written first, red until their step lands)

The kinds are as in the movement plan:
- `P`: stock `lua5.4` through `scripts/run_brawl_lua_tests.sh`.
- `L`: Luau through `app_host/tests/brawl.rs`.
- `S`: a smoke over an offscreen shell, in a new `scripts/smoke_brawl_fight.py`, added to `SMOKES` once
  green. `rpc.frame(n)` is 2n ticks.

Per-tick facts belong to `P`; `S` checks counts and readouts. Each fighter gets a hidden readout
`fight:state:<i>` holding `dmg, stocks, outs, act, act_t, hitlag, hitstun, inv, kbx, kby, fuel, jet_pow,
combo` and the last hit taken (`last_dmg, last_kb, last_angle, last_stun`). Numbers are printed with `%.17g`.

**Units and exactness.** As before: a tick is 1/120 s, +y is down, and every tunable is dyadic. Damage is a
whole number. Growth is per percent and dyadic (1/16, 5/64, …). A launch angle is a vector from a baked
table, rounded to 1/256 (P-H4), so every knockback number below is exact and the tests say `==`.

### Frame data and hitboxes (`moves.lua`, `moves_baked.lua`)

| id | proof | step |
|---|---|---|
| P-H1 | `moves.lua` is checked. Each move names a clip that exists. `startup + active + recovery` equals the clip's length × 120, and all three are even (one picture frame is 2 ticks). Hit windows lie inside `active`. `dmg` is an integer ≥ 1. `base` and `growth` are on the 1/64 grid. The angle is in the angle table. An unknown field is an error naming the move and the field | 1 |
| P-H2 | **baked = clips.** Running the bake again (`author.lua` FK on the clip at `⌊t/2⌋/60`, placed as `look.lua` places it: the planted sole for `plant` acts, the root pivot in the air, × `Look.SCALE`) gives the checked-in `moves_baked.lua` exactly. Every number is on the 1/8 grid. A clip edited without a rebake fails, naming the move and the tick | 3 |
| P-H3 | hitboxes are where the move says. Every active tick has ≥ 1 circle and no other tick has any. Side moves' circles are in front (`x − r > 0`, facing right). Down moves' circles are below the box's middle. `dheavy` has a circle on each side. The baked `reach` box of each move holds all its circles | 1, 3 |
| P-H4 | the angle table: each vector is within 1/256 of unit length and within 0.25° of its angle (the test uses trig; the rules never do) | 1 |
| P-H5 | the rule modules (`fighter, combat, jet, bot`) use no `math.sin/cos/tan/atan/sqrt/exp/random`, no `^`, no `pairs` and no clock (source grep). The platform's maths library can differ by an ulp between machines; `+ − × ÷` cannot | 1 |

### Combat (`combat.lua`, through `F.step`)

| id | proof | step |
|---|---|---|
| P-F1 | P-B1 holds for a two-fighter fight in every state the fight script reaches: plain data; two deep copies stepped with one input stay equal | 1 |
| P-F2 | **input picks the move.** On the ground, light alone / with x / with Down is `nlight / slight / dlight`, and heavy is `nheavy / sheavy / dheavy`. In the air: `nair / sair / dair`, and heavy is `shair / dhair`. Heavy alone or with Up in the air is `jet` (P-J1). Up counts as neutral. A side move turns to the held side on its first tick. A move lasts exactly its total ticks, then the fighter is `idle` or `fall` | 1 |
| P-F3 | **commitment.** During a move nothing new starts: no run, jump, dash or dodge. A ground move's `vx` is its `lunge` table exactly. An aerial keeps air drift and fast fall. Landing during an aerial ends it and gives `lag` for its `land_lag` ticks | 1 |
| P-F4 | **attack buffer.** A press within the last `atk_buffer` ticks of a move or of `lag` starts on the first free tick. A press earlier than that is dropped | 1 |
| P-F5 | **one hit, by the numbers.** Dummy at 0%, `nlight` connects on tick t: `dmg = 8`; both frozen for `hitlag = 4 + ⌊8/2⌋ = 8` ticks (positions and `act_t` unchanged); then `kb = 5 + 8/16 = 5.5` along 30° (0.8671875, 0.5), so `kbx = 4.76953125`, `kby = −2.75`, mirrored by the attacker's face; `hitstun = ⌊5.5 × 2⌋ = 11` | 1 |
| P-F6 | **once per window.** A circle that overlaps the box through a whole window hits once. `nair`'s two windows hit twice. One move hits two victims once each. A grounded victim hit at a downward angle is launched at its mirror (`360 − a`) | 1 |
| P-F7 | **knockback scales.** One `sheavy` on victims at 0/50/100/150% gives strictly increasing travel. The launch decays by `kb_decay` along its direction to exactly 0, never past. Gravity acts on the fighter's own `vy`. No input acts during hitstun. After hitstun, input acts while the launch still decays. Bounds the numbers must meet: `sheavy` from x = 800 KOs at 150% and not at 0%; `nlight` from the centre does not KO at 150%. The KO percent of every move from the centre and from the edge is printed for the step note | 1 |
| P-F8 | **trades.** Two moves whose circles reach each other's boxes on one tick both land: hits are gathered from the post-move state, then applied. Swapping the two fighters' indices gives the mirrored result | 1 |
| P-F9 | **dodge.** Spot dodge: invulnerable on ticks 3–22 only (a hit then is no damage and no hitlag), `dodge_cd` before the next one on the ground. Directional: 8/tick for 16 ticks (exactly 128), then × `dodge_keep`. In the air the inv ticks have no gravity. One air dodge per airtime, refreshed by landing and by being hit. The dash is untouched (P-M9 unchanged) | 2 |
| P-F10 | **ring-out and the match.** Blasted: stock − 1, `dmg = 0`, respawn invulnerable for `respawn_inv` ticks. When one fighter is left with stocks it is the winner, the others are `ko`, and Enter restarts everyone. The dummy has unlimited stocks (`stocks = −1`); it counts `outs` instead | 1 |
| P-F11 | **the dummy.** It has no input: it is launched, stunned, falls, lands and idles. It never attacks. Its `last_*` fields and `combo` (hits taken while in hitstun) are what P-F5 computed | 1 |
| P-F12 | **movement is unchanged.** P-M12's 1 200-tick script, run alone and again with a dummy at spawn 2, prints a fighter 1 `describe` line byte-identical to today's (`tick=1200 x=-759.5 y=-226 …`). Fighters pass through each other | 1 |
| P-F13 | **replay of a fight.** `script_fight.lua`: 1 800 ticks against the dummy using every move, both dodges and the jet. Copied through `U.wire` at 300, 777 and 1 234, then rewound to 777 and replayed, it equals the straight run. The final `fight:state` lines are printed for S-F3 | 1–4 |

### The jetpack (`jet.lua`)

| id | proof | step |
|---|---|---|
| P-J1 | **ignite** (heavy, neutral or Up, in the air, `fuel ≥ jet_cost`): `vy = −jet_kick` (−8) on the press tick, gravity on, act `jet` for 12 ticks. The rise over them is exactly **76.5**, and fuel drops by `jet_cost`. Its uppercut window hits by the P-F rules | 4 |
| P-J2 | **hover:** heavy still held, no direction. Gravity is off and `vy` approaches 0 by `jet_accel` (0.375): from −5 it reaches exactly 0 on the 14th hover tick and stays there. The peak from rest is exactly **107.375**. Fuel −1 a tick. Act `jet_hover`, `jet_pow = 1` | 4 |
| P-J3 | **climb and thrust:** Up held: `vy → −jet_rise` (−6), `jet_pow = 2`. Side held: `vx → x·jet_side`, act `jet_thrust`, facing follows | 4 |
| P-J4 | **the tank.** Held from full, the jet runs exactly `jet_fuel − jet_cost` (96) ticks after the ignite, then `jet_cut` for `cut_ticks`, then `fall`. No ignite while `fuel < jet_cost`, and no refill in the air. On the ground it refills `jet_refill` a tick (30 ticks from empty). Letting go cuts at once. A jump, dash or dodge press ends the jet without `jet_cut` | 4 |
| P-J5 | **a recovery, not flight.** A full tank's climb from rest gains at most **651.625**. From (edge + 300, 400) falling at `max_fall`, double jump plus jet lands on the stage. From (edge + 300, 1 000) with the double jump spent it does not | 4 |
| P-J6 | a hit during the jet stops it (hitstun, no `jet_cut`); the fuel left is kept | 4 |

### The CPU (`bot.lua`)

| id | proof | step |
|---|---|---|
| P-CPU1 | **pure and deterministic.** `Bot.input(state, i, rules)` reads only state and rules and returns an ordinary input table (counts that grow). The same state gives the same input, and so does a wire copy. Its memory (`bot` in its own fighter: counters, a MINSTD seed, the plan, the think timer, the delay ring) is plain data. P-H5's grep covers it | 5 |
| P-CPU2 | **it doesn't fall off.** Alone on the yard and on Twin Isles, 7 200 ticks from 8 starting points on the stage: no stock lost. From 4 points offstage within P-J5's reach it recovers | 5 |
| P-CPU3 | **it fights.** Against the dummy for 3 600 ticks it lands ≥ 20 hits with ≥ 6 different moves and rings the dummy out ≥ 1 time | 5 |
| P-CPU4 | **it waits.** It sees the opponent `bot_react` ticks late (the ring). A dodge answers an attack no sooner than `bot_react` ticks after the attack started | 5 |
| P-CPU5 | **replay.** Bot against bot and bot against an idle human, 3 600 ticks, wire copies and a rewind: equal to the straight run. The final lines are printed for S-F5 | 5 |

### The picture

| id | proof | step |
|---|---|---|
| P-A5 | `Look.ACTS` is total over the new acts: every move, `lag`, `dodge_spot`, `dodge_dir`, `hurt`, `jet`, `jet_hover`, `jet_thrust`, `jet_cut`. Until a move's clip exists it maps to a stand-in (step 1) | 1, 3 |
| P-A6 | attack clips: the length is the move's total / 120. Ground moves keep each boot within ±6 of the feet line at startup and recovery (P-A4's check). The blade shows only where §4 Q2 says | 3 |
| P-A7 | **the pack and the flame.** Every robot variant has `pack` on `body` and `flame` on `pack`. The flame is hidden at `jet_pow = 0`. Its `scale_y` is the clip's flicker × `FLAME[jet_pow]`. The frame's cache key carries `jet_pow`, so one state gives one frame. The number of cached frames is printed | 4 |
| P-X1 | (extended) `pack.lua`, `build.lua`, `author.lua` and every new clip are byte-identical with `character_lab`'s | 3–4 |
| L-1 | (extended) the app opens clean. Every new clip passes `gfx.clip` while warming (one or two a view), and the robots with the pack compile. The debug module load stays under 0.5 s (printed) | 3–4 |

### Through the shell (`scripts/smoke_brawl_fight.py`)

| id | proof | step |
|---|---|---|
| S-F1 | open: the mode button reads `Dummy`; two fighters, `hud:dmg:1` and `hud:dmg:2` read `0%`; the dummy stands at spawn 2. Screenshot | 1 |
| S-F2 | by keys: walk to the dummy, J: `fight:state:2` gives `dmg=8` and P-F5's `last_kb`. Side K raises it more. With F2 on, `hit:<i>` rects appear only on active ticks and `hurt:<i>` covers each box. Each move's active frame is shot to `shots/brawl/fight/` | 1, 3 |
| S-F3 | the hidden `script:fight` button runs `script_fight.lua` twice: the same `match:checks` both times, and final `fight:state` lines equal to P-F13's (Luau = stock Lua) | 1–4 |
| S-F4 | jet by keys: in the air, K held: `act=jet` then `jet_hover`, fuel falls, `look:state` shows `flame=1`; Up gives `flame=2`; letting go gives `jet_cut`; landing refills fuel to full. Shots `jet_ignite/hover/thrust/cut.png` | 4 |
| S-F5 | `vs:bot` with the keys idle: after 3 600 ticks the final lines equal P-CPU5's, and the human has lost ≥ 1 stock. Played to the end, `hud:ko` reads `CPU wins — press Enter` | 5 |
| S-F6 | ring-out: the dummy's percent set to 150 (hidden `dummy:pct`), `sheavy` near the edge: `outs` goes up by 1, the dummy respawns with `inv > 0` and `dmg=0` | 1 |
| S-F7 | cost: `measure_brawl_look.py --fight` (two robots, the fight script, release): app frame p50 under 4 ms; misses, blends and cache size printed | 6 |
| S-F8 | still green: `smoke_brawl` (its first click is the hidden `vs:none`; S-9's final line is unchanged), `smoke_brawl_maps`, `smoke_characters`, the Jetpack smokes | all |

**Checked by eye:** each new clip's contact sheet with its hitboxes drawn (§3, the loop); the feel in play.

## 1. Decisions

*Agreed (the user, 2026-11-11):*
- A. **Opponents:** a training dummy first, then a simple CPU in Lua. The network two-player game comes after.
- B. **Moveset:** Brawlhalla's core: light and heavy × neutral/side/down on the ground and in the air;
  spot and directional dodges with invulnerability; damage %; knockback that scales with %; launch
  angles; hitstun; blast zones and stocks. **The jetpack is the recovery move.**
- C. **Rigging here:** the pack on the robot's torso bone, a flame scaled by thrust, clips for ignite,
  hover, thrust and cut-off, and a tank that drains in the air and refills on the ground. Hit reactions,
  knees and ledge grab are *after*.
- D. The robot is drawn at `Look.SCALE = 0.75` with a 60 × 156 body.

*Proposed:*
1. **Combat is Lua over plain state inside `step`** (`combat.lua`, `jet.lua`, `bot.lua`), the movement
   plan's decisions 1–2 kept whole: arrays not `pairs`, no clock, no randomness outside state, stock Lua 5.4
   syntax. The hit test (circle against box) is Lua for now. When the geometry module of the movement plan's
   §1a lands, its overlap query takes it over: Rust answers "what touches", Lua decides what a hit does.
2. **Hitboxes are baked from the clips at authoring time.** `moves.lua` names each window's part, offset
   and radius. `scripts/bake_brawl_moves.lua` (stock Lua, `author.lua`'s FK) writes `moves_baked.lua`: per
   move, per tick, the circles relative to the feet facing right, on the 1/8 grid, plus the angle table and
   each move's `reach`. P-H2 fails when a clip and its bake disagree. *Rejected:* FK in `step` (trig from the
   maths library, ~50 tables a pose, multiplied by every rollback re-run); FK at load into `setup` (it would
   agree across peers, but it costs the debug load budget); hand-placed static circles (they drift from
   the picture). Step 1 bakes static circles through the same file, so the format is fixed from day one.
3. **The hurtbox is the collision box.** *Rejected for now:* hurt circles per part, which would be closer
   to the picture but double the bake. Revisit with hit reactions.
4. **The knockback model.** `kb = base + growth × dmg_after`. The launch is its own velocity `(kbx, kby)`,
   decaying by `kb_decay` along its direction, and it adds to the fighter's own velocity, on which gravity
   acts. `hitstun = ⌊kb × hitstun_k⌋`, and `hitlag = 4 + ⌊dmg/2⌋` freezes both fighters. No DI, teching,
   staling or weight in this plan. *Rejected:* Smash's full formula (too many knobs before anyone has played
   it); trig at runtime (see 2).
5. **Keys:** J light, K heavy (held: jet), L dodge; **Shift alone dashes** (K moved to heavy). The
   direction held at the press picks the move. Up counts as neutral, since there are no up moves. Presses
   are counts (`light_n, heavy_n, dodge_n`), and `heavy` is held. *Rejected:* the mouse (the keyboard is the
   PC plan).
6. **The dodge is its own key; the dash stays.** Spot or directional, inv windows, a ground cooldown, one
   air dodge per airtime (refreshed by landing and by being hit). *Rejected:* merging dash into dodge as
   Brawlhalla does. It is fewer keys, but it rewrites P-M9/M12 and the movement the user has tuned (§4 Q1).
7. **The jet is the heavy aerial with no direction or Up.** The ignite is a rising uppercut that hits.
   Holding heavy hovers, climbs or thrusts, and the fuel is in state. It can ignite again while
   `fuel ≥ jet_cost`, and the tank refills only on the ground. That is Brawlhalla's slot (its neutral heavy
   air is the recovery) with the user's tank. *Rejected:* a key of its own (a sixth action key); holding Jump
   once the double jump is spent (it fights the jump cut and buffer); Jetpack's proportional throttle.
   Starting numbers come from Jetpack's thrust/gravity ratio (1120/720 ≈ 1.5 → `jet_accel` 3/8 against
   `gravity` 1/4) and its rise/run ratio.
8. **The pack is part of the robot drawing.** `characters/robot/pack.lua` holds `pack` (parent `body`,
   drawn before `slot:body`, so it sits behind the chassis) and `flame` (parent `pack`, at the nozzle):
   ids, pivots and shapes. `build.lua` adds them to every variant and `author.lua` lets clips key them. The
   flame's size is a **view override** from state: after `clip:at`, `flame.scale_y ×= FLAME[jet_pow]`, and it
   is hidden at 0. `jet_pow` joins the cache key. *Rejected:* one flame clip per thrust level (four times
   the clips); a separate flame frame placed by FK (two elements, and the pointer loses the part).
9. **The dummy and the CPU are fighters with `ctl = "human" | "dummy" | "bot"`.** The cast is in `setup`.
   A bot's input is `Bot.input(state_at_tick_start, i, rules)` inside `F.step`, before any fighter moves,
   so the order doesn't matter. *Rejected:* a bot as a device's input source. It would be recorded like a
   player's, but it needs a seat in `ids`, reads the predicted state, and can't be tested in `P`.
10. **The CPU's brain** is deterministic and small, and it re-plans every `bot_think` (10) ticks from what it
    saw `bot_react` (12) ticks ago (a ring in its state):
    - offstage, it recovers: toward the nearest ledge, double jump, then jet up and in;
    - when a seen attack will reach it, it dodges, with odds from its MINSTD draw
      (`seed = seed × 48271 % 2147483647`, exact in doubles);
    - when the target is inside a move's baked `reach`, it uses a light, or a heavy once the target is
      past `kill_pct`;
    - otherwise it closes to `spacing` and keeps `edge_margin` from a ledge.
    Stage spans are derived once in `F.rules`. There is one level, tuned in `tune.lua`.
11. **Hits are gathered, then applied.** After every fighter moves, in index order: all live circles
    against all boxes (not your own, not `inv`, not already hit by this window), then every hit at once.
    So trades are symmetric. Fighters pass through each other, and it is free-for-all.
12. **The picture:** every move, both dodges and the four jet acts get their own clips. `hurt` borrows
    `apex` (hit reactions are after). Lights are fists and boots; heavies draw the energy blade
    (`slot:sword_forward` shown, the sheath hidden) unless §4 Q2 says otherwise.
13. **Load budget.** Clips beyond the movement set are built **one or two a view** (`L:warm`), as the robots
    are. Rules never wait on them because the hitboxes are baked; an unwarmed clip draws its stand-in for a
    frame or two. `look:loaded` counts them for the smokes.
14. **The HUD:**
    - percent and stocks per fighter along the bottom;
    - the training line: the dummy's last hit (`dmg kb angle stun`) and the combo;
    - a visible mode button `Solo / Dummy / CPU`, defaulting to Dummy;
    - the hidden `vs:none`, `vs:dummy`, `vs:bot`, `dummy:pct` and `script:fight`.
    F2's overlay adds `hurt:<i>` and live `hit:<i>`.

### Starting frame data (ticks at 120/s; `moves.lua`)

| move | input | startup | active | recovery | total | dmg | base | growth /% | angle° | rides | extra |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `nlight` | gnd J | 10 | 6 | 20 | 36 | 8 | 5 | 1/16 | 30 | `palm_front` | jab |
| `slight` | gnd J+side | 12 | 6 | 24 | 42 | 10 | 5.5 | 1/16 | 25 | `palm_front` | lunge 6/tick, ticks 1–12 |
| `dlight` | gnd J+down | 10 | 6 | 22 | 38 | 8 | 4.5 | 1/16 | 15 | `hoof_front` | low sweep, lunge 4, 1–8 |
| `nheavy` | gnd K | 22 | 8 | 36 | 66 | 18 | 7 | 1/8 | 80 | `palm_back` (blade) | overhead arc |
| `sheavy` | gnd K+side | 24 | 8 | 40 | 72 | 20 | 7 | 1/8 | 40 | `palm_back` (blade) | lunge 8, ticks 12–30 |
| `dheavy` | gnd K+down | 20 | 10 | 40 | 70 | 16 | 6.5 | 7/64 | 60 | both palms, both sides | slam |
| `nair` | air J | 8 | 4 + 4 (9–12, 17–20) | 16 | 36 | 4 + 5 | 3 / 4.5 | 0 / 1/16 | 70 / 50 | both palms | two hits; land lag 6 |
| `sair` | air J+side | 10 | 6 | 20 | 36 | 9 | 5 | 5/64 | 35 | `hoof_front` | land lag 8 |
| `dair` | air J+down | 10 | 8 | 20 | 38 | 8 | 4 | 1/16 | 300 | both boots | land lag 10 |
| `jet` | air K / K+up | 4 | 8 (5–12) | → hover or cut | 12 | 9 | 6 | 5/64 | 85 | `palm_front` | `vy = −8` on press |
| `shair` | air K+side | 14 | 8 | 30 | 52 | 15 | 6.5 | 7/64 | 35 | `palm_back` (blade) | lunge 9, ticks 10–22; land lag 14 |
| `dhair` | air K+down | 14 (stall, `vy = 0`) | until landing (≤ 60) | land lag 18 | — | 14 | 6 | 3/32 | 300 | both boots | falls at 16/tick |
| `dodge_spot` | L | inv 3–22 | | | 30 | | | | | | ground cooldown 48 |
| `dodge_dir` | L+dir | inv 3–20 | | | 32 | | | | | | 8/tick × 16 = 128, then × 1/4 |

Angles run counter-clockwise from the attacker's facing, y up (300 is down and forward). The numbers in
`tune.lua`:

| tunable | value | meaning |
|---|---|---|
| `hitstun_k` | 2 | hitstun ticks per unit of `kb`, floored |
| `kb_decay` | 1/8 | launch speed lost per tick |
| `atk_buffer` | 8 | how many ticks early a press waits |
| `respawn_inv` | 120 | invulnerable ticks after a respawn |
| `dmg_max` | 999 | |
| `jet_kick` | 8 | ignite: `vy = −8` |
| `jet_cost` | 24 | fuel an ignite takes |
| `jet_fuel` | 120 | ticks of thrust in a full tank |
| `jet_accel` | 3/8 | how fast hover and climb reach their target `vy` |
| `jet_rise` | 6 | climb speed with Up held |
| `jet_side` | 8 | thrust speed with a side held |
| `jet_refill` | 4 | fuel back a tick on the ground |
| `cut_ticks` | 10 | `jet_cut` length |
| `bot_react` | 12 | ticks the bot sees late |
| `bot_think` | 10 | ticks between re-plans |
| `kill_pct` | 100 | from here the bot prefers heavies |
| `spacing` | 140 | distance the bot closes to |
| `edge_margin` | 120 | how close to a ledge the bot walks |

KO check by hand, which P-F7 pins: `sheavy` at 150% gives `kb = 7 + 170/8 = 28.25`, and its horizontal
travel is about 2 400. From the edge (x = 800) that passes the blast line at 2 100; at 0% it travels
about 280.

## 2. Where the code goes

- `demo_apps/brawl/` (new):
  - `combat.lua`, `jet.lua`, `bot.lua` (all pure);
  - `moves.lua` (the table above);
  - `moves_baked.lua` (generated; its header names the bake script);
  - `script_fight.lua`;
  - `hud.lua` (view).
- `demo_apps/brawl/` (changed):
  - `fighter.lua`: seams only. The cast, `ctl`, the hit phase after the moves, gates in the velocity block,
    `F.describe_fight`. `F.describe` is unchanged.
  - `tune.lua`: the numbers above.
  - `input.lua`: J/K/L, `heavy` held, Shift-only dash.
  - `look.lua`: the new acts, the flame override, clips warmed a few a view.
  - `main.lua`: modes, the second robot (dummy `brute`, CPU `volt`), the overlay, the readouts.
- `characters/robot/` (in both apps):
  - `pack.lua` (new);
  - `build.lua` and `author.lua` read `pack.lua`;
  - 17 new clips in `clips/`: 11 moves, 2 dodges, 4 jet.
  - All are authored in `character_lab` (listed in `cast.lua`'s `own`) and copied.
- `scripts/`:
  - new: `test_brawl_fight.lua`, `test_brawl_jet.lua`, `test_brawl_bot.lua`, `test_brawl_hitboxes.lua`,
    `bake_brawl_moves.lua` (`--dump` prints circles for the sheets), `smoke_brawl_fight.py` (into `SMOKES`);
  - changed: `brawl_test_util.lua` (`U.input` gains the counts, `U.fresh(cast)`), `clip_sheet.py`
    (`--hitboxes`, `--ticks`), `measure_brawl_look.py --fight`, `smoke_brawl.py` (clicks `vs:none` first);
  - `fixtures/brawl_yard_v1.lua` gains spawn 2. `maps/yard.lua` gets `{ x = 450, y = 0 }` with a dated
    note; spawn 1 is untouched, so P-M12 doesn't move.
- `app_host/tests/brawl.rs`: L-1's counts and the load gate.
- Docs: `status.md` per step; this doc's step notes; `lua-apps.md` gets the "a fighter drawn from match
  state" pattern (owed since movement step 4) plus "a deterministic bot inside `step`".

## 3. Steps

Every step keeps `cargo test`, every `P` test and S-F8 green.

1. **Hits on boxes** (P-H1, P-H3 static, P-H4, P-H5, P-F1–F8, P-F10–F13 without dodge and jet, P-A5
   with stand-ins, S-F1, S-F2's numbers, S-F3, S-F6). **Opus**, because it sets the pattern the network
   plan replays. It builds:
   - `moves.lua` with static circles baked through the real file format;
   - `combat.lua`: the move machine, the buffer, the hit phase, hitlag, knockback, hitstun, percent,
     stocks, the winner;
   - the dummy, the keys, `hud.lua` and the overlay;
   - attack acts drawn on stand-in clips (`idle` on the ground, `apex` in the air), so it is playable
     against the dummy at the end of the step.
2. **Dodges** (P-F9). Spot, directional and air, inv windows, the cooldown, the refresh on hit. **Sonnet:**
   every number is pinned.
3. **Attack clips and the bake** (P-H2, P-H3 baked, P-A6, P-X1, L-1, S-F2's shots). **Opus** writes
   `bake_brawl_moves.lua`, `clip_sheet.py --hitboxes` and the first two clips (`nlight`, `sheavy`). The
   other nine moves and two dodges go **one clip per Sonnet subagent**, each through the loop below.
4. **The pack and the jet** (P-J1–J6, P-A7, P-X1, L-1, S-F4). **Opus** does `pack.lua` and its threading
   through `build.lua` and `author.lua` (shared by two apps; S-8's mirror scan and the body measurement are
   re-checked), `jet.lua`, and the flame override. **Sonnet** does the four jet clips through the loop.
5. **The CPU** (P-CPU1–5, S-F5). `bot.lua`, the `CPU` mode, the end-of-match screen. **Opus.**
6. **Feel and cost** (S-F7). The user plays and retunes `tune.lua` and `moves.lua`. **Sonnet** rebakes,
   updates the printed KO table and measures. The step note records the KO percents, the cache size, the
   frame times and the debug load time.

**The clip-authoring loop** (steps 3–4, one clip at a time):
1. Write `character_lab/characters/robot/clips/<move>.lua` with `author.lua`, keys on the frame data's
   boundaries: wind-up at the end of startup, full extension through active, settle in recovery.
2. `lua scripts/bake_brawl_moves.lua`.
3. `OSVAULD_OFFSCREEN=1200x800 python3 scripts/clip_sheet.py --hitboxes --ticks 4 <move>`: a frame every 4
   ticks, numbered by tick, with the baked circles in red on active ticks, plus the feet and wall lines.
4. Look at the sheet and rework until P-H3 and P-A6 pass.
5. Copy into `brawl` (P-X1).
6. The user judges the sheet, then the move in play via S-F2's shot.

## 4. Questions for the user — answered 2026-11-11

1. **Dash and dodge:** kept separate (decision 6 *Agreed*): Shift dashes as tuned, L dodges.
2. **The blade:** heavies draw the energy blade, lights stay fists and boots (decision 12 *Agreed*).
3. **The jet's input:** heavy in the air with no direction or Up (decision 7 *Agreed*).

After this plan, each its own: **two players over the network** (Jetpack's lobby, `frame = 4`; `net_sim`
learns binary files and keys first); **hit reactions** (flinch and tumble clips, hurt circles per part);
**knees** (a jointed leg instead of the piston); **ledge grab**; DI and teching; charged heavies; weapon
pickups and throws; clashing hitboxes; CPU levels; moving the hit test onto the geometry module's
overlap query; effects (hit sparks, jet smoke); sound; gamepad.

Not in scope: couch multiplayer, Rapier for fighters, trig or FK in `step`, new Rust.

## 5. Step notes

### Step 1 landed 2026-11-11 (branch `2d-characters`, not merged)

Built: `moves.lua`, `moves_baked.lua` (generated), `combat.lua`, `checks.lua` (the stage check moved out
of `fighter.lua`, since it walks with `pairs`, plus the P-H1 move check), `hud.lua`, `script_fight.lua`;
`fighter.lua` seams (the cast and `ctl`, combat at the top of the tick, the launch added in `move_x`/
`move_y`, the blast counting `outs`, the hit phase and the winner in `F.step`, `F.describe_fight`);
`tune.lua` (`hitstun_k`, `hitlag_base`, `kb_decay`, `atk_buffer`, `respawn_inv`, `dmg_max`); `input.lua`
(J/K/L, `heavy` held, Shift-only dash, the training press); `look.lua` stand-ins; `main.lua` (modes, the
dummy drawn as `brute`, F2's `hurt:<i>`/`hit:<i>`, `fight:state:<i>`, the hidden `vs:none`, `vs:dummy`,
`dummy:pct`, `script:fight`); `maps/yard.lua` and `fixtures/brawl_yard_v1.lua` spawn 2 (450, 0).
Scripts: `bake_brawl_moves.lua`, `test_brawl_fight.lua`, `test_brawl_hitboxes.lua`, `smoke_brawl_fight.py`
(in `SMOKES`); `smoke_brawl.py` clicks `vs:none` first. No Rust.

- **Tests (red first: no `moves_baked`, no `combat`):** P-H1, P-H3 (static), P-H4, P-H5, P-A5, P-F1-F8,
  P-F10-F13 without dodge and jet; S-F1, S-F2's numbers, S-F3 (Luau's final `fight:state` lines equal stock
  Lua's), S-F6. The hitbox test also reruns the bake and compares it with the checked-in file (P-H2's
  step 1 half). Every earlier Lua test, `smoke_brawl`, `smoke_brawl_maps` and `smoke_characters` pass;
  P-M12's line is byte-identical alone and with the dummy (P-F12). `app_host/tests/brawl.rs` was **not
  run**: loadtable job 2886 ended "local stayed unreachable for 10m".
- **P-F5's hand numbers hold exactly:** dmg 8, hitlag 8 both, kb 5.5 along (0.8671875, 0.5), kbx
  4.76953125, kby −2.75, hitstun 11; the first unfrozen tick moves the dummy by exactly (4.76953125, −2.5).
- **P-F7, measured.** `sheavy` launch travel in x: **280.03 at 0 %** (plan: about 280), **2 454.9 at 150 %**
  (plan: about 2 400). A KO here means *the launch itself* carries the box out (blasted while `kb > 0`
  or in hitstun). A dummy launched off the ledge at 0 % still falls out later, because it can't recover;
  that is not counted. KO percent, victim at x 0 / x 800 (ground moves on the ground, aerials with both
  in the air at y −300): dair 338/131, dhair 196/74, dheavy 221/158, dlight 253/183, nair 374/277, nheavy
  182/182, nlight 305/221, sair 251/181, shair 157/107, **sheavy 133/89**, slight 239/204. The bounds
  hold (sheavy from 800 KOs at 150 %, not at 0 %; nlight from the centre does not KO at 150 %).
- **nair is not a true combo (measured).** Both windows hit (dmg 9), but the first hit's hitstun is
  ⌊3 × 2⌋ = 6 and its second window opens 8 move ticks later, so the combo counter reads 1. Retune in
  step 6 if it should string.
- **Choices the plan left open, each pinned by a test:** down beats a side held with it (down-side is
  `dlight`/`dair`); a side aerial turns too; the move tick is `act_t + 1`; a ground move's vx is 0
  outside its lunge (it stops a run dead) and goes on if it slides off a ledge; presses are consumed
  during hitlag, hitstun and moves (an attack press in the last `atk_buffer` ticks of a move or lag is
  kept with the direction held at the press; jump, dash, drop are dropped); a hit zeroes the victim's own
  velocity, dash, fast fall, jump buffer and coyote, cancels its move and makes it `hurt` at once
  (`act_t` 0, then frozen); `combo` is 1 on a fresh hit and counts on while in hitstun or hitlag; the
  attacker frozen this tick has no live circles; a wall stops the launch's x, the ground or a ceiling its
  y; nair's active span 9-20 has a gap, so P-H3's "active tick" is a tick inside a window; a spawn right
  of centre faces left (the dummy faces the player; spawn 1 is unchanged); Enter restarts once there is
  a winner as well as when ko. `dummy:pct` is an input press (`pct_n`, `pct`), so it replays.
- **The readout** `fight:state:<i>` is `x y face dmg stocks outs act act_t hitlag hitstun inv kbx kby combo
  last_*`; `fuel` and `jet_pow` join it in step 4.
- **The moves are not in `setup`**: `rules.moves` is the app's own `combat.lua` table, so two peers with
  different `moves.lua` would desync (the network plan should send a hash).
- **The fight script** starts all 11 moves; 10 hits land and one ring-out (shair at 95 %). It was tuned by
  hand against an open-loop timeline; any change to tune or moves will move its hits, but P-F13 only
  needs it to replay.
- **Picture:** attacks draw `idle` (ground) or `apex` (air) and `hurt` draws `apex`, so in play an attack
  shows no swing yet (step 3). The top-left stock row went (the bottom HUD has it); the training line sits
  on a dark pill (the grey text was unreadable over the tree). Shots: `shots/brawl/fight/`.
- **Open:** `lua-apps.md`'s "a deterministic bot inside `step`" is step 5's; the jet press (heavy in the
  air, neutral or Up) does nothing yet; L is counted and ignored until step 2.

### Step 2 landed 2026-11-11 (branch `2d-characters`, not merged)

Built, Lua only: `combat.lua` (`dodge`, `dk`, `ddir`, `dcd`, `adodges` in state; `begin` starts and counts
a dodge, `dodge_inv`, `dodge_steer`, `dodge_fall`; a hit ends it and refreshes the air dodge), `fighter.lua`
seams (the press is read, `busy` includes a dodge, steer and fall hooks, the act, landing refreshes), `tune.lua`
(`dodge_*`, `air_dodges`), `main.lua` (the robot's opacity is 0.4 while a dodge is invulnerable; shot
`shots/brawl/fight/s_f9_dodge_inv.png`). `script_fight.lua` gained an air spot dodge. Rust not touched, and
`app_host/tests/brawl.rs` was not run (the step adds no load).

- **Tests (red first):** P-F9 in `test_brawl_fight.lua`: inv exactly on dodge ticks 3-22 (spot) and 3-20
  (directional), a hit on tick 2 or 23 lands (8 dmg, hitlag 8, dodge ended), on tick 22 or 3 it does nothing
  and the window is not used up; 30 and 32 ticks long; the next ground dodge starts exactly 48 ticks after
  the last started (47 is dropped); directional x = 8k to exactly 128 at tick 16, vx 2 at tick 17, 131 at the
  end (both sides); no gravity on air inv ticks; one air dodge per airtime, landing and a hit refresh it; the
  dash is untouched; plain data and equal copies. S-F9 by keys (spot, cooldown, directional, shot).
  P-F13 now asserts the script starts a spot, a directional and an air dodge.
- **P-M12 byte-identical.** **P-F13's finals changed, only fighter 1's x:** 489.75 to 619.875 (the dummy's line
  is identical, same 10 hits, 1 out). The script's two dodges used to be ignored; now the directional dodge
  and the spot dodge's dead stop move fighter 1 before the final sheavy. The air dodge was added by
  splitting a 70-tick wait (the next press needed the landing, so that wait grew by 10 ticks).
- **Rules the plan left open, each pinned by a test:** a side held at the press makes it directional, else
  spot, in the air too (Up/Down are ignored; no diagonal air dodge); the facing is kept (a back dodge);
  a spot dodge stops dead (`vx = 0` all 30 ticks); a directional dodge runs `dodge_speed` on ticks 1-16, `vx` is
  `x dodge_keep` on tick 17 and friction does the rest (so 131 in all on the ground; wall stops it as it does
  a run); the cooldown is 48 ticks from the *start* of a ground dodge (18 free ticks after a spot dodge) and
  applies to both kinds on the ground; an air dodge starts no cooldown and the cooldown counts in hitstun and
  moves; a press during cooldown, a move, lag or hitstun is dropped (no dodge buffer); an attack press in the
  last `atk_buffer` ticks of a dodge starts after it, as for moves; jump, dash, drop do nothing in a dodge;
  a dodge press while dashing cancels the dash; the air dodge is spent when it starts and a dodge that lands
  keeps going on the ground; no gravity (`vy = 0`) only on the air inv ticks, so ticks 1-2 still fall;
  the invulnerability is the existing `inv` (set to 1 on each inv tick, never lowered below a respawn's);
  a hit on the non-inv ticks cancels it (`hurt`); a respawn clears all of it.
- **Picture:** `dodge_spot`/`dodge_dir` still draw `idle`/`dash` stand-ins; the robot fades while `inv > 0`
  in a dodge (not for a respawn's inv). Real clips are step 3's.
- **Not in `fight:state`:** `dcd`/`adodges` (kept out so P-F13's lines compare with step 1's).

### Step 3 landed 2026-11-11 (branch `2d-characters`, not merged)

Built, Lua only (plus L-1's counts in `app_host/tests/brawl.rs`): 13 clips in `character_lab/characters/robot/
clips/` copied to `brawl` (11 moves, `dodge_spot`, `dodge_dir`; listed in `cast.lua`'s `own`); `author.lua`
(both copies): an arm IK key's optional fifth value turns the hand, and the blade it holds, by that many
degrees from its drawn angle; `moves.lua`: a circle is `{ part, r, at }`, `at` a point of the setup drawing
on that part (`static` is gone; heavies ride `sword_forward`); `scripts/bake_brawl_moves.lua`: a module plus
CLI, FK on the clip at the frame move tick k shows, placed as `look.lua` places the robot (sole planted for
ground moves, root pivot for aerials, × `Look.SCALE`), `--dump MOVE`; `look.lua`: `Look.FIGHT`, each move and
dodge its own clip with a `stand` drawn until `L:warm` has built it (robots first, then one clip a view),
`look:loaded` is `clips=n/23 robots=n/3`; `clip_sheet.py --ticks K --hitboxes`; `script_fight.lua` retimed.

- **Tests (red first: no clips):** P-H1 (every move on its own clip, `static` refused, `at` required), P-H2
  (the bake in-process and as a script equals the file; sheavy's clip edited without a rebake reads "sheavy
  tick 25: the clip has moved since the bake"), P-H3 baked (plus every `at` inside its slot's setup bounds),
  P-A5 (no stand-ins in the table; `stand` is the old one), P-A6 (lengths; ground moves' boots within 6 of
  the feet line off their active frames, dodges on every frame, worst 4.78; the blade out and the back hilt
  hidden on every active frame of the five heavies, never in a light, an aerial light or a dodge), the look's
  stand-in and warming (`test_brawl_look.lua`), P-X1 over all 23 clips, S-F2's per-move shots
  (`shots/brawl/fight/s_f2_<move>_active.png`, `look:state` names the move's clip, `hit:1` drawn). L-1 run
  (loadtable job 2889, mac): `clips=23/23 robots=3/3`, `poses=2` (two robots since step 1; the test said 1,
  it had not been run since). The module load printed there is **1.13 s** in debug on the mac, not the
  plan's 0.5 s; this step adds no load-time work (the new clips are warmed), so the number is step 1's.
- **Unchanged:** P-M12 byte-identical; P-F5's hand numbers; the P-F7 KO table and travel (280.03 / 2 454.9)
  are identical, because the test stands each victim at the first circle (`contact`) and knockback does not
  read circles.
- **Changed, by the bake (first active tick, world units, facing right; old static → baked):** nlight
  (52, −96) → (57.1, −77.6); slight (60, −90) → (67.4, −78.3); dlight (50, −14) → (59.4, −12.4); nheavy
  (36, −150)/(70, −108) → (59.6, −214.4)/(49.9, −174.3), sweeping forward to the front over the window;
  sheavy (70, −90)/(110, −86) r22 → (84.1, −62.8)/(140.5, −63.1) r24 (shair alike), the far circle on the
  blade's tip third; dheavy (±56, −20) → (99.3, −25.1) blade / (−40.8, −32.5) fist; nair (±40, −90) →
  (52.9, −75.5)/(−50.3, −73.9); sair (54, −50) → (55.6, −41.5); dair/dhair boots now 19-22 apart
  (−1.6..20.5, y 9-10). Blade circles at `at` (295.1, 176.1) and (365, 148.8) for the side heavies, (318.4,
  167) and (369.7, 147) for nheavy/dheavy: a heavy's reach is now up to 170 in front (was 132).
- **P-F13's script, retimed deliberately:** with the baked circles sair, dair, shair and the ring-out
  missed (8 hits). The walk before sair went from 10 to 46 ticks (its wait from 70 to 34, so later segments
  keep their ticks): now **11 hits** (nair twice) and the ring-out on shair. Finals: fighter 1 x 619.875 →
  850.875; the dummy's `act_t` 44 → 84, `inv` 26 → 0, `last_kb` 18.53125 → 19.078125, `last_stun` 37 → 38
  (it took one more hit before shair).
- **Looked at by eye:** contact sheets of every clip with the circles (a stock-Lua SVG render of the bake's
  FK during authoring, then `clip_sheet.py --hitboxes`), and the shell's active frames of nlight (fist and
  circle on the far gripper), sheavy/shair (both circles on the lit blade), nair (one on each fist), dheavy
  (blade low in front, fist behind), dair (both boots below the box): circles on the striking part, boots on
  the floor in the ground moves.
- **Open:** the robot's legs are one-segment pistons: a long kick stretches the leg and shears the boot a
  little (dlight, sair were kept short for it; knees are after this plan). nheavy's first active ticks are
  above a standing box (the arc reaches the front from tick 25). `hurt` and `lag` still borrow `apex`/`idle`.

### Step 4 landed 2026-11-11 (branch `2d-characters`, not merged)

Built, Lua only (plus L-1's counts in `app_host/tests/brawl.rs`): `characters/robot/pack.lua` (both apps,
byte-identical): `pack` on `body` and `flame` on `pack`, inserted just before `slot:body` (flame, pack, chassis),
shapes from `build.lua`'s kit; `build.lua` adds them to every variant, `author.lua` lets clips key them. At rest
the flame is a stub inside the pack's outline, so a clip that doesn't key it (Paka's, in `character_lab`) never
shows it; a clip's `scale_y` stretches it out of the nozzle. `jet.lua` (pure, in P-H5's scan); `combat.lua`
(`pick` returns `jet`, `start` ignites, `begin` hands the end of the ignite to `Jet.begin`, a hit clears the jet);
`fighter.lua` seams (mode `jet`: `Jet.steer` for air steering, `Jet.lift` for gravity, `Jet.after` after the
landing check; the acts); `moves.lua` `jet` (rebaked: its fist circle rises from (2.25, −81.5) to (10, −120.75));
`tune.lua` the plan's eight numbers; `look.lua` (`Look.FLAME = {0.625, 1, 1.375}`, `Look.flame` over every pose,
cached or blended; the key carries `^jet_pow`); four clips `jet`, `jet_hover`, `jet_thrust` (loops, 0.2 s),
`jet_cut` (10 ticks); `main.lua` `look:state` gains `flame=`; HUD hint. `fight:state` gains `fuel` and `jet_pow`
(after `kby`).

- **Tests (red first):** `test_brawl_jet.lua` P-J1-J6; P-F2 (air heavy neutral/Up is `jet`); P-A5/P-A6 over the
  jet clips; P-A7 in `test_brawl_clips.lua` (parts and order in all three robots, no clip moves pack or flame
  off its parent, the resting flame inside the pack, the burning clips' flame at least 45 out of the nozzle and
  leaning at most 3° / 1° / 17° for ignite / hover / thrust) and `test_brawl_look.lua` (hidden at 0, flicker ×
  FLAME, the key, frame count); P-X1 over `pack.lua` and 27 clips; S-F4 by keys with shots
  `shots/brawl/fight/jet_ignite/hover/thrust/cut.png`.
- **The plan's numbers, exact:** ignite rise **76.5**, vy −5 after it; hover reaches 0 on hover tick 14, peak
  **107.375**; climb −5.375, −5.75, −6; thrust to ±8 by 0.375; **96** jet ticks from full then **10** cut ticks;
  refill full in **30** ground ticks; a held full-tank climb peaks at exactly **651.625** (ignites alone: 381.5).
  P-J5: from (edge + 300, 400) the double jump and the jet land on the stage in 112 ticks with 72 fuel left;
  from (edge + 300, 1 000) without the double jump it rings out. The uppercut: dmg 9, kb 6.703125 at 85°; its
  KO % (launch only) is 290 / 290.
- **Rules the plan left open, each pinned by a test:** the ignite is an ordinary air move (committed; its 12 ticks
  burn no fuel; land lag 6); fuel is a whole number, the ignite needs `fuel ≥ jet_cost`, a heavy press below
  that does nothing; the tank burns from the tick after the ignite while K is held and fuel > 0, one a tick;
  **the cut stops the rise** (`vy = max(vy, 0)` on its first tick, then gravity: that is what makes 651.625 the
  peak, not just the climb), and it is *free*: any press acts in it (a re-ignite, an aerial, a jump, the air
  dodge), and a jump, dash, landing, move or dodge ends it early; letting go of an ignite cuts after its 12
  ticks; climb is `jet_hover` with `jet_pow` 2, any side is `jet_thrust` (pow 2, Up adds the climb), Down is a
  hover (no fast fall); `jet_pow` 3 on the ignite, 0 in the cut; a light press in the jet ends it (no cut) and
  starts the aerial, as jump, dash and dodge presses do; landing ends the jet
  with no cut; refill on every grounded, unfrozen tick including the landing tick; respawn fills the tank; a
  hit keeps the fuel (the hit tick's burn is spent) and K still held does not relight.
- **P-M12 byte-identical; P-F5 and the KO table unchanged** (jet's row added). **P-F13 changed on purpose:**
  the script's jet segment was one tick of K (an ignite and a cut); it now holds K through the hover and a
  thrust back (`KH`, K held without a new press), lets go, then air-dodges in the cut. The segment keeps its 92
  ticks, so later segments start where they did; the thrust back moves fighter 1, so its final x is
  850.875 → **839**; the dummy's line is unchanged but for the new fields (11 hits, 1 out, as before). Both lines
  gain `fuel=120 jet_pow=0`. S-F3's Luau finals equal stock Lua's.
- **Looked at by eye:** an SVG render of the robot's FK over each jet clip while authoring, then S-F4's shots:
  the pack sits behind the chassis on the back in idle, air and mirrored (`step4_*.png`), the flame comes out of
  the nozzle and points down in the ignite and hover, back-and-down in the thrust (mirrored facing left), and
  is gone in the cut.
- **L-1 not run:** loadtable job 2890 (mac) waited ("mac is on battery at 28%"), then ended
  `interrupted → mac #199 (remote interrupted)` after 4 m with no log. Its counts are updated (27 clip files,
  `pack.lua` not a robot variant); S-F2's `look:loaded` reads `clips=27/27 robots=3/3` in the shell.
- **Cache:** the fight script draws 639 distinct frames (robot, clip frame, `jet_pow`, face; blends aside), 4
  with the flame.

### Step 5 landed 2026-11-11 (branch `2d-characters`, not merged)

Built, Lua only: `bot.lua` (pure, in P-H5's scan, now required: fighter, combat, jet, bot); `fighter.lua`
seams (`F.rules` adds `seed` (setup's, 1 by default), `spans` and `ground` from `Bot.stage`; a `bot` fighter
carries `f.bot`; `F.step` asks `Bot.input` for every bot before the restart check and before anyone moves);
`tune.lua` (`bot_react` 12, `bot_think` 10, `bot_dodge` 1/2, `kill_pct` 100, `spacing` 140, `edge_margin`
120); `hud.lua` (`CPU` names and mode; `CPU wins — press Enter`); `main.lua` (mode button Dummy → CPU → Solo,
the CPU drawn as `volt`, hidden `vs:bot`, `script:cpu` (keys idle, 3 600 ticks) and `vs:bots` (CPU vs CPU,
to watch recoveries)). Rust not touched.

- **Tests (red first: no `bot.lua`):** `test_brawl_bot.lua` P-CPU1-5; S-F5 in `smoke_brawl_fight.py`.
  P-CPU1: MINSTD's first five draws from 1 exactly; at every 150th tick of 2 400 bot-vs-bot ticks, two wire
  copies give equal inputs and equal memory after; the input has exactly the 13 fields of a key input; every
  count grows by 0 or 1. P-CPU2: alone, 8 starts × 7 200 ticks on the yard and on Twin Isles (ground,
  platforms, the boulder), no stock lost (it wanders: ~12 000 units, ~500 jumps per stage); back on the
  stage from (edge + 300, 400) and its mirror falling at `max_fall` in 145 ticks, (edge + 450, −200) in 110,
  (edge − 400, 100) in 86, and a fifth, spiked against the stage's side at y 450 with no double jump and 30
  fuel, in 186 (wall jumps). P-CPU4: with `bot_dodge` 1 it spot dodges **13** ticks after a human's sheavy
  starts with `bot_react` 12, **21** with 20; never with `bot_dodge` 0, nor against an attack out of reach.
- **Numbers vs the plan:** P-CPU3 vs the dummy, 3 600 ticks: **23 hits** (≥ 20), **9 moves** (≥ 6: slight 6,
  sheavy 4, dair/dheavy/dlight 3, nair/nheavy/nlight/sair 1), **4 ring-outs** (≥ 1). P-CPU5: bot vs bot 30
  hits, nobody out in 3 600 ticks; vs the idle human 12 hits, the human ko (0 stocks) and the CPU's 3 kept;
  the CPU wins at tick **2 000**. Copies at 900/1 777/2 500 and the rewind to 1 777 equal the straight run.
  Cost (stock Lua, this laptop): `Bot.input` **2.9 µs** a call; a bot-vs-bot tick about 23 µs.
- **S-F5:** the 3 600-tick comparison runs through the hidden `script:cpu` (a stop at 3 600, as `script:fight`
  has; a plain `vs:bot` match can't stop on a tick). The mode button reads CPU; `script:cpu`'s final `fight:state` lines equal stock Lua's P-CPU5
  (Luau = stock Lua) with the human at 0 stocks; `vs:bot` with the keys idle shows `CPU wins — press Enter`
  (by tick ~2 004 at frame granularity) and Enter restarts both at 3 stocks.
- **P-M12 byte-identical; P-F13's finals unchanged** (x 839, the dummy's line as step 4's); the dummy mode is
  as it was.
- **Rules the plan left open, each pinned by a test or listed here:** presses are the fighter's own `seen`
  count + 1 (no separate counters, so a restart needs none); each bot's seed is `(setup.seed + 7919 i) mod
  (2³¹ − 2) + 1`, and a restart reseeds (the same match again); the ring is filled with the first tick's
  view, so for the first 12 ticks it sees the start; what it sees of another is `x y vx vy face act move mk
  dmg inv grounded` (velocity with the launch), and it aims at the nearest one carried on by its seen
  velocity for `bot_react` ticks (straight line; grounded stays on its y); **the dodge is looked at every
  tick** (the ring is the delay), once per seen attack (keyed by its start tick and attacker), only when it
  can dodge now, the attack's last window not yet over by now, and the attack's reach (baked box plus lunge)
  covers its box; the dodge is spot on the ground, neutral in the air; **plans every `bot_think` ticks**:
  a move that reaches the predicted target now (heavy past `kill_pct`, before that a heavy one time in four
  if one fits, else a light), never at a target seen invulnerable, never a lunge off its top, never an
  aerial with nothing under it; neutral/down moves only when already facing it, side moves turn; else
  it walks to `spacing` × ½, ¾ or 1 from it (drawn), clamped `edge_margin` inside the ground, and stops
  within 32 of the goal (run friction slides ≤ 24.5); jumps to a target 120 above within 300, double
  jumps after it within 250, drops through a platform to one 60 below; a wall in the way is jumped;
  with no target it wanders (a random goal, a jump, a dash only if it ends 220 inside the margins, a
  short hop or a nair, a drop); **recovery is every tick**, from its own state, whenever it is airborne
  with no top under it: nothing is spent while a straight drift toward the nearest point inside the
  ground lands on it (a 240-tick simulation with gravity and air accel); else a wall jump off the
  stage's side (also out of a burning jet), the double jump (at `vy ≥ −2`), the jet (ignited neutral,
  Up and the side held, let go once the drift makes it), an air dash up and in (not on a wall, where
  the dash refreshes), the air dodge; under the stage it first hovers out sideways; `ground` is the
  widest run of tops touching at one height (yard −900..900; Twin Isles −1 100..1 100 with the bridge).
- **L-1 not run:** loadtable job 2891 (forwarded to mac) ended `interrupted`: "stayed busy with a live job of
  this checkout for 120s (sync failed: checkout d347815257fa2abd7fa22cce3ed96b21 has a live job on this peer)".
  The step adds no clip or robot file, so L-1's counts don't move.
- **Picture:** the CPU is `volt` (fighter 1 stays `scout`, also in `vs:bots`). Looked at by eye:
  `shots/brawl/fight/cpu_open.png`, `cpu_attack.png` (volt's blade on the scout, sheavy), `cpu_recover.png`
  (CPU vs CPU at 131/140 %: one jetting back from under stage height, the other swinging),
  `cpu_wins.png`.
- **Measured, not tuned:** the first bot-vs-bot run lost a stock to a spike (kby 12.6 down, no double
  jump): it burned its tank against the launch, then slid 400 down the wall with three wall jumps unused;
  the wall jump fixed it (the fifth P-CPU2 point). Two CPUs past 130 % rarely die now; retune in step 6.
  One level only (CPU levels are after this plan).

### Step 6 landed 2026-11-11 (branch `2d-characters`, not merged)

Lua and docs only. `moves.lua` growth retuned and rebaked (`moves_baked.lua`); `script_fight.lua` re-searched;
`test_brawl_bot.lua` P-CPU6; `test_brawl_fight.lua` nair combo assert; `smoke_brawl_fight.py` adapted;
`measure_brawl_look.py --fight` (S-F7). `tune.lua` untouched. The user's play-feel pass is still theirs.

- **Retune (old -> new), why:** two CPUs past 130 % rarely died (median KO 184 %, 18 KOs in 8 seeds x 7 200 ticks).
  Growth per percent: nheavy 1/8 -> 12/64, sheavy 1/8 -> 11/64, shair 7/64 -> 9/64, dheavy 7/64 -> 9/64, sair 5/64 -> 7/64;
  lights, dair, dhair, jet and nair's growth unchanged. Result: **28 KOs, median 140 %** (range 106-171); P-CPU6 pins
  >= 1 KO a seed and median <= 150. Chosen by sweep (kill_pct and bot_dodge barely move it; growth does).
- **nair chains:** first hit base 3 -> 4.5, so hitstun is floor(4.5 x 2) = 9 against a second window 8 move ticks
  later; combo counter 2 (asserted in P-F6). Its KO % is unchanged (374/276; the second hit sets the launch).
- **KO % table (centre / edge), old -> new:** nheavy 182/182 -> 127/127, sheavy 133/89 -> 92/59, shair 157/107 -> 119/80,
  dheavy 221/158 -> 168/119, sair 251/181 -> 177/127; the rest as before (dair 338/131, dhair 196/74, dlight 253/183,
  jet 290/290, nair 374/276, nlight 305/221, slight 239/204). sheavy travel 280.03 / 2 454.9 -> 337.64 / 2 892.7 (0 % / 150 %).
  Bounds still hold (sheavy from 800 KOs at 150 %, not at 0 %; nlight from the centre never).
- **Unchanged:** P-M12 byte-identical (movement not touched); P-F5's numbers (nlight growth untouched).
- **P-F13 changed:** the old timeline's dummy flew out of reach of later moves, so the script was re-searched (a Lua
  search that plays each move and keeps the first approach that lands): 9 hits (was 11), all 12 moves started, dummy
  not rung out (was 1). Finals: fighter 1 x 839 -> -41.875; dummy x -1530.17 dmg 106 (was step 4's line).
  P-CPU3: 22 hits, 8 moves, 4 outs (was 23/9/4); P-CPU5 final lines changed (CPU wins at tick 2351, was 2000); S-F3/S-F5 equal stock Lua.
- **Cost (S-F7, release shell offscreen, `--fight`, script:fight 1 800 ticks, two robots):** cold run 644 poses (mean 0.55 ms,
  max 1.53 ms), 281 blends; warm run app-thread view p50 **1.08 ms**, p95 2.41, max 5.11 (gate: p50 < 4 ms). Frame total p50
  5.0 ms is dominated by the host thread and render, not the app. Cache: 646 distinct frames, 5 with flame. Stock Lua:
  `Bot.input` 2.3-3.3 us a call; a bot-vs-bot `F.step` with two calls 18-28 us (rules + bot; draw is the view figure above).
- **Debug module load (plan: 0.5 s):** measured by cutting main.lua in the debug shell offscreen (open_item wall time on
  this laptop): an empty app 0.07-0.1 s, the manifest and an empty main 0.07 s; the full app 0.30 s (0.28-0.30 over
  repeated runs); all the repo's Lua `require`s together cost under 4 ms in stock Lua (fighter 1.7, map 0.9, combat 0.8,
  moves_baked 0.3, bot 0.3). The debug-shell cost here is the whole of `main.lua` module scope plus uploading and parsing 1.3 MB
  of files (art is 780 KB of it); removing the art folder from the app brings it to about 0.17 s. So on this laptop the load
  is **0.3 s, under budget**, and the 1.13 s on the mac was that machine's speed and battery state, not a module hotspot:
  nothing to fix in Lua. 
- **L-1:** job 2892 interrupted (exit 125, on mac); **job 2901 passed on nixos** (3 tests): `clips=27/27 robots=3/3`, `poses=2`, 18 pictures, 16 textures, 18.04 MiB, all pictures after 19 views. Its printed module time was **2.1 s** (debug, nixos worker; it times the whole `brawl()` helper: workspace, upload and open, not just module scope), 3.0 s to all pictures. The 0.5 s figure is not comparable across machines (laptop shell 0.3 s, mac 1.13 s, nixos 2.1 s), and no Lua hotspot explains it; a hard gate would need a fixed machine, so none was added.
- **Looked at:** `cpu_recover.png` (a CPU at 144 % beside a launched one), `cpu_attack.png`; no clip or picture changed.
