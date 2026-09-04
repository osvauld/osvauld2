# Visual substrate — Frame, math, vector graphics, physics (draft, 2026-08-28)

One substrate under four things we want and don't have: **math typesetting**, **vector
graphics**, **charts that aren't a closed enum**, and **interaction that feels alive**.

Companions: `animation.md` (rev 3 — the timeline/binding layer this builds on),
`runtime-rebuild-plan.md` §2.5 (rich text leaf — the first thing that needed this),
`w3.md` (the current week; none of this is W3 work).

Status: **design notes from a pairing session, nothing built.** No week assigned.

---

## 0. What already exists (verified against source — don't rebuild)

- **Text is single-style.** `TextSpec { text, family, size, color }` (`el.rs:19`);
  `text.rs:68` only calls `push_default`. Parley's `RangedBuilder` supports per-range styles;
  nobody has used it.
- **Editing is plain-text by design.** `parley::PlainEditor` (`editor.rs:18`). Styled editing
  is a separate problem (M3).
- **Only text leaves are measured.** `layout.rs:62` sizes text leaves from the shaped extent;
  `custom()` leaves get nothing and must be hand-sized.
- **Paint knows two shapes.** `RoundedRect` fill/stroke + glyph runs (`paint.rs:79`).
  No arbitrary paths.
- **Animation = property-keyed bindings.** `slide`/`fade`/`tint` over a linear `Transition`
  (`anim.rs:1`), retained in `Store` under a closed `Slot` enum (`state.rs:13`).
  See `animation.md` §14.1 — that shape is right; the *driver* is what's limiting.
- **Press is not animated.** `paint.rs:69` hard-swaps `press_fill`. `animation.md` §13 Phase B
  lists "press feedback (stretch)", unchecked.
- **`.offset` translates subtrees; scale/rotate are deferred.** `animation.md` §8 defers full
  Affine explicitly, because hit-testing would need the inverse. Real cost, still true.
- **Luau is multi-file already, but only `main.lua` loads.** `app_src.rs:52` stores a whole
  `files` map into the LoroDoc; `app_host/src/lib.rs:58` reads exactly one entry. No `require`.
- **The interrupt budget is per-frame.** `fires` resets in `view()` (`lib.rs:79`) and `update()`
  (`:110`); the 1M ceiling at `:185` is ~half a frame of Luau. Sized for a UI app, not an
  animating one.
- **`now()` is whole seconds.** `lib.rs:176` — `.as_secs() as i64`. Useless for animation.
- **`repaint` exists but Lua can't reach it.** `El::repaint()` (`el.rs:585`), honored at
  `lib.rs:217`, absent from the props table (`props.rs:195-215`).

---

## 1. The one decision — drawing becomes data

Today the escape hatch is a closure: `custom(|scene, text, rect, xform| …)` (`el.rs:346`).
A closure can only draw. You cannot measure it, nest it, cache it, diff it, export it, or
scale it.

Replace it with a value:

```rust
Frame { w: f32, h: f32, baseline: f32, items: Vec<(f32, f32, Item)> }

enum Item {
    Glyphs { font, size, glyphs: Vec<(GlyphId, f32, f32)>, color },
    Path   { path: BezPath, style: Fill | Stroke },
    Group  { transform: Affine, filter: Option<Filter>, frame: Frame },
}
```

**`baseline` is the field that makes math possible.** `x^2` is not two boxes side by side —
the `2` sits at a height defined relative to the `x`'s baseline. Nest a fraction in a sentence
and the fraction's axis must meet the sentence's baseline. With `baseline` in the type, the
layout engine has exactly one rule at every depth: *given child frames with w/h/baseline,
place them*. Without it, every construct is a special case.

**`Group` is the field everything else hangs off.** Five unrelated features turned out to be
operations on a Frame subtree:

| feature | what it does to a Group |
|---|---|
| morph / tween | lerp matched subtrees' transforms |
| hit-testing | walk the tree, test positioned children |
| filters | read `filter` at composite time |
| physics | write `transform` each frame |
| retained caching | cache the whole subtree by id |

Five features, one node type. That's the signal the model is carved at the right joint, and
it's the argument for building `Frame` before anything that needs it rather than during M4.

**This is not a new idea in this repo.** `animation.md` §8b already reaches for a `PaintItem`
stream to make group opacity composite correctly ("build once, serve three"). `Frame` is that
stream generalized and given a name. Build them as one thing, not two.

### 1.1 Consequences

- `pdf_paint/src/lib.rs:3` already says *"the caller owns layout — everything arrives absolutely
  positioned in its own source space."* That is `Frame`, egui-shaped and implicit. One Frame,
  N renderers: vello, PDF, SVG.
