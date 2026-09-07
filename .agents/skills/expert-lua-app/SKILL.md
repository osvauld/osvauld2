---
name: expert-lua-app
description: "Reviewer and author-guide for osvauld2 Lua apps (.lua files in shell2/src/kanban/, demo_apps/, or any uploaded app). Use when writing or reviewing app code against docs/lua-apps.md."
---

# Lua app review

Read `docs/lua-apps.md` fully before writing or reviewing app Lua — it is the contract, and
this skill is its checklist. Output `BLOCKER` / `SHOULD` / `NOTE`; you advise, the user
judges. The kanban app (`shell2/src/kanban/`) is the canonical corpus; the round-trip suite
must stay green.

1. **Three kinds of table**: elements come only from `ui.*` constructors; a bare nested
   table splices; `doc.map`/`doc.list`/`doc.text` values are data. Hand-written `tag =` is
   a BLOCKER. Children are positional entries; their order is meaning.
2. **State home** (the three-way choice in the guide): gesture state → locals; element-keyed
   or reload-surviving state → `ui.state`; claims about the work (names, widths, content) →
   the document. A resize width stored in viewer state permanently is the classic miss.
3. **Doc discipline**: read the mirror *first*, write after (it's a frame behind); address
   by stable `id = uuid()`, never by position; seeds guarded at module scope; `:move`'s
   target is the post-removal index; collect-then-delete when iterating; unchanged writes
   are no-ops (don't guard them).
4. **One write per finished gesture.** Drags accumulate in viewer state across
   `start`/`move`; a single commit lands at `end`/`"release"`. Sixty writes per drag is a
   BLOCKER.
5. **Handlers**: `on_drag`/`on_drop`/scroll need an `id`; drag coords are grab-relative,
   drop coords normalized (0–1) with phases `"over"`/`"release"`. Handler tables refill
   every frame — no caching closures across frames.
6. **Layout honesty**: unknown props are errors; `min_w`/`max_w` bound a `grow` child (a
   drag-clamp is policy, bounds are physics — both, not either); `no_wrap` for labels;
   scroll containers need `id` + `grow`.
7. **Readability for the guide**: if a pattern here is new (not in the guide's patterns
   table), NOTE it — the guide should grow with the corpus.
