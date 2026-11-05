# Multiplayer as a platform feature — proved by Jetpack

Status: **in progress — steps 1–7 and 9 built on branch `jetpack-mp` (2026-11-04).** Worktree `.claude/worktrees/jetpack-mp`, branch
`jetpack-mp` off `laptop-host` (a87ab8b, which already holds dev's Jetpack). Builds on
[laptop-host](laptop-host.md) sessions and, for step 10 only, [owned entries](owned-entries.md)
(uncommitted on `laptop-host` at writing). Absorbs the ephemeral half of
[group-chat-sync](group-chat-sync.md) §6 / step 10, which was never built.

A long-horizon, test-first plan: §0 lists the tests that prove it done; they are written first,
then §6's steps make them pass.

*Revision 2026-11-04 (same day, before any code): the first draft kept rollback in a per-game
`jetpack/lockstep.lua` and Jetpack's physics in Lua. The user decided multiplayer is a platform
feature: the platform runs the whole loop (`net.match`), rewinds `ui.world` too, and Jetpack moves
onto `ui.world` in this plan. §7 keeps what the first draft decided and why it changed.*

## 0. Done means these pass

**End-to-end (`scripts/e2e_mp.py`)**: two or three offscreen shells, real vaults, real Lua VMs,
real iroh over loopback (relay off), as `e2e_host.py` does. Inputs are injected over the bridge;
the virtual clock makes every frame number exact. Each test screenshots every peer to
`target/e2e-shots/M<n>-<peer>.png`; a step is judged done only after the images were looked at.

| # | scenario | proves | step |
|---|---|---|---|
| M1 | A, B, C open one app in A's space. A sends on a declared channel; B and C receive it with `from` = A (within 5 s wall-clock; *was* "3 frames", see step 4). No vault entry or doc changes on any peer or the host. An undeclared channel is an error in A's console | `net` channel, ephemeral | 4 |
| M2 | A sends faster than the manifest's rate; B receives at most the cap; A's console says `net: over rate` | host-side rate cap | 4 |
| M3 | B and C show A in `net.peers()`; C's shell is killed; A and B drop C within 2 s | presence | 4 |
| M4 | `demo_apps/mp_dots` (a `net.match` with no world): 10 s of scripted inputs on A and B; state hash equal on A, B, C at every 1 s checkpoint | `net.match` convergence | 5 |
| M5 | M4 with B's link delayed 120 ms (`OSVAULD_NET_DELAY_MS`): B's own dot moves on the frame of B's input; hashes still equal | rollback hides latency | 5 |
| M6 | Hockey as a two-player `net.match`: 20 s of play, the puck's path and the hashes equal on both peers, with and without the delay | world rollback | 6 |
| M7 | Jetpack single-player through `net.match`: `smoke_jetpack.py` green unchanged | the port loses nothing | 7 |
| M8 | Jetpack, A + B + C: a match starts with the same map, roster and pilots on all three even when B's workspace has other jetpack assets; 20 s of play, hashes equal, each pilot visible on the others | Jetpack multiplayer | 7 |
| M9 | B runs a modified Jetpack (no fuel drain). Within 1 s A and C flag B desynced and drop it; A and C play on. With only A and B the match ends `disputed` | cheating = desync | 9 |
| M10 | C's shell is killed mid-match; A and B show "waiting for C", then after 10 s end the match with no result | drop-out | 9 |
| M11 | A match ends. The `results` doc holds one entry: players, scores, a signature from every player. B writing a forged result over the bridge is refused by the host (`Rejected`) | central stats without a referee | 10 |
| M12 | Laptop and Pixel replay the same 60 s Jetpack input log: equal hash at every checkpoint. Then a live two-player match over n0's relay | cross-architecture determinism | 11 |

**Rust (`cargo test`)**: deterministic `math` matches a recorded table bit for bit; envelope
sign/verify and tamper refusal; manifest `channel` parsing; rate cap; relay fan-out to the right
sessions only; `net.match` (`Headless`): a rolled-back run equals a straight run, a state the
step mutates outside `state` is caught by the hash; world save → step → restore → step equals one
straight run, and the world hash is stable; the `signed_by` rule predicate.

**Smoke.** `scripts/smoke_mp.py` runs M1, M4, M6 and joins `SMOKES` at step 12.

## 1. What and why

Jetpack Arena is single-player against bots. Sync and permissions now run over iroh sessions, but
those carry only CRDT doc state; 120 Hz game state through Loro docs would be slow and would
bloat every vault. Multiplayer needs an ephemeral channel and a way for players to agree on a game
without trusting one of them — and it should be the platform's, so any app gets it by describing a
match, not by writing netcode.

## 2. Two trust routes — this plan builds the second

Decided with the user 2026-11-04:

- **Trusted route (applications):** a host decides, members propose. That is what sessions,
  rules and `Rejected` already are. A host-authoritative game mode is this route; not in this plan.
- **Participant route (games):** trust is between the players. Every peer computes the same game;
  cheating shows up as disagreement. No referee, so an appointed kunki host relays a match
  without running Lua.

## 3. `net.match` — the platform's loop

```lua
local match = net.match({
	id = "arena",
	tick = 1/120,
	frame = 4,                                   -- ticks per input frame (30 Hz on the wire)
	players = roster,                            -- from net.peers(), fixed for the match
	setup = { map = map, guns = guns },          -- sent by the starter; every peer plays this
	state = function(setup, players) return initial end,
	input = function() return my_input end,      -- plain table, read locally once per input frame
	step = function(state, inputs, events) ... end,
})
match.state      -- read-only view of the latest predicted state, for drawing
match.status     -- "lobby" | "playing" | "waiting" | "ended" | "disputed"
```

The platform (app_host, Rust) does: sending the local input, predicting missing ones (repeat
last), saving `state` (a deep copy; plain tables of numbers, strings, booleans) and every world the
match drives, rewinding to the earliest corrected frame and re-running `step`, the 1 s state hash,
desync and drop-out handling, and results. `step` is the game's rules; it runs many times per
frame during a rollback, so it must change nothing but `state` and the worlds it commands. That
rule cannot be enforced; the hash catches a break at run time, as a desync.

- Input delay 1 frame; rollback window 8 frames (~266 ms), beyond which the match waits.
- A single-player game is a one-player match: the same path, no network.
- The match runs on the frame clock, so offscreen tests drive it exactly with `rpc.frame`.

## 4. Determinism

Bit-equal results on x86_64 and aarch64:

- **Luau `math`**: `+ - * /`, `sqrt`, `floor` are IEEE and agree. `sin/cos/tan/atan/atan2/
  exp/log/pow` come from each platform's libm, so the platform replaces them, for every app, with
  the `libm` crate's software versions (no hardware intrinsics). An app never thinks about it.

  *As built (step 1, 2026-11-04), `app_host/src/dmath.rs`:* measured on arm64 macOS, 11 499 of
  72 000 sweep results differed from `libm` before the swap. The swap needs `math` compiled as a
  mutable global, or Luau's FASTCALL calls C `sin` without looking in the table (this costs every
  `math.*` call its fast path). `lerp` and `map` are replaced too, and `math.noise` is refused:
  all three are C++ arithmetic a compiler may fuse. **`%` was a surprise:** Luau computes it as
  `a - floor(a/b)*b` in C++, which clang fuses into one rounding on arm64 (3 881 of 4 000 float
  `%` results differed); `.cargo/config.toml` sets `CXXFLAGS=-ffp-contract=off`. Luau's build
  script does not watch `CXXFLAGS`, so an existing target dir needs `cargo clean -p mlua-sys`
  once. Because that flag can be stale or overridden, every VM probes its own `%` at build
  (`dmath::Agrees`) and `net.match` refuses to start on a VM that fuses. Not covered, per the
  guide: `^` (constant-folded and run with the platform's `pow`) and `string.format` of floats
  (`snprintf`). `vector` is refused (float32, native `atan2`); no app used it. `libm` is built
  with `force-soft-floats`, since another crate's defaults turn its `arch` feature on.
  Review (fresh `pi`, 2026-11-04) found the `^`, `vector`, PRELUDE-order and libm-feature gaps.
- **`pairs` order** is not part of the contract: `step` iterates arrays.
- **Content**: `setup` carries what would otherwise differ per device (workspace assets).
- **Rapier**: built with `enhanced-determinism` (Rapier's own cross-platform mode, slower). A
  match-driven world does not tick on frame time: the match steps it, `n` steps at a time.
- **Input**: values are quantized by the game (an aim angle as an integer unit) before `input`
  returns them.

## 5. `ui.world` under a match

- `driven = "arena"` on a `ui.world` hands its clock to that match: it steps only when the match
  steps, and is saved, restored and hashed with `state`.
- **Moments become events.** In a driven world, `on_hit`/`on_zone`/`on_timer` handlers are an
  error; what they would have reported arrives in `step`'s `events`, so a rollback replays them.
- Commands (`set`, `push`, `blast`) are allowed inside `step`.
- **Decided 2026-11-05 with the user: a driven world is a function of `state`.** A rewind re-runs
  frames, so a body made at frame 2 must exist at frame 2 of the re-run, not at the next screen
  frame. The match carries `worlds = { rink = function(state) return { entity, ... } end }`;
  the platform reconciles each driven world from it at the start of every match frame, re-runs
  included, and the view's `ui.world({ id = "rink", driven = "hockey", ... })` only shows it
  (entities there are an error). Rejected: `world:spawn`/`:remove` from `step` — cheaper, but a
  second, command-style way to make bodies that only matches would use.

Jetpack needs more of the world than hockey does. Gaps, read from `lua-apps.md` 2026-11-04:

| Jetpack needs | world today | step 7 adds |
|---|---|---|
| side-on flyer: gravity, thrust, analog move from the match input, slide on platforms | controllers are top-down walkers on local keys | a `body = "flyer"` driven by `set(id, { move, thrust })` from `step` |
| position and velocity of each soldier every tick, for bots and rules | `ray`, `at`; no read of an entity | `world(id):get(id)` → `{ pos, velocity, grounded }` |
| swept shots against cover and soldiers | `ray` | — (fits) |
| rockets, grenades, splash | loose bodies, `on_hit`, `blast` | — (fits, as events) |
| pickups in reach | sensors, `on_zone` | — (fits, as events) |
| rigged soldier aiming the gun at any angle | clips and attach; no per-frame pose from Lua | none: soldiers stay Lua-drawn frames placed from `match.state`, as `view.lua` does now; world entities carry colliders and no drawing |
| gun zoom | "Not yet: zoom" | none: Jetpack's own camera stays (drawing is Lua's) |
| platforms and soldiers as bodies with no world drawing (Lua draws them) | `EntitySpec.drawing` is required (`world/src/lib.rs`) | `drawing` optional when a `collider` is given |
| ground materials (ice, rubber platforms; grenades bounce off them) | `Material` only on loose bodies; fixed bodies are grip 0, bounce 0, combine Max | `material = { grip, bounce }` on fixed entities (hillclimb will use it for mud and ice) |

**Shared with hillclimb** (`docs/design/hillclimb.md`, agreed 2026-11-04 with that session):
hillclimb lands `world(id):get(id)` → `{ pos, rot, velocity, spin, tick }` first; this plan adds
`grounded`. Hillclimb's line colliders, wheel joints, motors and `loose.mass` all live in
Rapier's sets or the description, so a snapshot covers them. Restore must also rebuild the world's
own state outside Rapier: the joint id→handle map, entity↔body handles, timers, step carry.

## 6. Steps

1. **Deterministic `math`** for every app (`libm`, soft). Rust table test; full `smoke.py` green.
2. **Envelope and channels**: `courier::net` envelope `{ ws, item, channel, seq, from_device,
   body }` signed by the device key, so a host relays what it cannot forge; manifest
   `channel <name> { rate n/s size bytes }`. Rust tests.
   *As built 2026-11-04:* the `channel` grammar already existed (`send`, `rate`, `slot sender`,
   `app-permissions.md` §3); step 2 added `size N`, capped by `manifest::CHANNEL_SIZE_MAX`
   (16 KiB), which `courier::net` also enforces on seal and open. `seal` takes the device
   signature as a closure, so courier stays iroh-free. `net::Seen` refuses a replayed or older
   `seq` per sender, item and channel, since a signature proves who, not when.
3. **Transport**: a second QUIC stream per session (game traffic never queues behind a large doc
   update); the host — shell or kunki — relays to sessions on the same space with the item open
   and to its own app, checking membership and rate, never reading `body`; presence join/leave.
   M2, M3.
   *As built 2026-11-04 (host half):* `Frame::NetHello { attestation, ws_id, token }` opens the
   second stream, admitted like `Hello` with a held token; then `NetFrame`s — `Open`/`Close`
   an item, `Msg(Sealed)` both ways, `Here { item, attestation }`/`Gone { item, device }` for
   presence. `kunki::host::relay::Nets` keeps who is here on which item and relays only to them;
   each message passes `Nets::judge` — signature, item open on that stream, `courier::net::
   may_send` (device and space match the session, channel declared, `send` admits a role,
   `size`), `Seen`, then `Rate` (token bucket, a second's burst; no `rate` means
   `DEFAULT_RATE` = 60/s, not unlimited). Rules are read once per `Open` (`Admin::net_rules`).
   A channel with no `send` admits every member. Refusals are logged on the host, not told to
   the sender — the sender's own over-rate message comes from its local check (step 4).
   **M2 and M3 move to step 4**: they need Lua `net` to observe, and the member dial and
   the host's own app as a `Peer` are shell plumbing, also step 4.
4. **Lua `net`**: `net.send`, `net.on`, `net.peers` in app_host, shell2 plumbing (member dials
   the second stream after `Welcome`; the host shell's own app joins `Nets` as a peer). M1,
   M2, M3.
   *As built 2026-11-04:* `app_host/src/net.rs`. `net.send(channel, value)` (handlers only;
   returns `false` when over rate), `net.on(channel, fn(body, from))`, `net.peers()` →
   `{ id, account, me }` sorted by id, `id` the device key in hex. The channel is checked
   against the app's own `manifest.osv` (an undeclared one is an error), `size` too, and the
   same token bucket the host runs (`manifest::DEFAULT_CHANNEL_RATE`, now shared with
   `courier`) drops locally with `net: over rate on "x"` in the console. Values travel in a
   tagged binary form with sorted map keys, so equal values are equal bytes. Who is here and
   the outbox outlive a reload; `net.on` handlers are per VM. In the shell (`channels.rs`) the
   app thread hands its outbox over after each batch; the UI thread signs (`seq` = wall-clock
   µs, rising) and routes — the session's channel stream for a joined space, the process's own
   `Nets` for a hosted one (`Admin::own_net_rules`). **Found while building M3:** a killed
   shell sends no FIN and QUIC's idle timeout is 10 s, so the member sends `NetFrame::Beat`
   every 500 ms and the host closes a channel stream silent for 2 s. M1's "within 3 frames"
   became "within 5 s of wall-clock": the network does not wait for the virtual clock.
   `scripts/e2e_mp.py` M1–M3 green with `scripts/e2e_apps/net_probe`.
5. **`net.match`** without worlds, proved by `demo_apps/mp_dots`. M4, M5.
   **Player-cap sim** (added 2026-11-04 with the user): a Rust harness runs N matches in one
   process over a fake network (per-player delay, jitter, loss; no iroh) and records rollback
   rate and depth, stalled frames and ms per frame, for 2, 4, 6, 8 players on LAN, good wifi,
   and 4G with one bad link. Run on `mp_dots` here, on Jetpack at step 8, on the Pixel at
   step 11. The cap is picked from those numbers, not guessed.
   *As built 2026-11-04:* `app_host/src/net_match.rs`. A match is per VM; its traffic rides
   the channel named by its `id`, which must be declared with a `rate` covering the frame rate
   (checked at creation). Snapshots are `state` in `net`'s wire form, taken before every input
   frame; a late input that differs from what a frame ran on rewinds to that frame. Inputs
   before `DELAY` are nil for everyone. The starter is the lowest player id; its `setup` wins.
   A hello is answered once and resent every 0.5 s in the lobby, so match creation order does
   not matter. Checkpoints every ~1 s of frames, hashed (sha256, 8 bytes); a differing peer is
   listed in `match.desynced` (step 9 decides what follows). Rust: one-player, rolled-back ==
   straight, impure step caught, own input never predicted. `e2e_mp.py` M4, M5 green
   (`OSVAULD_NET_DELAY_MS` delays a member's channel stream both ways).
   **Sim, `mp_dots`, 20 s per row** (`cargo run --release -p app_host --example mp_sim`, nixos):

   | players | net | rollbacks/s | frames per rollback | stalls/s | frames run of 600 |
   |---|---|---|---|---|---|
   | 2 | 4G | 3.0 | 3.9 | 0 | 596 |
   | 4 | 4G | 6.6 | 4.0 | 0.3 | 596 |
   | 6 | 4G | 12.2 | 4.9 | 7.3 | 593 |
   | 8 | 4G | 14.6 | 4.7 | 6.8 | 593 |
   | 2 | 4G + 1 bad | 2.6 | 5.5 | 14.0 | 549 |
   | 8 | 4G + 1 bad | 12.5 | 4.5 | 34.8 | 480 |

   LAN and wifi: no stalls up to 8. Network-wise 4 is comfortable on 4G and 8 works on wifi; one
   bad link costs everyone 8–20% of frames. `mp_dots` costs < 0.3 ms per frame even at 8, so
   CPU says nothing yet — the cap waits on Jetpack (step 8) and the Pixel (step 11). Scripted
   inputs change every 15–20 frames; real players change more often, so more rollbacks.
6. **World under a match**: Rapier `enhanced-determinism`, `driven`, save/restore/hash, moments
   as events; hockey as a two-player match. M6.
   *As built 2026-11-05:* `world/src/snapshot.rs`. A snapshot clones Rapier whole (every set
   is `Clone`; the solver's warm-start contacts come along, so a restored world steps exactly as
   the original did) plus each entity's place, velocity, heading and steering, keyed by id, and
   who is in which zone or touching what. Restore expects the world already reconciled from the
   same frame's state; an entity spawned anew gets its old body back and its colliders re-tagged
   with the new owner. Reconcile now removes bodies in name order, so two devices hand out the
   same Rapier handles. `world(id):set(id, { move = { x, y } })` steers a controller in place of
   keys (`World2d::steer`). In app_host, `net.match{ worlds = { rink = fn(state) } }` makes
   each world driven: described at the start of every match frame, snapshotted with `state`,
   stepped `tick / world::STEP` steps after each `step`, its hits, zone changes and moves
   handed to the next `step` as `events` (`{ world, kind = "hit"|"zone"|"move", ... }`), its
   bytes (`World2d::digest`) in the checkpoint hash. A driven `ui.world` only shows: entities,
   gravity, wells, joints, actions and `on_*` moments there are errors, and commands outside
   `step` are refused. Clips play on the render clock (`show_at`) and report nothing. Timers
   are not driven yet — a match counts ticks in `state`. **`enhanced-determinism` is not on:**
   it turns on `simba/libm_force`, which `rapier3d` shares, and `smoke_marble_gates` then saw
   the 3D platform's drawn rotation and its physics rotation one f32 ulp apart. Whether x86_64
   and aarch64 agree on a driven world without it is measured at step 11 (M12) instead.
   `demo_apps/mp_hockey`; Rust: a restored world equals the straight run, a body gone since
   comes back, hockey's hashes equal across a rolled-back and a near-straight run and both peers.
   Ahead of Jetpack, two platform changes: the starter's `setup` travels in as many hellos as
   the channel's `size` needs (Jetpack's maps, guns and pilots are ~100 KB; a peer still short
   of it says so in its hello and is sent it again); and a one-player match starts the moment
   it is made, with no input delay — single-player Jetpack must feel exactly as before (M7).
7. **Jetpack in `net.match`, on its own Lua sim**: `World` (`world.lua`) becomes plain data —
   no cycles, static map and guns in `setup` — and its `step` the match's; the player's input
   is the match input; bots stay (every peer runs them). Lobby and roster in the HUD. M7, M8.
8. **Jetpack on `ui.world`**: flyer body, `:get` with `grounded`, optional drawings, fixed-body
   materials; `physics.lua` replaced by a driven world. Its own step after the multiplayer
   ones, or its own plan.

   *As built 2026-11-05 (step 7):* `world.lua`'s state is plain data — soldiers in `humans` and
   `bots`, a soldier's spawn an index into the map's spawns, a pad's gun naming its pad by index,
   a shot's owner by soldier id, teams by `a.team` — and what never changes in a match (map, guns,
   pilots, solids, the loadout seam) is the setup, reached through `World.meta`'s `__index`. A
   rewound state is a fresh copy, so the metatable goes back on before use. Presses (reload, swap,
   pick up, start/pause/resume) travel as counts, so an input frame applied on several ticks — or
   again after a rewind — acts once per press. Casings and back-blast smoke were spawned into the
   state by the view; they are the view's own now (`fx_art.lua`, aged on game time). Single-player
   is a one-player match: no delay, input every tick — all seven Jetpack smokes green unchanged
   (M7). Together: PLAY TOGETHER (shown when `net.peers()` has others) sends the roster and map on
   a `lobby` channel and every peer makes the match; 30 input frames a second; the starter's map,
   guns and pilots travel in its setup (~100 KB, in pieces); pause is off; humans are a team
   against the bots, each with their own pilot. `e2e_mp.py` M8 green: three shells, 20 s of key
   input, equal hashes, every pilot in every view. Not yet tested: B with different jetpack assets
   in its own workspace (the setup carries the content, so it should not matter).
   The stock-Lua tests (`scripts/test_jetpack_*.lua`, `scripts/run_jetpack_lua_tests.sh`) drive the
   new World through `scripts/jetpack_solo.lua`, the old single-player surface.

   *Revised 2026-11-05 with the user (before step 7 was started):* steps 7 and 8 were "world
   gaps for Jetpack" then "Jetpack on `ui.world` + `net.match`". Reading the code at step 6:
   Jetpack's game is already a deterministic pure-Lua step (box collision, a seeded minstd RNG
   in its state), so it runs in a match almost as-is, while the Rapier port is a large rewrite
   that changes the feel and risks M7's unchanged smokes. Multiplayer first; the port follows.
   The old text:

   > 7. **World gaps for Jetpack**: flyer body, `:get`. Rust tests, a world demo.
   > 8. **Jetpack on `ui.world` + `net.match`**: state and rules into `step`, `physics.lua`
   >    replaced by the world, lobby and roster in the HUD. M7, M8.
