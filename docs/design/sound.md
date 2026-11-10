# Sound — a mixer, generated sounds, sound files

**Integration revision 2026-11-10:** Steps 1–2 are integrated into dev. Described loops, Hillclimb game sounds and file decoding remain unbuilt. Combined verification is recorded in [the integration plan](game-branch-integration.md).

Status: **plan agreed 2026-11-06; steps 1–2 built 2026-11-06 (U-1, U-2, S-1, S-2 pass), not
landed.** Found in step 2: a bus that has played nothing yet has no samples, so a capture pads
the time before its first sound with silence. Worktree
`.claude/worktrees/hillclimb`, branch `hillclimb`. Hillclimb is the first user; Jetpack's sound
step ([militia](jetpack-militia.md) §11) reuses all of it.

Scope (user, 2026-11-06): **files and generated sounds both.** A generated sound is a Lua table
of settings an agent writes (sfxr-style), like a map; a file is a `.wav`/`.ogg` from any pack.
Hillclimb's first pass: a **looping engine** whose pitch and volume follow the car, and
one-shots for **coin, fuel can, landing thud, crash**.

Today nothing in the shell makes a sound. `cpal` is already in the workspace (the `voice` POC).

## 0. End-to-end tests (written first, red until their step lands)

The offscreen shell has no sound card, so its mixer runs on the **virtual clock**: when a request
advances frames, the mixer renders that much audio into a buffer. Sound is then tested the way
pixels are: real samples from the real path (bridge → shell → Lua VM → mixer).

Two bridge requests (§1.6): `SoundLog {item_id}` (what started and stopped, when, at what pitch
and volume) and `SoundCapture {item_id, seconds}` (the last N seconds of the app's mix as a mono
48 kHz WAV).

`U` = `cargo test -p audio`; `S` = bridge smoke over an offscreen shell
(`scripts/smoke_sound.py`, `scripts/smoke_hillclimb.py`), added to `SMOKES` once green.

| id | proof | step |
|---|---|---|
| U-1 | a synth spec renders the same samples every time; a 440 Hz square has 440 ± 1 crossings per second; attack/sustain/decay put the peak and the silence where the spec says | 1 |
| U-2 | the mixer sums voices, applies volume and pitch (pitch 2 doubles the counted frequency), clips nothing past ±1 (a limiter), and a stopped voice fades over 10 ms rather than clicking | 1 |
| S-1 | a tiny app calls `sound.play(beep)` on click. After the click and 30 frames: the log has one `start beep`; the capture has energy where the click was and silence before it | 2 |
| S-2 | `sound.synth{…}` with an unknown field, or a bad value, puts an error naming the field on the console, like `ui.*` | 2 |
| S-3 | a looping sound described in the view (`ui.sound{id=…}`) plays while described; changing its `pitch` in the view changes the captured frequency; no longer describing it stops it (log `stop`). A hot reload that still describes it keeps **one** voice (no doubled engine) | 3 |
| S-4 | closing the app, or the app failing, silences it: the log stops every voice | 3 |
| S-5 | Hillclimb: idle, the engine plays; holding gas raises its captured frequency. A scripted drive logs `coin` for each coin taken, `fuel` for the can, `thud` on a hard landing, and `crash` when the run ends head first, with the engine stopped | 4 |
| S-6 | `sound.file("audio/x.ogg")` and `.wav` play through the same path; a missing or broken file is a console error naming it | 5 |

`cargo test --workspace` and every existing smoke stay green each step.

## 1. Decisions

1. **One new crate, `audio`**: mixer + synth + decoding, plain Rust, no Lua and no device. The
   shell owns the device (`cpal` output) and the clock; `app_host` installs the Lua `sound`
   global. Same split as the rest: the runtime never knows Lua.
2. **One mixer per shell, one bus per app.** Each open app gets a bus; closing or failing the app
   drops its bus. Apps run on their own threads, so they send commands (`play`, `set`, `stop`)
   over a channel; the mixer never calls into Lua.
3. **Two ways to make a sound, both handles** (like `gfx.drawing`):
   - `sound.synth{ wave = "square"|"saw"|"sine"|"noise", freq, slide, attack, sustain, decay,
     volume, … }` is checked strictly (unknown fields are errors). Agents write these. It is
     data, so it can live in a file like a map (`sounds.lua`).
   - `sound.file("audio/coin.ogg")` reads an app file. **This needs binary item files**, built
     but not landed in the `moon-rover` worktree ([blender import](blender-import.md) step 1).
     So it is the last step, after that lands.
4. **One-shots are calls; loops are described.** `sound.play(s, {volume, pitch})` from any
   handler (coin taken, crash) is fire-and-forget. A continuous sound is a view element,
   `ui.sound{ id = "engine", sound = s, pitch = p, volume = v }`. It is retained by id like
   `ui.world`, plays while it is described, and stops when it is no longer described. So the
   engine is just "describe it every frame with this pitch", which keeps behaviour in Lua and
   makes hot reload safe (S-3). `ui.sound` takes no space and draws nothing.
5. **Behaviour stays in Lua.** Rust renders waves, envelopes and mixes. What pitch an engine has
   at what speed, and when a landing counts as hard, is Hillclimb's Lua.
6. **Offscreen is exact.** The mixer has no clock of its own: the shell advances it, from the
   device callback when windowed, from `rpc.frame`/`rpc.advance` offscreen. So a capture after
   N frames is the same every run. `SoundLog` and `SoundCapture` are driver requests like
   `Frame`.
7. **Pitch is resampling** (a pitch of 2 plays twice as fast). That is right for engines and
   blips. Time-stretching is out of scope.

Out of scope: positional/stereo panning, music streaming, effects (reverb, filters beyond the
synth's own), mic input (that is `voice`).

## 2. Steps

1. **`audio` crate**: synth, mixer, limiter, fades. U-1, U-2.
2. **Shell + Lua one-shots**: device output (windowed), virtual-clock rendering (offscreen),
   per-app bus, `sound.synth`, `sound.play`, `SoundLog`/`SoundCapture`, Python client methods,
   `smoke_sound.py`. S-1, S-2. `lua-apps.md` gets a Sound section.
3. **Described loops**: `ui.sound`, retained by id, stop on undescribe, close and failure. S-3,
   S-4.
4. **Hillclimb sounds**: `sounds.lua` (engine, coin, fuel, thud, crash as synth specs written by
   an agent), engine pitch and volume from speed and gas, one-shots from the run's events, a
   hard-landing rule in `run.lua` (pure, with its stock-`lua` test). S-5.
5. **Sound files** (after binary item files land): `.wav` (`hound`) and `.ogg` (`lewton`)
   decoding, `sound.file`. S-6. Then Hillclimb can swap any synth for a CC0 file.

When it lands: `docs/status.md` gets a "Sound" entry and `lua-apps.md` documents `sound` and
`ui.sound`.
