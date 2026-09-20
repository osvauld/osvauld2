# Six apps — proving the Lua layer is authorable (plan, 2026-09-19)

Status: **§4a landed, app 1 written** (`demo_apps/pomodoro`, 2026-09-19), `gap-log.md` open with
six entries. §7 was added 2026-09-20 and changes the near-term order: the harness is the blocker,
not the apps. **§7 step 1 landed 2026-09-20** — `shell2 --offscreen WxH` runs windowless with real
pixels and a driven virtual clock; step 2 (the pointer and time ops) is next. The six apps in §2
are still a proposal.

Companions: `gap-log.md` (the deliverable), `docs/lua-apps.md` (the author's contract these apps
are written against), `animation.md`, `runtime-rebuild-plan.md` (why).

---

## 0. Why now — what a study does not catch

Ten apps live in `demo_apps/`. Every one of them was written to exercise the primitive it was
built for, so every one of them passes. That is the failure mode: **a study validates the
feature it was made to validate, and is silent about everything else.**

The finding that prompted this plan was that the animation engine shipped (`animation.md` rev 4,
phases 0–D: property-keyed bindings, springs, `Driver::Press | Hover | Value`,
`runtime/src/el.rs:651–728`) and **no Lua app could reach it**.

**That finding was wrong, and its being wrong is the better argument.** Corrected same day:
`grep -rni "transition\|spring\|easing" app_host/src/` returns nothing, which is what it was read
from — but the props are registered through a `prop!` macro that takes the name as an *identifier*,
so no string-literal grep can see them. `props.rs:277` carries an `// animation` section:
`fade_in`, `fade`, `slide_in`, `tint`, `press_scale`, `scale`, with `on_faded_out` in `CALLBACKS`.
Lua apps can animate today.

What is actually missing there is narrower: no choice of easing curve (`Easing::Linear` and
`EaseInOut` are never constructed anywhere — the builders hardcode `EaseOut`), no duration control
on `press_scale` (fixed at 0.12s), and no general `on_done` — only `fade` has a completion
callback.

So the motivating claim was overstated by a host contributor reasoning about the bridge from the
host side, twice in one session (the other was comparing shape ids instead of the whole
`FrameHit`, which a unit test then failed to catch). **That is the case for §1.** Gaps inferred
from the host are wrong often enough to be worthless as a plan; gaps an app hits are facts. The
six apps are not a demo exercise, they are the measuring instrument.

`web-parity-backlog.md` records the same shape from the other side — "most gaps are not missing
capability; the primitive exists and isn't exposed."

## 1. The rule

**Real, agent-written, gap-logged.** All three carry weight:

- **Real** — an app one of us opens on purpose, more than once. Not a demo. The filter matters
  because a demo you never reopen only surfaces the gaps you already knew to look for; the
  second week of daily use is where the real ones are. If nobody would open it, cut it and pick
  another.
- **Agent-written** — the agent writes the Lua through the documented surface (`docs/lua-apps.md`),
  not the host contributor reaching around it. M2 is *agent-built* dashboards; the risk being
  retired here is whether the layer is **authorable**, which is a different question from whether
  it is capable. Hand-writing six apps answers the capability question we already know the answer
  to.
- **Gap-logged** — the apps are not the deliverable. §3 is the deliverable.

## 2. The six — provisional

| # | feature under test | app | known gap it should hit |
|---|---|---|---|
| 1 | timeline / timers | pomodoro or meeting timer | no readable monotonic clock, no wake-at-T (§4) |
| 2 | animation | kanban — exists, needs moving | no easing choice, no general `on_done` (§0) |
| 3 | drag + grabbed-shape | floorplan / diagram editor | shape-level hit already landed; wants snapping, constraints |
| 4 | zoom / pan at scale | map or large graph | culling, hit-test cost at depth |
| 5 | text + CRDT | notes / outliner | rich-text Lua surface (M2, plan §4) |
| 6 | data + charts | dashboard — **this one is M2** | `data.*`, Polars mirror, chart types |

Notes on the table:

- **#2 is nearly free.** The kanban app is already written and already pinned by the round-trip
  tests, but it lives in `shell2/src/kanban/` and is loaded by four `include_str!` paths in
  `app_host/src/tests.rs:2394–2401`. Moving it to `demo_apps/kanban` is mechanical and has been
  pending for a while. It earns its slot because animated reorder is the most natural place for
  the narrower gaps §0 ends on to bite: no easing choice, and no `on_done` to sequence with.