9. **Desync and drop-out**. M9, M10.
   *As built 2026-11-05:* `net_match.rs` keeps every player's hash per checkpoint and settles
   them in order once all counted players' are in. One hash everywhere: on. A strict majority:
   the others are out from `DROP_AFTER` (2) checkpoints later — every honest peer computes the
   same frame, and none runs past it before the verdict, so a drop rewinds at most to a frame
   still held. From that frame a dropped player's input is nil and holds nothing up. The odd one
   out plays to the same frame (the others still need its inputs until then) and ends itself:
   `ended = "desynced at frame f"`. No majority (two who disagree, or three all different):
   `disputed`, no result. Drop-out: stalled on a late input (or a missing hash) for 1 s the
   status reads `waiting` with `match.waiting` naming who; at 10 s the match ends, `ended =
   "dropped out: ids"`. Detection is at the first checkpoint after the cheat (~1 s of frames plus
   the network); the drop lands two checkpoints later. M9 runs on `mp_dots`, whose "cheat" button
   (a test seam) makes one device play by other rules — not Jetpack's fuel, as §0 says: the
   mechanism is the platform's and any game's state shows it. Jetpack shows a banner for
   waiting, ended and disputed. `e2e_mp.py` M9, M10 green; Rust: majority drop, dispute, drop-out.
