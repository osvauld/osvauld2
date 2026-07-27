# Animation system + subtree transform — design (rev 3, 2026-07-26)

> **As-built (rev 3):** Phases 0–D shipped, but **exit diverged from this doc** — we built
> neither v1 (declare-until-done) nor v2 (tombstone) but a third approach, **deferred-message
> (Option B)**, and split the single `TransitionSpec` into **property-keyed bindings**. See
> **§14 — As-built** for the real shape and what's still open. §11's v1/v2 are kept as the
> design survey that led there.

Expands w1.md §4/§5 into a build plan. Companion: `w1.md` (the week), `runtime-rebuild-plan.md`
(the why). Rev 2 adds §1: three runtime restructures that make the rest *simpler to build* —
adopted after re-reviewing the plan against `lib.rs` with the license to reshape (the runtime is
~1.5k lines; bolting-on costs more than reshaping). Scope: state transitions (Tier 1), subtree
transform (Tier 3 enabler), enter/exit (Tier 2, two levels — §11), page transitions (Tier 3),
loops (Tier 4, caret blink). Lottie/velato stays deferred.

---

## 0. What already exists (verified against source — don't rebuild these)

- **Clock.** `Render.start: Instant`; `render.paint` already passes `now: f64` into the frame
  closure; `lib.rs frame()` ignores it (`_now`). The clock exists; nothing consumes it. (R1
  moves it onto `Runner` — §1.)
- **Redraw discipline.** Retained mode, `ControlFlow::Wait`. `frame()` ends with
  `if self.app.animating() || needs_redraw { redraw() }` — the all-or-nothing hook §6 replaces.
  Pointer moves already request a repaint, which is why stateless hover works.
- **Retained store.** `Store` keyed `(Id, TypeId)` with live-sweep: anything not touched during a
  frame is dropped by `sweep()`. Animation state parked here **auto-GCs when its element leaves
  the tree** (also the reason exit can't live here — §11).
- **Hover.** Derived in `paint::draw`: `over = pointer ∩ rect` → `Look::resolve(over)` hard
  switch. The exact seam §7 upgrades.
- **Two frictions the old plan inherited** (they motivate §1):
  - `frame()` builds the scene *inside* `Render::paint(closure)`, forcing the alias-every-field
    dance (`let hits = &mut self.hits; …` × 10) — and giving `on_done` messages produced
    mid-frame no path to `self.app.update()` (the app is borrowed by the closure).
  - Behaviour routing = nine parallel `Vec`s (`hits`, `input_hits`, `input_maps`,
    `context_hits`, `drag_hits`, `enter_msgs`, `esc_msgs`, …), cleared and rebuilt every frame by
    `take()`ing out of `Placed`. Every new behaviour pays the "new field + clear + take" tax.
    Bonus finding: `draw` pushes/pops a **clip layer per clipped node** — N layers for N children
    of a scroll container.

## 1. Restructure first — what the tiny runtime affords

### R1 — flatten the frame (Phase 0; prerequisite for a sane tick)

Change `Render`'s contract from "call me with a closure" to "give me a finished scene":

- `Runner` owns `scene: Scene` and the clock (`start: Instant`, `last_frame: Option<f64>`).
- `Render` gains `viewport()` + `transform()` getters and
  `present(&Scene, clear)` = acquire surface → render_to_texture → blit → present.
- `frame()` becomes straight-line code:
  `now/dt → view → solve → placed loop (hits + tick) → sweep → draw → present → dispatch queued
  msgs → wake decision`.
- The borrow dance **deletes itself** — this change is mostly removals. `on_done` dispatch gets a
  natural home (after present, plain `self.app.update(m)` calls, then request redraw).
- Accepted quirk: on surface loss we now build a scene and drop it (was: skip building). Rare,
  transient, harmless.

### R2 — retain the placed list; hit vectors become queries (own phase; independent)

Keep last frame's `placed: Vec<…>` on `Runner` instead of shredding it into parallel vectors:

- `click` / `right_click` / drag / enter / esc paths query it directly —
  `iter().rev().find(|p| p.behaviour.on_click.is_some() && hit_rect(p).contains(pt))` — same
  topmost-wins semantics; `M: Clone` covers dispatch; the `take()` calls and ~7 `Runner` fields
  disappear.
- `scroll_hits` / `bar_hits` stay: they're *computed geometry* (thumb rects, gains, content
  sizes), not behaviour routing.