- **#6 is not a seventh thing.** It is M2 itself (target ~2026-10-17), approached as an app rather
  than as a subsystem. If the six compete for time, this is the one that cannot slip.
- Numbers 3–5 are the least certain and the most likely to be replaced by something either of us
  would actually use.

## 3. The gap log — the actual deliverable

One file, `docs/design/gap-log.md`, appended to as each app is written. One entry per wall hit,
written **at the moment it is hit**, before working around it. The working-around is the part
that destroys the evidence, which is why the entry comes first.

Each entry records:

- **What the app wanted** — in the app author's words, not the host's. "Dismiss this toast after
  three seconds."
- **What it had to do instead** — the workaround, concretely, with the line count it cost.
- **Whether the runtime can already do it.** Three verdicts, and the distribution is the finding:
  `no door` (capability exists, no Lua surface — §0's class), `missing` (nothing exists),
  `wrong shape` (exists and reachable, but the shape fights the app).
- **What it would take** — one line, honest, no estimate theatre.

A `no door` entry is a cheap fix and a bad smell — it means we shipped a thing and never wired it.
Counting those against `missing` at the end is the number that says whether the Lua layer is
behind the runtime, or the runtime is behind the ambition.

## 4. What this decides — the timeline question

A live design argument is **deliberately parked** behind these apps. Both shapes are worked out;
neither has evidence. From the 2026-09-19 discussion:

- **Deadlines in the model.** The app stores an absolute point on a monotonic timeline
  (`M.toast_until = e.t + 3`) and declares timers as data; the runtime diffs them like handlers and
  services them with `ControlFlow::WaitUntil` (today `lib.rs:1452` is a bare `Wait`). Reload-safe,
  because the deadline is in `_state`. Needs a stamp on click (`event_at` is already computed at
  `lib.rs:660, 976, 1055` and already rides `DragEvent.t` — click and hover just don't carry it).
- **Coroutines.** `wait(3)` suspends and the runtime resumes it when due — Roblox's `task.wait`.
  Far more expressive, and sequential time code is markedly easier for an agent to generate
  correctly than a deadline state machine, which cuts in its favour given §1. The blocker is that
  a suspended coroutine is live state outside `_state`: it cannot be serialized, so hot reload
  drops or duplicates it and it cannot ride the CRDT to a peer.

**The rule: do not design the timeline until apps 1 and 2 are written.** If three of the six want
sequential `wait`, coroutines win and the hot-reload cost is worth paying — possibly as the hybrid
where coroutines are session-local and rebuilt from model state on reload. If they all want a
deadline sitting in the model, the declarative shape holds. Right now both positions are guesses.

### 4a. Independent of that argument

One thing lands regardless, and is not blocked on the above:

**Luau already has a monotonic clock, and it is live in our sandbox.** `os.clock()` is
`clock_gettime(CLOCK_MONOTONIC)` (`luau/VM/src/lperf.cpp:50`), nanosecond-backed; `os` is never
stripped in `sandboxed_vm`. Probed 2026-09-19 through the real loader: `os.clock`, `os.time` and
`os.date` all present, ~6µs per call (the call overhead, not the resolution).

That is the *wrong* clock and it must be shadowed:

- It is real time, so it ignores the virtual offscreen clock — any app touching it makes its own
  tests machine-dependent and turns `Headless::advance` into a lie.
- Its epoch is machine uptime, unrelated to `e.t` / `FrameTick::elapsed` (seconds since app start).
  Mixing them silently yields a number in the hundreds of thousands.
- `os.time` / `os.date` leak wall time and locale past our own `now()`.

Shadowing the three in `sandboxed_vm` is a sandbox-integrity fix on its own terms, and it should
land before the apps are written rather than after one of them has quietly come to depend on
`os.clock`.

## 5. Sequencing

**Superseded 2026-09-20 by §7** — writing app 1 showed the harness costs more than the apps do.
The original order is kept below; steps 1 and 3 are done.

1. ~~**`os.*` shadow** (§4a)~~ — landed 2026-09-19.
2. **Move kanban** to `demo_apps/kanban`, fixing the four `include_str!` paths — mechanical, and
   it puts app #2 in place.
3. ~~**App 1**, with the gap log open from the first line.~~ — `demo_apps/pomodoro`, 2026-09-19.
4. **Read the log, then decide the timeline** (§4).
5. **App 6 in parallel from the start**, because it is M2 and M2 has a date.

Apps 3–5 are explicitly not scheduled here. Whether they survive contact with the log is the
point of the log.

## 6. What would change this plan

- If the gap log after two apps is mostly `no door`, the problem is bridge coverage and the answer
  is a parity sweep (`web-parity-backlog.md`), not six apps.
- If it is mostly `wrong shape`, the Lua contract itself needs revision, and that is a bigger and
  more interesting problem than any of the six.
- If the agent cannot write app 1 from `docs/lua-apps.md` without a host contributor reaching in,
  that is the M2 finding, and it arrives four weeks early — which is the best possible outcome of
  this plan and the reason §1 insists on agent-written.

## 7. One driver — the bridge, offscreen (2026-09-20)

Writing app 1 cost more in harness friction than in Lua. Half the gap log is tooling, and the
worst of it was mechanical: fifteen cold-start runs sweeping pixel coordinates to press a button
whose `id` the app had already given it.

### What we have

Two drivers, and the split between them is an accident rather than a design:

*The `window` row is what step 1 closed; the rest still stands. Kept as it was written.*

| | `open` (`app_host/examples/open.rs`) | the bridge (`scripts/`) |
|---|---|---|
| window | none | **always** — `shell2` has no offscreen mode |
| address an element | pixel coordinates | **by `id`** |
| pointer pipeline | **the real one** — hit tests, drags, hover | none: `Request::Click` calls the handler directly |
| session | one action list, then exit | **persistent REPL** (`drive.py`) |
| clock | virtual, exactly 1/60s a frame | real |
| sees | element tree, console | tree, console, doc data, **real pixels** |
| edit Lua live | no | **`write_file` + `reload_item`** |
| needs | the cargo workspace | a socket |

### The decision

**The bridge becomes the only driver, and `shell2` gains an offscreen mode.** Two arguments, and
the second is the one that settles it:

- The two things `open` uniquely has are not architectural, they are **ops nobody has written**.
  Pointer input is `on_cursor_moved` / `click` / `on_cursor_release` — the same methods `Headless`
  calls — behind a request. Virtual time is already in the Runner: `now()` returns `self.clock`
  when `offscreen.is_some()`, real time otherwise. Offscreen mode *is* the virtual clock.
- `open` needs the cargo workspace. An agent authoring apps against a shipped binary has a socket
  and no source, so **every hour spent on `open` is spent on a tool that stops existing** at the
  moment it would matter most. The bridge is already the planned agent surface (`mcp-bridge`).

### Driven, not free-running

The one design decision inside this, taken up front because it shapes the build: an offscreen
shell **paints when told**, the way `Headless::frame()` does — a `Frame` op advances the clock by
one frame, an `Advance` op jumps it. If the offscreen shell keeps its event loop running on real
time it is merely a window nobody can see, with none of the determinism, which is the worst of
both. Everything time-shaped that landed last week depends on this being right.

### Order

1. **`shell2` offscreen** — window optional, driven rather than free-running.
2. **Bridge ops** — `Pointer`, `Drag`, `Frame(n)`, `Advance(secs)`.
3. **Delete `open.rs`** — one commit, so there is never a window in which two drivers drift.

*Revised 2026-09-20: step 2 split into 2a/2b/2c — see "Step 2, and the seam it needed".*

Nothing leaves `open` before step 2 lands, or pointer and time coverage disappear in the gap. A
consequence worth stating: the `--advance SECS` flag that gap-log 1.5 asks for should **not** be
built. It is five lines into a file this section deletes.

### Step 1, as built (2026-09-20)

`shell2 --offscreen WxH`, `runtime::run_offscreen`, `Render::offscreen`. Smaller than expected in
two places and bounded in one.

**Offscreen keeps its pixels.** The plan assumed a windowless shell would be blind. It is not:
`Render::capture_scene` was already documented as never acquiring a surface frame, so the only
window-bound line in the whole constructor was `create_surface`. An offscreen `Render` is the same
device, renderer and vello target with `present` returning false — and `lib.rs`'s screenshot path
needed **no change at all**, because the fallback it already had for *surface loss* ("must not turn
a requested shot into the previous frame") routes a failed present through `capture_scene`. That
fallback was written for a different reason and turned out to be the offscreen path.

**Driven came for free.** No window means no `RedrawRequested`, so `request_redraw` is a no-op and
the loop only ever wakes for a user event. `user_event` paints one frame per delivered message and
advances the clock 1/60s, which preserves the bridge's own stated contract ("answering also
repaints", `bridge.rs`) rather than inventing a second one. Verified to the frame:
120 requests = 2.000s, the pomodoro reading 25:00 → 24:58 (`scripts/smoke_offscreen.py`).

**The bound: offscreen still needs a display server.** Probed, because it decides how far this
reaches: with `DISPLAY` and `WAYLAND_DISPLAY` both unset, `EventLoop::build()` fails outright —
*"neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set"*. So what landed is **windowless,
not headless**. It hides the window; it does not remove the dependency on a compositor being
reachable. `xvfb-run -a` covers CI as an ops workaround.

Removing it for real means the event loop goes too — a plain loop owning the `Runner` and the
socket, with `bridge::spawn`'s `EventLoopProxy<Msg>` generalized behind a small `Wake<M>` trait
(impls for the proxy and for an `mpsc::Sender`). Perhaps 150 lines. **Deliberately not done yet**,
and the reason it is safe to defer is that the two differ *only* in where `Msg` comes from: the
offscreen `Render`, the driven frame, and every bridge op in step 2 are identical under both. It is
a transport swap layered on this, not a rewrite of it. Do it when CI or a container needs it, which
is a real date but not this week's.

### Step 2, and the seam it needed (2026-09-20)

Step 2 was written as four ops. It is not: it is **one seam and then ops on top of it**, and finding
that out changed the order.

`Pointer` cannot be answered from `Shell::update` — an `App` gets `&mut self` and no `Runner`, so
nothing there can reach `on_cursor_moved`. Then the same wall turned up from the other side:
putting rects in `DumpTree` looked cheap and is not, because `DumpTree` calls `app.view().info()`
on a **fresh view with no layout at all**. Layout happens in `Runner::frame()` and the result never
leaves the `Runner`. Two apparently unrelated asks, one cause — which is why gap-log 1.4's verdict
of `no door` was right, and the door is specifically *layout results do not leave the Runner*.

The seam mirrors `ScreenshotRequest` exactly, completion closure included, so the runtime never
learns what an RPC is: `App::take_driver() -> Option<DriverRequest<Msg>>`, drained by the `Runner`
in `user_event` after `update`, answered with a `DriverReport`.

- **2a — the seam, `Frame(n)`, `Advance(secs)`.** Landed. `Advance` deliberately does not reuse the
  frame tick: `tick` paints and *then* spends 1/60s, so `Advance(3)` would land on 3.0167 and any
  equality a caller wrote would be a lie. Its paint is a look at the new instant, not a frame of
  time passing. A 25-minute pomodoro now finishes in one request, which is gap-log 1.5 closed — by
  the bridge, as §7 said, and not by the `--advance` flag 1.5 asked for.
- **2b — `Rects`.** Landed. Read from `hits`, not from layout, and reporting the **clipped**
  rect — `Geometry::contains` tests `visible_rect` and nothing else, so an element scrolled half
  out of view has a layout rect whose centre misses, and a fully clipped one cannot be hit at any
  coordinate and is simply absent. Reporting layout rects would have rebuilt gap-log 1.4 exactly:
  plausible coordinates that quietly do nothing. `DumpTree` says what exists; this says what is
  reachable. The test that matters clicks the centre of what it reports and asserts the handler
  fired — the loop closed rather than described. `Headless::rects()` exposes the same readback to
  Rust tests, which had the same blindness.
- **2c — `Pointer`, `Drag`.** The real hit-test path, and the only thing still keeping `open.rs`
  alive.

### What this costs the apps

Apps 2–6 wait on it. That is the trade being made deliberately: five more apps each paying app
1's coordinate-sweep tax is more expensive than building the harness once, and app 1 is enough
evidence that the tax is real. M2 (app 6) is the thing to watch — if step 1 looks like it will run
past a week, app 6 starts in parallel on the existing driver regardless.
