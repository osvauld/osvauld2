# Multiplayer as a platform feature — proved by Jetpack

Status: **planned 2026-11-04, nothing built.** Worktree `.claude/worktrees/jetpack-mp`, branch
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
| M1 | A, B, C open one app in A's space. A sends on a declared channel; B and C receive it with `from` = A within 3 frames. No vault entry or doc changes on any peer or the host. An undeclared channel is an error in A's console | `net` channel, ephemeral | 4 |
| M2 | A sends faster than the manifest's rate; B receives at most the cap; A's console says `net: over rate` | host-side rate cap | 4 |
| M3 | B and C show A in `net.peers()`; C's shell is killed; A and B drop C within 2 s | presence | 4 |
| M4 | `demo_apps/mp_dots` (a `net.match` with no world): 10 s of scripted inputs on A and B; state hash equal on A, B, C at every 1 s checkpoint | `net.match` convergence | 5 |
| M5 | M4 with B's link delayed 120 ms (`OSVAULD_NET_DELAY_MS`): B's own dot moves on the frame of B's input; hashes still equal | rollback hides latency | 5 |
| M6 | Hockey as a two-player `net.match`: 20 s of play, the puck's path and the hashes equal on both peers, with and without the delay | world rollback | 6 |
| M7 | Jetpack single-player through `net.match` + `ui.world`: `smoke_jetpack.py` green unchanged | the port loses nothing | 8 |
| M8 | Jetpack, A + B + C: a match starts with the same map, roster and pilots on all three even when B's workspace has other jetpack assets; 20 s of play, hashes equal, each pilot visible on the others | Jetpack multiplayer | 8 |
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
5. **`net.match`** without worlds, proved by `demo_apps/mp_dots`. M4, M5.
6. **World under a match**: Rapier `enhanced-determinism`, `driven`, save/restore/hash, moments
   as events; hockey as a two-player match. M6.
7. **World gaps for Jetpack**: flyer body, `:get`. Rust tests, a world demo.
8. **Jetpack on `ui.world` + `net.match`**: state and rules into `step`, `physics.lua` replaced
   by the world, lobby and roster in the HUD. M7, M8.
9. **Desync and drop-out**. M9, M10.
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

- Player cap (proposal: 4 players, bots up to 6 soldiers).
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