- Payoff beyond hygiene: **ghosts (§11 v2) need exactly this retained list** — with R2 it's
  ambient and Phase F shrinks to markers + tombstone replay. It's also the seed of the future
  pointer-move fast path (repaint from retained placed without re-running view/layout — the
  rebuild plan's dirty-gate seam; out of scope here).

### R3 — one paint stream with Push/Pop markers (folds into Phase C, as a *replacement*)

The flat list has no subtree boundaries. That gap currently costs a clip layer per clipped node,
and it blocks group opacity (§8b) and ghost spans (§11 v2). One fix serves all three:

```rust
enum PaintItem<M> {
    Node(Placed<M>),
    Push { clip: Option<Rect>, alpha: f32, id: Option<Id> },   // subtree opens
    Pop,                                                        // subtree closes
}
```

`emit` produces this stream. One clip layer per scroll container (*fewer* layers than today),
alpha groups for §8b, id-tagged spans for ghosts. Not an add-on — it **replaces** the
per-node clip path. (`Placed.clip` may survive for hit-rect intersection only.)

## 2. The model — three layers, one primitive

```
clock (Runner) → animated values (tick, in Store) → bindings (paint reads value, lerps a prop)
```

The single primitive: **a retained scalar that glides toward a target over a duration**.
Everything else — hover fade, press, enter/exit, page slide — is a *binding* that reads that
scalar and lerps a paint property. Presets are named bundles, never separate machinery.

Two rules that keep this sane:

1. **Store linear progress; apply easing at read time.** Stored eased values snap or double-ease
   when reversed mid-flight (pointer leaves at 60%). Linear progress retargets cleanly: flip
   `target` 1→0 and the same number ticks back down.
2. **Tick mutates, paint reads.** The tick (inside the placed loop) takes `&mut Store`; `draw`
   keeps `&Store`. Never advance progress inside `draw`.

**The Lua constraint (load-bearing, from the M1 aim):** apps will ultimately be Luau — so
animation must be **declaration-only**. The app (Rust today, Lua at M1) declares *data* — ids,
durations, preset enums, tombstones — and receives *messages* (`on_done`); the runtime owns
every per-frame computation. A Lua app must never need to run at frame rate to animate, and a
hung/sandboxed app's animations still complete because the runtime ticks them. This is why:
no egui-style synchronous value reads in `view` (would force per-frame Lua), `on_done` is a
message not a callback, easing is a fixed enum (extend with cubic-bezier *params* — data, like
CSS — never closures), and `App::tick` app-owned progress is only a fallback (§10) — it calls
into the app every frame, exactly what the Lua boundary must avoid.

## 3. Data types (runtime, new module `anim.rs`)

```rust
struct Transition {      // retained per animated id in Store — swept when the element vanishes
    progress: f32,       // linear 0..1, the ONLY mutated field
    target: f32,         // 0.0 or 1.0
    duration: f32,       // seconds, from the El declaration
}

enum Easing { Linear, EaseOut, EaseInOut }   // pure fns of t, tiny fixed set
```

- `tick(dt)`: move `progress` toward `target` by `dt / duration`, clamp 0..1.
  `in_flight()`: `progress != target`.
- **A new `Transition` starts at `progress = 0`, never at its target.** This is the
  enter-friendly choice: anything born with target 1 fades in (§11's Store-miss rule); a
  hover element born under the pointer glides to hover instead of snapping. One rule, no cases.
- Easing lives on the *declaration* (the El), not in stored state — the store holds only what
  must survive between frames.
- **dt** comes from `Runner`'s clock (R1): `dt = now - last_frame`, **clamped to ≤ 0.1 s**
  (first frame / unfocus gap must not teleport-with-overshoot; clamping degrades to "skip near
  the end", which is what you want).

## 4. Declaration — animation attaches to any El, never to a widget

```rust
.transition("todo:add", 0.15)          // hover-driven; optional .easing(Easing::EaseOut)
.transition_to("menu", 1.0, 0.15)      // app-declared target (exit v1, §11)
.enter("menu", Enter::Fade, 0.15)      // plays once, on first appearance
.exit("todo:5", Exit::SlideLeft)       // snapshot opt-in + exit spec, replayed by ghost() (§11 v2)
```

- `TransitionSpec { id: Id, driver: Driver, duration: f32, easing: Easing, on_done: Option<M> }`
  in `Behaviour<M>` (`map_rc` passes it through, mapping `on_done` — same as scroll/drag).
  It is **read in place, never `take()`n** — both tick and paint consult it (precedent: paint
  already reads `p.behaviour.input`).
- **The primitive is target-agnostic**: `Driver::Hover` derives the target from
  `pointer ∩ hit_rect`; `Driver::Value(f32)` takes it from the app's declaration. One
  `Transition` in the store either way.
- **Explicit ids, like `scroll_y("todo:list")`.** Auto-ids are impossible: the tree is rebuilt
  every frame with no keys, and index paths break on reorder.
- **`.enter` is sugar over the Store-miss rule**: id absent on first appearance → created at 0
  with target 1 — appearance *is* the trigger, existence *is* the "was it triggered" memory
  (sweep erases it on removal, so re-appearing replays). The preset names the *binding*:
  `Fade` → group opacity (§8b), `SlideFrom*` → offset (§8a), resolved against the element's
  **own laid-out rect** (the app never needs viewport math). First consumer: overlay open fade.
- **`.exit` on a live element does two jobs**: names the exit animation and emits §1-R3 span
  markers each frame so the runtime can find this subtree's paint output later. The actual
  trigger is the app's tombstone (§11 v2) — exit can't self-trigger; the element's state dies
  with it.
- An element with `hover_*` but no spec keeps today's hard switch. Zero cost unless used.

## 5. Tick — inside the existing placed loop, not a new pass

`frame()`'s placed loop already computes `hit_rect` (clip-intersected) and already mutates the
store (`get_or::<Scroll>`). Tick joins it — for each element with a `TransitionSpec`:

1. resolve the target from the driver — `Hover`: `pointer ∩ hit_rect` → 1.0/0.0 (clip-aware:
   fixes hover-through-clipped-rows for animated elements); `Value(v)`: just `v`,
2. `store.get_or_with::<Transition>(id, …)` — **marks it live**, so sweep keeps it,
3. set the target, `tick(dt)`,
4. **edge-trigger `on_done`**: only if progress *arrives this tick* (was in flight, now equals
   target) push the msg into a local queue — never on frames already at rest,
5. OR-accumulate `in_flight()`.

After `present` (R1's straight-line frame): dispatch the queue via `self.app.update(m)`, and if
anything was dispatched, request a redraw (state changed — same contract as a click).

## 6. Wake policy — replaces `App::animating()`, which is **deleted**

| condition                              | action                                     |
|----------------------------------------|--------------------------------------------|
| any transition in flight, or msgs just dispatched | `request_redraw()` → vsync-paced loop |
| a loop is alive, nothing in flight     | `ControlFlow::WaitUntil(next deadline)`    |
| idle                                   | `ControlFlow::Wait` (today's behavior)     |

- vsync pacing + dt math ⇒ speed is refresh-rate independent (144 Hz = smoother, not faster).
  Never advance by a per-frame constant.
- `WaitUntil` exists for W4's caret blink (a 2 Hz square wave must not burn a 144 fps loop).
  Wire it when the blink lands (§9); note: the deadline is set from `frame()`'s caller, which
  has the `ActiveEventLoop`.
- **`App::animating()` is deleted in Phase A** — superseded by runtime in-flight tracking.
  Only the live-both page fallback (§10) would re-introduce an app keep-alive (alongside
  `App::tick`); don't carry the dead hook meanwhile.
- **`EventLoopProxy` is out of animation scope.** w1.md §4 lists it, but it solves cross-thread
  wakeups (async/MCP/agent events later), not timing. Strike it from the animation checklist.

## 7. Paint binding v1 — the Look lerp

`Look::resolve(over)` gains a sibling:

```rust
resolve_t(t: f32) -> (Option<Color>, Option<Border>)   // t = eased progress
```

- **Endpoints first, then lerp**: A = `resolve(false)`, B = `resolve(true)`, lerp A→B by `t` —
  keeps the `hover_fill.or(fill)` fallback logic in exactly one place.
- Lerp rules: fill color lerp; stroke = color + width lerp; **radius snaps** (v1).
- Color lerp: peniko `Color` (the `color` crate) provides interpolation; gamma-space lerp reads
  slightly muddy mid-fade — accepted v1, revisit in linear-sRGB if visible.
- `draw` picks per element: spec present → read `Transition`, `resolve_t(eased)`; else →
  today's `resolve(over)`.

First consumer (proves the pipeline): todo.rs add-button — `.transition(…)` on the row that
already has `.fill(accent).hover_fill(accent_press)`. **The W1 done-when.**

## 8. Subtree transform — the Tier 3 primitive (two capabilities)

Page transitions demand not animation but *"move this whole subtree and fade it as a group."*

### 8a. Subtree translate — `.offset(dx, dy)`

Paint-time translation of an element **and all descendants**, invisible to Taffy (no reflow).

- Implementation is tiny: `emit()` already accumulates `(ox, oy)` down the recursion — add the
  element's offset into that accumulation. Done.
- **Hit-testing follows for free**: hit rects come from the same accumulated rects, so visuals
  and clicks move together. No Affine, no inverse hit-test (that's why scale/rotate are
  deferred).

### 8b. Group opacity — `.opacity(a)`

Correct semantics = one vello layer per subtree: `push_layer(alpha)` before, `pop` after the
last descendant — the group composites once, then fades as one image. Carried by §1-R3's
`PaintItem` stream (which also replaces per-node clip and tags ghost spans — build once, serve
three).

**Stepping stone (optional):** multiply an inherited alpha down `emit` and apply per-primitive —
no restructure, but overlapping child/parent fills ghost ("see through the text into the page
behind"). Invisible on fast fades; judge by eye. The marker stream is the real design.

Deferred: `scale` / `rotate` (full Affine + hit inversion), blur, masks. Slide + fade covers
pages.

## 9. Loops (Tier 4) — `f(now)`, no stored progress

Loops are *stateless*: pure functions of the clock modulo a period. No `Transition`, no target.

- **Caret blink (W4, the only planned consumer):** `Field` gains `blink_epoch: f64`, reset on
  any edit/caret move (caret solid right after typing). Paint:
  `visible = ((now - epoch) % 1.0) < 0.5`. Wake: `WaitUntil(next half-period)` while an input
  is focused — §6's middle tier.
- Spinner/pulse: same shape, but each visible loop forces continuous redraw — loops must be
  *declared* so the wake policy knows one is alive. Defer the API until a real consumer (M1).
- `now` starts flowing in Phase 0 (R1 moves the clock to `Runner`).

## 10. Pages (Tier 3) — one page's enter is the other's exit

A page transition is §11's enter/exit at screen scale, in lockstep:

- Screen roots declare both halves up front:
  `.enter(Enter::SlideFromRight, 0.3)` + `.exit(Exit::SlideLeft)`.
- Navigate: the router swaps `current` **and** declares `ghost("screen:login")` (§11 v2) beside
  the new screen. Same frame: login's ghost slides out, todo's enter slides in — two independent
  transitions in lockstep because they share a duration and started together.
- Ghost's `on_done` → router drops the tombstone. The whole router is `leaving: Option<ScreenId>`.
- **No `App::tick`, no app-owned progress, no keep-alive hook** — runtime in-flight tracking
  drives everything.
- The outgoing screen is a **frozen snapshot** — non-interactive, not updating. At 200–300 ms
  that's what every framework effectively does anyway (input is blocked during transitions).
- Edge, noted not solved: navigating *back* mid-flight — drop the tombstone for the screen
  you're returning to, else its ghost and its live self paint together.
- **Fallback if Phase F is cut:** the live-both router — keep the outgoing screen declared
  (`outgoing: Option<Screen>` + app-owned progress advanced by a re-introduced `App::tick(dt)` +
  keep-alive), render both wrapped in §8 offset/opacity. Heavier app code, zero ghost machinery,
  outgoing stays live. The tombstone version is the destination; this is the bridge — and note
  it's **Lua-hostile** (per-frame calls into the app layer), another reason it's fallback-only.

## 11. Tier 2 — enter/exit (two levels)

> **Superseded by §14.** What shipped is **deferred-message exit (Option B)** — neither v1 nor
> v2 below. The v1/v2 survey here is the reasoning trail that led to it; read §14 for as-built.

The hard half is **exit**: the app drops an element from `view()`, but it must keep painting for
the exit duration. Survey — how the immediate-mode world actually handles this:

| framework | animation mechanism | exit story |
|---|---|---|
| Dear ImGui | none — you lerp your own values every frame | keep drawing it yourself until your own timer ends |
| egui | `ctx.animate_bool(id)` — retained values keyed by `Id` in ctx memory (== our `Transition` in `Store`) | **no automatic exit.** Idiom: keep *declaring* the widget while the animated value > 0, drop it at 0. `CollapsingHeader` openness works exactly this way |
| Iced | nothing first-class; app-side lerp + a time subscription | same: the app retains |
| Compose | `AnimatedVisibility(visible)` — node stays in composition while animating out | the app-retains idiom, formalized as a wrapper |
| React + Framer | `AnimatePresence` — a *wrapper component* holding removed children until their exit finishes | retention without touching the framework core |
| SwiftUI / Flutter | retained-tree diff engine detects removal, animates the node out | the full engine — what we're **not** building |

Three observations that shape the plan:

1. **No immediate-mode framework has automatic exit.** The universal idiom: the thing isn't
   actually removed while animating out — *the app keeps declaring it* until the animation lands.
2. egui makes that ergonomic because `animate_bool` returns the value **synchronously, during
   declaration** — the immediate-mode superpower. Our `view(&self)` is pure and can't read the
   `Store`, so completion becomes what everything else in the Elm shape is: a message.
3. `AnimatePresence` proves retention doesn't need the framework core — a wrapper that owns dead
   children is enough. Our v2 is that idea at the paint level.

### v1 — "declare until done" + completion message (the egui idiom, Elm-ified)

- Close request → app state `Open → Closing`: element **still declared**, now with
  `.transition_to(id, 0.0, dur)` + opacity binding → fades out → runtime delivers `on_done` →
  app flips to `Gone`, next frame omits it, sweep collects the state.
- Zero new machinery beyond Phases 0–B (+ §8 for the opacity binding). Covers overlay/modal
  close.
- Cost: three-state boilerplate per exit-animated element — and for *list removals* you'd keep
  the corpse in your `Vec` until it fades. The motivating consumer for v2.

### v2 — ghost replay via an app-declared tombstone

The app *declares that an element is transitioning out* — as a dummy element:

```rust
ghost("todo:5", Msg::GhostGone)   // layout-inert marker: "replay this id's snapshot, exiting"
```

- The live element opted in earlier with `.exit(Exit::Fade)` (§4) — that emitted the §1-R3 span
  markers making its paint output findable, and named the exit animation.
- The runtime holds the **previous frame's paint list** — with R2 that retention is *ambient*
  (it's the same list events query; a move, never a clone — `Placed` holds closures and can't
  clone anyway). The frame the element vanishes is the frame the tombstone appears → snapshot
  present, no gap.
- Tombstone processing: find the id's span in the retained stream, **strip behaviour** (ghosts
  are non-interactive: no clicks, no hits), animate offset/opacity by an **inline** progress
  (not `Store` — undeclared state would be swept), paint after main content, deliver `on_done`
  at 0. App drops the tombstone.
- **Why tombstone beats auto-detect** (runtime diffing vanished ids): the trigger is explicit
  and Elm-honest — the app *says* it, the runtime never guesses; no per-frame id-diff; and
  cancellation (mid-flight back-nav) is simply *not declaring* it. Auto-tombstone-on-vanish can
  land later as sugar on the same machinery.
- Known edges, accepted: a ghost referencing swept scroll state paints unscrolled (~150 ms
  life); siblings snap into the freed gap while the ghost fades in place — smooth gap-closing is
  *animated layout*, a separate feature in every framework too (deferred with scale/rotate).
- What it buys over v1: todo-delete with **zero corpse data**; page transitions (§10) without
  keeping the outgoing screen live.

## 12. Decisions log

| decision | why |
|---|---|
| **animation is declaration-only (the Lua constraint)** | M1 apps are Luau: data in (ids/durations/presets/tombstones), messages out (`on_done`); runtime owns all per-frame work — no app code runs at frame rate, animations survive a hung app |
| **flatten the frame first (R1)** | straight-line `frame()` deletes the closure borrow-dance and gives `on_done` dispatch a home; mostly removals |
| **retain placed, hits become queries (R2)** | kills the per-behaviour parallel-vector tax; ghosts need the retained list anyway; seeds the dirty-gate fast path |
| **`PaintItem` stream replaces per-node clip (R3)** | one mechanism = clip (fewer layers than today) + group alpha + ghost spans |
| explicit ids (`.transition("x", …)`) | tree rebuilt per frame, no keys; index paths break on reorder; consistent with scroll/drag/input |
| store linear progress, ease at read | mid-flight retarget (pointer leaves at 60%) reverses cleanly |
| new `Transition` starts at progress 0 | one rule, enter-friendly; born-under-pointer glides instead of snapping |
| tick inside the placed loop | the loop already has `hit_rect` + `&mut Store`; clip-aware `over` fixes hover-through-clip for animated elements |
| `on_done` is edge-triggered | fires once, on arrival — never on resting frames |
| animation state in swept `Store` | element vanishes → state GC'd free; no leak, no bookkeeping |
| clock moves to `Runner` (R1) | tick needs `now` before scene build; `Render`'s `now` param retires |
| **delete `App::animating()`** | superseded by in-flight tracking; only the live-both fallback would re-add a keep-alive (with `App::tick`) |
| `EventLoopProxy` cut from animation scope | it solves cross-thread wakeups, not timing; redraw chaining + `WaitUntil` suffice |
| exit v1 = declare-until-done + `on_done` msg | the universal immediate-mode idiom (egui/ImGui/Iced); egui reads the value synchronously in declaration — our pure `view()` can't, so completion becomes a message |
| enter = Store-miss rule + `.enter` presets | appearance is the trigger, existence is the memory; sweep resets it on removal for free |
| exit v2 = app-declared `ghost(id)` tombstone + snapshot replay | explicit Elm-honest trigger (no vanish-diffing); zero corpse data; cancellation = don't declare it; needs R3 markers + R2's retained list |
| pages = new screen's `.enter` + old screen's `ghost` | one page's enter *is* the other's exit; lockstep via shared duration; kills `App::tick` + app-owned progress (live-both router demoted to fallback) |
| group opacity via layer markers (not alpha-multiply) | alpha-multiply ghosts on overlap; markers are correct compositing (stepping stone allowed) |
| translate via `(ox, oy)` accumulation | hit rects move with visuals for free; no Affine, no inverse hit-test |
| scale/rotate deferred | needs full Affine through paint + hit inversion; slide+fade covers pages |
| animated layout (gap-closing) deferred | a separate feature in every framework too; not needed for M0 |
| Lottie/velato parked | secondary per plan; rides the same clock when it lands |

## 13. Build order

### Phase 0 — flatten the frame (R1)
- [ ] `Render`: `viewport()` / `transform()` getters; `present(&Scene, clear)`
- [ ] `Runner`: owns `scene: Scene` + clock (`start`, `last_frame`); `Render`'s `now` retires
- [ ] `frame()` straight-line: view → solve → placed loop → sweep → draw → present → dispatch →
      wake; delete the alias dance

### Phase A — animation infrastructure (w1 §4)
- [ ] `anim.rs`: `Transition` (tick / in_flight, starts at 0) + `Easing` fns
- [ ] `TransitionSpec` (driver + `on_done`) on `Behaviour<M>`; `.transition` / `.transition_to`
      builders; `map_rc` passthrough
- [ ] tick folded into the placed loop (edge-triggered `on_done` queue, `any_in_flight`)
- [ ] wake: `in_flight || dispatched || needs_redraw` → redraw; **delete `App::animating()`**

### Phase B — hover fade (w1 §5 — the W1 exit)
- [ ] `Look::resolve_t` (endpoints-then-lerp; color + stroke lerp, radius snaps)
- [ ] `paint::draw` branches: spec → eased lerp, else hard switch
- [ ] todo.rs add-button hover fade — **the W1 done-when**
- [ ] press feedback (stretch)

### Phase C — subtree transform
- [ ] `.offset(dx, dy)` → accumulate into `emit`'s `(ox, oy)`
- [ ] R3: `PaintItem` stream **replacing** per-node clip; `.opacity(a)` group layers
      (alpha-multiply stepping stone allowed)

### Phase D — enter/exit (as-built: deferred-message, §14) ✓
- [x] property-keyed bindings: `slide`/`fade`/`tint` slots replace the single `TransitionSpec`;
      paint always `resolve_t` (hover + fade coexist)
- [x] `on_done` edge-trigger + dispatch after present
- [x] exit via `exiting` map + `drive` target-0 override + held message (Option B) — todo delete
- [x] overlay dismiss fade: backdrop `.exit(panel_id)` read from the panel's own fade binding
- [ ] `.enter`/`Enter::Fade` preset **enums** (today: `.fade_in`/`.slide_in` builders, no bundle layer)
- [ ] `.exit()` bool self-fade form; `.exit_as(Type)` asymmetric exit
- [ ] stale-`exiting` cleanup (element removed by another path mid-fade)
- caret blink still W4 (blink_epoch + `WaitUntil` tier) — not this doc's checklist

### Phase E — retain placed, hits become queries (R2; independent, any time after 0)
- [ ] `placed` lives on `Runner`; `click`/`right_click`/drag/enter/esc query it
- [ ] delete the taken-vector fields (`hits`, `input_hits`, `input_maps`, `context_hits`,
      `drag_hits`, `enter_msgs`, `esc_msgs`) + their clear/rebuild
- [ ] `scroll_hits` / `bar_hits` remain (computed geometry)

### Phase F — ghosts + pages (needs C + E)
- [ ] `.exit(Exit)` emits id-tagged span markers; `ghost(id, on_done)` tombstone element
- [ ] ghost replay: move span from retained stream, strip behaviour, inline progress, paint
      after main, `on_done` at 0
- [ ] consumers: todo-delete fade; login ⇄ todo page slide (`.enter` + `ghost` in lockstep)

### Done-when
- add-button visibly fades in/out on hover, reverses smoothly mid-fade (**W1 exit = through B**)
- overlay fades in on open, fades out on dismiss, app state cleanly reaches `Gone`
- page slide login ⇄ todo: new screen enters while the old one's ghost exits, in lockstep
- `cargo check -p runtime` clean, zero warnings

### Cut lines (in order, when hot)
1. Phase F ghosts + pages (bridge pages with the live-both router if needed sooner, §10)
2. Phase E (pure hygiene until F needs it)
3. group opacity markers (keep the alpha-multiply stepping stone)
4. press feedback
5. enter presets (overlay open fade)

**Never cut:** 0 → A → B — the W1 exit criterion, and W4's caret blink builds on the same
clock + wake policy.

## 14. As-built (rev 3, 2026-07-26) — property-keyed bindings + deferred-message exit

Phases 0–C landed as designed. Phase D diverged twice — both simplifications found while building.

### 14.1 Property-keyed bindings (replaces the single `TransitionSpec`)

One `transition: Option<TransitionSpec>` per element meant one timeline drove *everything*: an
element couldn't fade in **and** hover-tint (paint branched — a transition present locked hover
out), and couldn't compose two drivers. Fix: split the slot by **property**.

```rust
struct Binding<M> { id, driver, duration, easing, on_done }   // a timeline handle

// on Behaviour<M>, replacing `transition`:
slide: Option<(Binding<M>, (f32, f32))>   // + from-offset payload
fade:  Option<Binding<M>>
tint:  Option<Binding<M>>
```

- Each property binds its **own** timeline; two properties share one by sharing an `id`.
- Builders: `.tint(id, ms)` (Hover), `.slide_in(id, xy, ms)` / `.fade_in(id, ms)` (Value 1.0).
- Paint no longer branches: **always `resolve_t(t)`**, with `t` = tint's eased progress if the
  slot is set, else `over as 0/1` (instant hover = a snapping timeline). Hover + fade coexist.
- `drive()` in the placed loop ticks every slot uniformly; attach `on_done` to **Value**
  timelines only (a Hover binding "lands" on every pointer settle).

This separates the primitive §2 implied but merged: a *timeline* is a scalar; a *binding* is
"property P reads timeline T." Three collisions (hover+enter, hover+color, asymmetric enter/exit)
all dissolve once timeline and binding are distinct.

### 14.2 Exit = deferred-message (Option B) — neither v1 nor v2

The question §11 missed: *who keeps the corpse alive while it fades?* v1 = the **app** (Open/
Closing/Gone). v2 = the **runtime paint list** (tombstone). Option B = **the app keeps declaring
it for free, because the runtime simply hasn't delivered the message yet.**

- `.exit(id)` on the clickable element names the timeline to reverse (the row's fade id —
  cross-element, since the `×` button owns no timeline). `exit: Option<Id>` on Behaviour.
- Runtime state `exiting: HashMap<Id, Msg>` (retained). The per-frame binding **can't** hold the
  pending msg — `view()` rebuilds it every frame — so the id keys retained state instead.
- Click on an exit element → routed to `exit_hits`; on hit → `exiting.insert(id, msg)`. The
  message is **held, not dispatched.**
- App state unchanged (msg undelivered) → `view()` still emits the element → its timeline keeps
  ticking. `drive()` forces `target = 0` for ids in `exiting` → fades out (enter reversed, same
  scalar to 0).
- On landing → `exiting.remove(id)` → deliver the held msg (reuses the `on_done` → `done_msgs`
  → `app.update` path) → app removes → next frame swept.

The app's `update()` is the **same one-line delete** it'd write with no animation. No state
machine, no corpse in the `Vec`, zero exit boilerplate.

**Overlay dismiss is free:** the runtime already builds the dismiss backdrop, so it stamps
`.exit(panel_id)` on it (`panel_id` read from the panel's own `fade` binding). Dismiss fades the
panel, then delivers the dismiss message. The app writes only `.fade_in` on the panel.

### 14.3 What Option B does *not* cover — and what's still open

Option B covers exits triggered by **runtime-seen input** (click, dismiss). It does **not** cover
**data-driven** removal — a row that vanishes because a *server sync* deleted it, no click. That
still needs **v2 (tombstone) + Phase E (retained placed)**, deferred until sync exists.

Still open:

- **Phase E** — retain `placed` on `Runner`, hits become queries (R2). Not started.
- **Phase F** — tombstone/ghost for data-driven exit + zero-corpse list delete. Needs E; deferred to sync.
- **Pages** (§10) — screen transitions. Not started.
- **Loops** (§9) — caret blink (W4). Not started.
- `.enter` / `Enter::Fade` **preset enums** — today only `.fade_in` / `.slide_in` builders; no named-bundle layer over §8.
- `.exit()` **bool self-fade** (reads own fade id) + `.exit_as(Type)` **asymmetric exit** — only explicit `.exit(id)` built.
- **stale-`exiting` cleanup** — if the app removes an element by another path mid-fade, its `exiting` entry strands (could misfire on a reused id); one-line retain-after-sweep fixes it.
