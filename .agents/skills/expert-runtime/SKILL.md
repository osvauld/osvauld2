---
name: expert-runtime
description: "Reviewer for the osvauld2 runtime crate (runtime/) — the UI substrate. Use when a diff touches runtime/src: El builders, layout, paint, text, editor, drag, scroll, animation, state store, ids."
---

# Runtime review

Read the diff cold against this checklist. Output `BLOCKER` / `SHOULD` / `NOTE`; you advise,
the user judges. `runtime/src/lib.rs`'s `//!` header and `docs/architecture.md` § invariants
are the context.

1. **Paint order == reverse hit-test order**, both from tree order. Anything that emits
   out-of-order or hit-tests out-of-order is a BLOCKER. Overlays paint last, hit first.
2. **One vocabulary.** A new element capability must be reachable from both front-ends:
   an `El` builder AND (if Lua-facing) a prop in `app_host`'s registry. An `El` feature
   only Rust screens can reach is a NOTE (pending); only Lua can reach is a BLOCKER.
3. **The store.** Retained state is `Store::get_or` keyed `(Id, TypeId, Slot)` — entries
   registered by use during the frame, `sweep()` drops the unregistered. New retained state
   must join this pattern or it leaks forever. Ids come from the author (`El::id`), so
   anything needing continuity must *require* an id (walk enforces for scroll).
4. **On-demand paint.** The loop is `ControlFlow::Wait`; anything that happens off-frame
   (worker thread, timer, subscriber) must poke the `EventLoopProxy` or it is invisible.
   Silent-non-repaint is a BLOCKER.
5. **Immediate description.** `view` is a pure description; no retained widget state may be
   mutated from `view`. Heavy things are islands owning their hot state.
6. **Hot paths.** Layout/paint run per interaction; per-element allocations and per-frame
   `format!`s need a cost argument. `walk` (app_host) is the benchmark to keep in mind.
7. **Known traps** (`docs/design/view-and-interaction.md` §0 lists more): press identity
   compares geometry not `Id`; `now()` is whole seconds; `Easing` is a closed enum; colour
   lerp is sRGB. Don't "fix" these in passing — they are recorded decisions with designs.
8. Tests live in `src/<module>/tests.rs` via `#[cfg(test)] mod tests;` — source files stay
   free of test code.