- Charts stop being `enum ChartKind { Line, Bar, Scatter }` (`app_engine/src/node.rs:363`).
  Today every new visual is a Rust change and a release. After: a `.lua` file in a space.
  That is most of M2's actual value — the agent *builds* visuals instead of picking from three.
- Scale invariance: a Frame under a transform zooms cleanly. Required by M4's canvas.

---

## 2. Math typesetting

### 2.1 The decisive finding

**`ttf-parser` 0.25.1 has complete OpenType MATH support**, and it is already in the local
cargo registry. `read-fonts` 0.40.2 (what vello/parley use) has **no** MATH table — checked.

| `ttf_parser::math` | gives |
|---|---|
| `Constants` | all ~56 — axis height, fraction rule thickness, script shifts, radical/limit gaps |
| `GlyphInfo` | italic corrections, top accent attachments, extended shapes, per-corner kerns |
| `Variants` | vertical/horizontal constructions, `GlyphAssembly`, `min_connector_overlap` |

The font plumbing *was* the hard part of a math engine. It's a thin dependency over the same
font bytes — same file, same glyph ids, and vello's `draw_glyphs` already takes raw ids
(`text.rs:105`). This removes the entire reason to depend on typst or ReX.

### 2.2 Shape

- `str → Frame`. Pure: no scene, no GPU, no parley, no window. Headless-testable by asserting
  box geometry, which is the only way this gets correct.
- Memoize on `(expr, size)` — it's a pure function, so caching is free, and that matters once
  something animates at 60fps.
- **Glyph ids, not strings.** Stretchy delimiters and assembled braces select *unencoded*
  glyphs — size variants and assembly parts with no codepoint. A string cannot name them.
  (An earlier strings-only sketch was wrong for exactly this reason.)

### 2.3 Algorithm and font

- Use **MathML Core §Layout**, not the TeXbook. It's written directly against these MATH
  constants, it's unambiguous, and it's what browsers implement.
- Shape: parse LaTeX → atom list (Ord/Op/Bin/Rel/Open/Close/Punct/Inner) → inter-class spacing
  table → per-construct box rules → `Frame`.
- Ship a MATH font: Latin Modern Math (~400KB) or STIX Two Math (~1MB), both OFL. Consistent
  with the two faces we already bundle for determinism (`text.rs:17`).
- Estimate: parser ~600 lines, layout ~1200. Bounded, dependency-light, no unstable APIs.

---

## 3. The Lua seam — how work is divided

**Rule: the declaration crosses the boundary, the data does not.**

### 3.1 Never expose scalar math from Rust

An mlua call costs ~200–500ns. Luau's builtin `math.sin` is a VM fast path at ~10ns. So
`rust.sin(x)` in a Lua loop is **20–50× slower** than doing nothing at all. A boundary crossing
only pays when it amortizes over many elements.

**Ship `vec.sin(buf)`, never `rust.sin(x)`.**

### 3.2 The ladder

| tier | who loops | crossings/frame | vocabulary | ceiling |
|---|---|---|---|---|
| Luau scalar math | Lua | 0 | anything | ~1k |
| **bulk ops on buffers** | Lua orchestrates, Rust loops | ~10s | anything composable | ~100k |
| declared sims | Rust entirely | 1 | **closed** | millions |

Tier 2 is the target. It's the NumPy model — Lua still writes the algorithm, in terms of whole
arrays instead of scalars. Tier 3 is for two or three known-hot things only; make it the general
mechanism and you've rebuilt a rigid engine and lost the reason Lua is there.

### 3.3 Luau already has the primitives

- **`Buffer`** — `mlua-0.10.5/src/buffer.rs:13`, native Luau buffer as `Value::Buffer`, with a
  zero-copy `&[u8]` view from Rust (`conversion.rs:648`). **The GC never walks it.**
- **`Vector`** — `src/vector.rs:12`, a *primitive value type*, not a table. No allocation, no GC
  pressure, arithmetic on the VM fast path. Note it is **f32**, not f64 (Luau scalars are f64).

Storage is Rust-owned, Lua holds handles. Allocate once, mutate in place, and the payload never
enters Luau's heap. Also keeps the sandbox intact — bounds-checked ops on an opaque handle,
never a raw pointer.

### 3.4 Precision

Identical for scalars — Luau numbers are f64 (`crdt.rs:37` converts straight through), kurbo is
f64, and both sides feed the same vello path. Two places f32 bites:

- **Infinite canvas zoomed deep at a far offset** — coordinate magnitude eats the mantissa.
  Standard fix: f64 world origin, subtract before handing to f32.
- **Long-running integrators** — keep sim state in f64 buffers, use f32 only for what's drawn.

---

## 4. Animation