10. **Results** (after owned-entries lands): each player signs `(match_id, result_hash)` over
    `net`; when all have, one entry goes to `results`; manifest
    `rule results/* { forbid insert unless signed_by(players) }`, checked by `courier::rules` in
    the writer's shell and on the host — the host can refuse a forged result, not make one. M11.
11. **Pixel**. M12.
12. **Docs and smoke**: `lua-apps.md` `net` and `net.match` sections, `status.md`,
    `architecture.md`, `smoke_mp.py` in `SMOKES`.

## Out of scope

Host-authoritative mode; joining a match in progress; direct peer-to-peer connections (all
traffic goes through the host); voice; matchmaking outside a space.

## Open questions

- Player cap: set from the step 5 sim (4 was a guess; bots up to 6 soldiers).
- Drop-out ends the match (M10). Continuing without C needs the others to agree on the frame C
  left — later?
- Rollback cost on the Pixel: Lua `step` × up to 32 ticks plus deterministic Rapier in one frame.
  Measured at step 6 before the window is fixed; if too slow, the window shrinks and input delay
  grows.

## 7. The first draft, superseded 2026-11-04

Rollback lived in Lua per game (`demo_apps/jetpack/lockstep.lua`), and Jetpack kept `physics.lua`
inside its step — because its physics is 64 lines of boxes against platforms, and a Lua table is
trivially saved and rewound while `ui.world` could not be (it steps on frame time, has no
save/restore, and its Rapier is not built deterministic). The user: multiplayer should be a
platform feature, so every world game gets it; the cost is §5's world work before Jetpack plays
online.
