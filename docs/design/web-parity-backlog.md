# Web-parity backlog — runtime UI

Goal: parity with the web for **representation** (styling) and **interaction** (events/states).
Source: two read-only audits of `runtime/` (2026-07-30).

## Guiding insight

Most gaps are **not missing capability** — the primitive already exists in vello / parley / Taffy;
it just isn't exposed as an `El` builder, or isn't forwarded through the Lua `walk`
(`app_host/src/lib.rs:149`). So parity is mostly: small struct fields + builders + widening `walk`
in lockstep. Two things need a real type change (fill → brush) or a new paint call (shadow).

**Two layers.** `runtime` already has more than Lua can reach. Every runtime addition needs a
matching `walk` arm or Lua apps stay strictly smaller than the runtime.

---

## Representation (styling)

### Tier 1 — can't build a modern UI without
1. **Font weight + italic** — `TextSpec` (el.rs:19) has no weight/style; parley `FontWeight`/`FontStyle` exist. ~1 field + 1 `push_default` each.
2. **box-shadow / elevation** — nothing today; vello `draw_blurred_rounded_rect`. Biggest "depth" lever.
3. **Gradients as fills** — needs `Look.fill: Option<Color>` → brush enum (peniko `Brush::Gradient`).
4. **text-align** — hardcoded `Alignment::Start` (text.rs:93).
5. **Border style (dashed/dotted) + per-corner radius** — `Border` (el.rs:44) has only width+color; dashes already used for debug (paint.rs:194); `radius` is one `f64`.

### Tier 2 — polish / robust layout
6. min/max size + aspect-ratio — free from Taffy, unexposed.
7. line-height + letter-spacing — parley props exist; line-height only wired in editor (editor.rs:37).
8. truncation / ellipsis + white-space/wrap control — always wraps to width (text.rs:74).
9. per-side padding/margin + ml/mr/mx/my/uniform margin — trivial Rect writes.
10. text-decoration (underline/strike) — parley props exist.
11. **`El::scale`** — wanted for the drag ghost ("lifted" preview at ~1.05×). Paint-only `Affine` around
    the subtree is easy; the catch is painted bounds then diverge from the layout rect used for hit
    tests. Acceptable for a ghost (never hit-tested), a footgun in general — so gate it, or scale the
    layout too. Also forces a Lua-boundary change: `walk` currently sends `pos - grab` pre-subtracted,
    and scaling about the grab point needs `grab * s`, so Lua would need `grab` itself → wider
    `CallPhase` or a table arg. See Tier 3 for the general transform.

12. **global UI scale (user zoom, Ctrl±)** — rem's accessibility job without a unit system: one
    multiplier, viewport ÷ z before layout + root transform × z at paint. Numbers stay logical px;
    no cascade, no new units. (DPI is already handled — `set_scale` at lib.rs:852.)
