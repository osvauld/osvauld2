# Six apps — proving the Lua layer is authorable (plan, 2026-09-19)

Status: **nothing built.** This is the plan for the next stretch, agreed 2026-09-19. The six
apps in §2 are a proposal and expected to be edited before any of them starts.

Companions: `docs/lua-apps.md` (the author's contract these apps are written against),
`animation.md` (the engine §0 says Lua can't reach), `runtime-rebuild-plan.md` (why).

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
  pending for a while. It earns its slot because animated reorder is the most natural consumer of
  the bridge §0 says is missing.
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

1. **`os.*` shadow** (§4a) — small, unblocked, and gets worse the longer it waits.
2. **Move kanban** to `demo_apps/kanban`, fixing the four `include_str!` paths — mechanical, and
   it puts app #2 in place.
3. **App 1 and app 2**, with the gap log open from the first line.
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