### 4.1 Two kinds, different machinery

| | property transitions | parametric |
|---|---|---|
| CSS analogue | `transition` | `@keyframes` |
| animates | a property of an element | the content itself |
| state | `Store` keyed id + `Slot` | just `t` |
| status | built (`animation.md` §14.1) | not possible from Lua |

Parametric is nearly free — the view tree is rebuilt every frame anyway, so animation is a pure
function of the clock. It needs exactly two things: **a sub-second monotonic clock** (from
`Runner.start.elapsed()` at `lib.rs:189`, *not* `SystemTime` — a wall clock jumps) and
**`repaint` in the Lua props table**.

### 4.2 Springs replace duration tweens

`Transition { progress, target, duration }` (`anim.rs:1`) walks toward the target at constant
speed. Flip the target mid-flight and it reverses instantly — a visible velocity discontinuity.
Flick a card, catch it, release: a tween restarts, a spring carries velocity through.

```rust
Spring { value, velocity, stiffness, damping, target }
```

Semi-implicit Euler, ~10 lines, same `tick(dt)` shape, same `Store`/`Slot` home. No duration to
pick — tune stiffness once and the whole app feels coherent. This is the single highest
value-per-line change in this document, and it serves both button feel and drag/throw gestures.

### 4.3 Morphing

Because Frames are nested data with stable structure, you can lay out two expressions, match
subterms by path, and lerp each matched group's transform:

```
(a+b)^2  →  a^2 + 2ab + b^2      -- with the `a` sliding to its new home
```

~100 lines, and it generalizes for free — the same interpolator morphs a bar chart when its
data changes. Impossible with closures: two paint calls have no identity to compare.

---

## 5. Physics

### 5.1 For rope, verlet beats a physics engine

In rapier a rope is a chain of rigid bodies and joints — many bodies, solver iterations, and
still stretchy without tuning. In verlet/PBD it's N points plus distance constraints relaxed
3–5 times per frame: **~50 lines**, stable, and cloth/hair/chains are the same code.

| | use | effort |
|---|---|---|
| **verlet / PBD** | rope, cloth, hair, chains, soft bodies | ~1–2 days, no dependency |
| **`rapier2d` 0.35.2** | stacking, friction, convex collision, real joints, CCD | ~1 week to expose well |

Most of what makes a UI feel alive is springs + verlet, not rigid-body dynamics.

### 5.2 If we do bind rapier

The binding is easy — arena handles into `RigidBodySet`/`ColliderSet` map onto the same
Rust-owns-storage/Lua-holds-handles pattern. The trap is reading results back: 1000 body
transforms per frame is 1000 crossings.

```lua
world:step(dt)            -- 1 crossing
ui.bodies(world, style)   -- Rust reads transforms → Frame directly
```

### 5.3 Two constraints specific to us

- **Physics is view state, not document state.** Two clients running the same sim *will*
  diverge — float behaviour differs across platforms and dt differs per machine. Only the seed
  and parameters go in the CRDT. Same call already made for table filters.
- **Hot reload must preserve the world.** A physics world is retained state; reloading
  `main.lua` shouldn't reset it. Same problem as a retained Frame cache — solve once, not twice.

---

## 6. Filters

Checked against vello 0.9. Two tiers, and the first is bigger than expected.

**Free — pure Frame metadata, zero new rendering infrastructure:**

| filter | call |
|---|---|
| group opacity | `push_layer(…, alpha, …)` — `scene.rs:105` |
| blend modes (multiply, screen, overlay…) | `push_layer(Mix + Compose)` |
| clip to any shape | `push_clip_layer` — `scene.rs:192` |
| soft / gradient masks | `push_luminance_mask_layer` — `scene.rs:154` |
| drop shadows | `draw_blurred_rounded_rect` — `scene.rs:256` |

That's most of CSS `filter` + `mix-blend-mode`, as a `filter` field on `Group` mapped straight
to `push_layer` arguments. Note this is also exactly what `animation.md` §8b needs for correct
group opacity — same mechanism, build it once.

**Needs a render-to-texture pass — a separate project, not part of this one:**
blur of *arbitrary* content (the blur above is rects only — the shadow fast path), colour
matrix (saturate / hue-rotate / grayscale), glow on arbitrary shapes.

---

## 7. Lua plugins

The defaults library — `chart.ticks`, `chart.palette`, `chart.layout` — should be Lua, not Rust.
**If the standard library has to be Rust, the primitives are wrong.** Writing it in Lua is how
we find out what's missing.

- **Plumbing is 80% there.** `app_src.rs:52` already stores many files; `lib.rs:58` reads only
  `main.lua`. Adding `require` is resolve-in-map + load + cache, ~40 lines.