13. **baseline alignment for mixed-size text in a row** — box-centering ≠ baseline: an 11px number
    next to a 14px word floats ~2px high (mnemonic chips; the web's default `vertical-align:
    baseline`). Taffy has `AlignItems::Baseline` but it's meaningless until our text measure
    reports baseline metrics. Until then: manual `mt` nudge, or equal sizes + color hierarchy.
14. **overflow-safe centering** — a centered child wider than the viewport overflows *both* edges
    and the left side is unreachable by scroll (CSS's `safe center` / `margin:auto` problem).
    No auto margins, no safe alignment in Taffy builders today. Mitigation shipped instead:
    minimum window size (winit `with_min_inner_size`) so auth screens can't shrink into the bug.

### Tier 3 — advanced / rare
per-element transforms (rotate via `Affine`), z-index, rounded clipping (clip ignores radius today),
blend modes, filters/backdrop-filter, background image, multiple backgrounds, text-transform,
box-sizing toggle, position:fixed.

### Already have
padding (uniform/x/y), mt/mb, w/h/full/grow, fill (solid), stroke (width+color), radius (uniform),
opacity (propagates), offset (fake translate), absolute + top/left/right/bottom, scroll clip,
overlay/anchored panel, CSS color parsing (in `app_host`), hover_fill/hover_stroke.

---

## Interaction (events / states)

### Tier 1 — core
1. **Generic focus for non-input els + focus ring + programmatic focus** — only text fields focus today (`Focus` hardwired to `Field`, editor.rs:234). Buttons/rows can't be focused/keyboard-activated. Foundation for the rest.
2. **Tab focus traversal** — no Tab handling; needs (1) + ordered focusable list (frame already builds ordered hit lists).
3. **App-level keydown + shortcuts** — only Enter/Esc/F5/F12 reach the app; mods already tracked (lib.rs:472). Wire an `on_key`/shortcut map.
4. **active/pressed visual state** — none for general els (only scrollbar thumb, paint.rs:171). ← **in progress**; add `press_*` like `hover_*`.
5. ~~**click-on-release**~~ — **done (2026-08)**. Press arms `Runner.pressed`; release fires only if the
   pointer is still inside the pressed rect. Drag promotes past 5px and clears `pressed`, so a click and
   a drag are mutually exclusive.

### Tier 2 — expected
6. copy/cut/paste — absent; parley selection exists, plug clipboard (arboard/winit) into editor.rs.
7. disabled state — no flag; bool on `Behaviour` that suppresses hit insertion + dims paint.
8. checkbox/radio/toggle + change events — no primitives.
9. hover enter/leave events + tooltips — hover is visual-only; pointer∩rect already computed each frame (lib.rs:178).
10. double-click + long-press — need press timestamp/count in `click()` (lib.rs:365).
11. placeholder text for inputs — `TextSpec` carries only the value; draw a `fg_3`-style hint when
    value is empty. Wanted by shell2 signup/login fields.
12. password masking — editor renders plaintext only; needs bullet rendering (+ show/hide toggle).
    Wanted by shell2 signup/login; until then passphrases are visible on screen.

### Tier 3
on_scroll events + public programmatic scroll API, keyup, pointermove-to-app, per-element cursor override, explicit submit.

### Already have
click (data-only), right-click/contextmenu, drag & drop (start/move/end + targets), hover visual (+ eased via tint),
inputs + IME, autofocus, on_enter/on_esc, scroll (wheel + nested innermost-first), text select + select-all,
cursor shape (auto grab/text/default), transitions engine (tint/slide/fade).

---

## Lua exposure gap (`walk` + `props.rs`, app_host)

Mostly closed (2026-08) by the `PROPS` fn-pointer registry — 31 props, plus key/type validation and an
`unknown prop` error. Covers sizing, spacing, alignment, absolute positioning, paint, hover_*, tint,
fade_in, autofocus. `walk` keeps only tag construction, `id`, `scroll`, `on_drag`, `on_drop`, `on_input`.

Children accept `false` (skipped), bare strings (text leaf), and untagged tables (fragments, spliced
recursively). Nil children are a hard error — they leave holes and `raw_len` silently truncates.

Still unreachable from Lua: `overlay`, two-arg `fade(to, ms)`, `slide`, `custom` painters, `font` family.

**Standing hazard of the registry:** within a `Prop` variant any builder swaps silently — `("fill",
Prop::Color(El::color))` typechecks and is wrong. Only the test suite catches it.

---

## Drawing + loops — the `custom` gap, decided (2026-08-07)

Above lists `custom` painters as unreachable from Lua. That's not a missing builder, it's a design
decision; taken here.

**Drawing crosses to Lua as *data*, never as a callback.** A Lua fn invoked during paint breaks three
commitments: `app-isolation-draft.md`'s instruction-count interrupt has no seam inside the frame, so
a runaway painter hangs every app in the process; `w3.md` §6's `dump_tree` can't serialize a closure,
so the agent goes blind on anything drawn that way; and per-element VM re-entry per frame is a
different order of cost than the 0.19ms of a 0.92ms frame the W2 pass measured at 387 elements.

**The primitive is an SVG path string.** `kurbo::BezPath::from_svg` (kurbo 0.13.1, `svg.rs:103`)
parses `d` syntax directly, so one prop buys the whole expressiveness of SVG paths —
`{ tag = "path", d = "M 12 3 A 9 9 0 1 1 3 12", stroke = 3, color = "#3b82f6" }`. No vocabulary to
design: `d` is the geometry notation an LLM has read most, and a hand-rolled `{kind="arc", …}` table
trades that familiarity for nothing.

**Stock components (`ui.spinner`, gauges, sparklines) are Lua modules over the primitive, not Rust
builders.** The author is a model and the model can't add Rust — a missing component blocks until
someone writes it, a missing shape it derives in the same turn. Primitives in the platform,
components in userland (the browser/React split). Hallucinated geometry is corrected by the feedback
loop (`dump_tree` now, screenshot W4/W5), not by shrinking the vocabulary.

### Loops (animation.md Tier 4) — the wake policy

`fade`/`tint`/`slide` are *transitions*: Rust stores progress and detects arrival (`lib.rs:216`,
`drive` at `:361`). A loop has no target, so it never registers as in-flight, the frame chain at
`lib.rs:357` stops, and a declared spinner freezes mid-phase.

- [ ] `Appearance.repaint: bool` + `El::repaint()`; the walk ORs it into `any_in_flight`. ~3 lines,
      no `Store` entry, no `Transition` — loops are `f(now)`, nothing to store or GC.
      **Needed by W3 §1** (Argon2 pending state), so this half lands this week.
- [ ] `prop!(repaint)` for Lua, in lockstep — otherwise Lua apps can never have a loading state.
- [ ] **later, with W4's caret blink:** `ControlFlow::WaitUntil` (animation.md §6's middle tier)
      replaces the `any_in_flight` line *only* — declaration, builder, paint math and Lua prop all
      carry over, so this is not throwaway work. Matters for **slow** loops (a blink burning 60fps to
      move 2px); a spinner wants full rate regardless.
- [ ] **open:** loops need a clock inside `view()`. Rust can read `since.elapsed()` off its own
      state; Lua has no equivalent — either `ui.now()` (ambient, makes `view` impure) or `now` passed
      into `view()` (an `App` signature change). Decide once, with both consumers visible.

**Rotation stays deferred** (Tier 3 representation, above) and a spinner doesn't need it: rebuild the
arc from angles each frame (`kurbo::Arc::new(c, r, start, sweep, 0.0)`) instead of transforming a
fixed path. Name loop props for the effect, never the motion — `spin` is a trap while Affine is
unbuilt.

---

## Suggested near-term slice (tied to current work)

1. ~~**click-on-release fix**~~ — done.
2. ~~**Expose existing runtime styling in `walk`**~~ — done via `props.rs`.
3. **Pressed state** (`press_fill`/`press_stroke`) — `Runner.pressed` now exists, so the state is already
   tracked; only the paint side is missing.
4. **`El::scale`** (Tier 2 #11) — blocks the scaled drag ghost.
5. Then pick from Tier 1 representation (font weight, shadow, text-align) as the "beautiful" pass.