- **The host writes the loader, so the loader is the security boundary.** Luau's sandbox gives
  no `require`; that's a feature.
- **Two kinds, different policy.** A *pure library* (deterministic functions, no state, no IO)
  needs no caps at all — start there. A *capability plugin* needs the full caps story.
- **Resolve at workspace scope, not app scope.** If every app imports its own palette, we've
  rebuilt the inconsistency the library was meant to fix — two agent-built charts, two palettes,
  one dashboard, neither wrong. The library is a file in the workspace; the workspace index
  points at it. No new concept.
- Plugins share the app's per-frame interrupt budget. Fine for pure functions; worth knowing.

**Why a defaults library at all:** the gap between a chart that's *correct* and one that's
*good* is axis ticks at `0/25/50/75` not `0/33.33/66.67`, labels that don't collide at 40
categories, palettes legible in both themes, consistent spacing, sane empty states. None of that
is math — it's taste encoded as rules. An agent writing each chart from scratch nails the
geometry and misses all of it.

---

## 8. Build order

**Near-term — reactive containers.** Cheapest visible win, and it forces the generalization
everything else needs. Do this before the substrate.

- [ ] `Driver::Press` beside `Driver::Hover` — ~15 lines, hover already works this way.
      Closes `animation.md` §13 Phase B's unchecked "press feedback (stretch)".
- [ ] `Transition` → `Spring` — ~10 lines. Press/release is *the* interruptible case.
- [ ] **Fix press identity first.** `paint.rs:69` identifies the pressed element by comparing
      rects. Scale a button on press and its rect changes, so it stops matching itself and press
      drops mid-interaction. Key by `Id` before touching transforms.
- [ ] `Affine` on `Placed`. `animation.md` §8 deferred this for a stated reason — hit-testing
      needs the inverse. For scale-about-a-centre that inverse is trivial; do the narrow version,
      not general Affine, and say so.
- [ ] Click position on `on_click` — `on_right_click` already carries `(f32, f32)` (`el.rs:243`).
- [ ] `Slot::Anim(name)` replacing the closed `Slot` enum. Today each effect costs four edits
      (Slot variant + Behaviour field + El builder + prop). Ripple is #6, glow #7. Same
      closed-enum→open move as `ChartKind`.

Ripple then falls out of the last three as a **declaration**, not a Rust feature.

**Three unblockers** — small, and they gate rope, particles, charts, and math alike:

- [ ] `Path(BezPath)` arm in `paint.rs` — vello already does it, just unplumbed
- [ ] sub-second monotonic `now()` from the frame clock
- [ ] `repaint` in the Lua props table

**Then, roughly in order of value:**

- [ ] `Frame` + vello renderer (built together with `animation.md` §8b's `PaintItem` stream)
- [ ] verlet (~50 lines) — rope on screen is the demo that proves the whole path
- [ ] `filter` on `Group` — a day once Frame exists
- [ ] buffers + bulk ops
- [ ] `require` + the Lua defaults library
- [ ] math engine (~1800 lines)
- [ ] `rapier2d`, render-pass filters — later or never

Cheapest real demo of the whole direction: **spring + Path + verlet = a draggable rope**,
without touching Frame, buffers, or any dependency. Good way to test the feel before committing.

---

## 9. Open questions

- **Does `Frame` subsume `animation.md`'s `PaintItem`, or sit above it?** They overlap heavily.
  Deciding this wrong means building the same thing twice.
- **Interrupt budget for animated apps.** 1M/frame is sized for UI. Tier 2 dies around ~50k
  elements for a reason unrelated to rendering. Raise it, or make it per-app?
- **When does `Frame` land?** The argument for before M2 is that M2's value *is* agent-built
  visuals, and M3 (math blocks) and M4 (canvas shapes) both need it. The argument against is
  that a closed chart enum ships faster **once**.
- **Editable styled text** stays out of scope here — `PlainEditor` is single-style by design,
  and marks-in-Loro is the M3 doc-editor problem, not this one.

## 10. Decisions log

- **Frame over closures.** Closures can't be measured, nested, cached, diffed, or exported.
- **Glyph ids over strings** in Frame — stretchy delimiters need unencoded glyphs.
- **Own the math engine** rather than typst/ReX — `ttf-parser` removes the reason to depend.
- **MathML Core over the TeXbook** — written against the same MATH constants.
- **Springs over duration tweens** — interruption with velocity is the whole point.
- **Verlet over rapier for rope** — ~50 lines vs a week, and better suited.
- **Physics is view state** — never synced through the CRDT.
- **Bulk ops, never scalar FFI** — a crossing must amortize.
- **Plugins resolve at workspace scope** — app scope reintroduces inconsistency.
