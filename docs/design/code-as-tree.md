# Code as a tree

**Status:** design, agreed in outline. Parser chosen and measured (§8½). One spike outstanding
before any code — now scoped to the lowering alone.
**Related:** `interactive-document.md` §3 (data and behaviour), `merge-referee.md`,
`view-and-interaction.md`. Memory: *code-as-CRDT* moves out of "future directions" and becomes
the substrate this describes.

---

## 0. The question this answers

Right-click a label and change its colour. Drag a container's edge and resize it. Ask an agent to
restyle one card. All three have to end with **the app's own source changed**, and the next frame
showing it.

Today none of them can, and the reason is not the gesture — it is that nothing connects a pixel
back to the code that made it. `app_host` stamps `t.line = debug.info(2, "l")` on every node
(`app_host/src/lib.rs:462`), `walk` reads it back, formats it into an error breadcrumb, and drops
it. Even kept, a line number is too coarse: two `ui.text` calls on one line are indistinguishable,
and Luau's `debug.info` offers no column.

The fix is not better lookup. It is that **code stops being text**.

---

## 1. The decision

> **The tree is the artifact. Lua text is an import format and an export projection.**

- **Import**: upload a `.lua`, it is parsed once, node ids are generated, the tree replaces
  whatever was there. Wholesale.
- **Edit**: every incremental change — agent, gesture, human — is a structural operation on nodes.
- **Export**: the tree prints back to Lua, for reading and for feeding the VM.

The artifact lives inside the app. A `.lua` file is something you can produce from it and something
you can replace it with, but not a thing you edit alongside it.

**What this buys, and it is the whole point:** there is no text→tree reconciliation. The parser
runs once per import and never appears in the edit loop. Preserving node identity across an
arbitrary text rewrite — the problem that makes every structural editor hard — is not a problem
this system has, because it does not accept arbitrary text rewrites.

---

## 2. The one primitive

Every gesture reduces to:

> **set property `P` of node `N` to value `V`**

| gesture | node | property |
|---|---|---|
| right-click → colour | the run's table literal | `color` |
| drag a container's edge | the `ui.col` call's table | `w` |
| agent restyles a card | the card function's body | (subtree replacement) |

That three unrelated-looking features collapse to one operation is the strongest argument that the
depth is right. At block granularity none of them can be expressed — you can only say "here is a
new block", which rewrites everything inside it.

---

## 3. Identity

**Every node has an id.** Generated at import, stable across every structural edit, regenerated on
reimport.

Ids are visible in the printed Lua:

```lua
ui.text({ "hello", color = "#fff", _nid = "k3f9" })
```

Metadata a human can see. One projection, not two — a stripped projection is a later convenience,
not a requirement.

**Reimport resets identity.** Uploading a file replaces the tree, so every id in it changes and
anything pointing into it breaks: a pinned comment, a live-block binding, a stored agent reference.
This is a rule, and it must be visible as one rather than discovered:

> **Raw text replacement resets identity. Structural edits preserve it.**

---

## 4. How code reaches the VM

```
tree ──print──▶ Lua text (with _nid) ──vm.load──▶ VM ──view()──▶ table ──walk──▶ El ──▶ Placed
                                                                    │
                                                      _nid rides through here
```

`walk` carries `_nid` onto `El`; `emit` carries it to `Placed`. A click hands back the exact node.
No parsing at click time, no line numbers, no ambiguity — **provenance falls out of code
generation** rather than being reconstructed.

This is the mechanism that does not exist at block granularity, and it is why the depth pays.

---

## 5. Three edit paths

**Gesture** — `set(nid, key, value)`. One property, known exactly. Cheap, precise, and what the
right-click and the resize both compile to.

**Agent** — `replace(nid, lua)`. The agent writes *Lua text* for one subtree; it is parsed and
grafted in place. This matters: agents are good at writing code and bad at emitting field-level
mutations, so the natural unit is a snippet, not a setter. The edit is still surgical, because it
is scoped to a node — the parent, the siblings and their ids all survive.

**Import** — `write_file(path, lua)`. Wholesale, resets ids. Bootstrap and recovery.

The existing MCP surface only has the third. The first two are new operations, and their vocabulary
is an open question (§8).

---

## 6. What this trades away

**Git, for app source.** The artifact is a CRDT, so what you would commit is an export, and
checking one out and reimporting resets every id. The CRDT's own history replaces version control
here. This is consistent with the direction — it is not an accident to be worked around later.

**Editing in an external editor.** By design. Text goes in and text comes out; the editing happens
inside.

**Formatting as an authored property.** The printer owns layout. What a human typed as spacing does
not survive; comments do (§8).

The parser preserves formatting perfectly (§8½), so this loss is entirely ours, and **the decision
is to accept it.** The schema does not carry quote style, number literal spelling, trailing commas,
blank-line placement or named-field order. The printer picks one form and always picks the same one.

This costs less than it looks like, because **normalisation happens at the door**: import parses,
lowers, prints once, and stores the canonical form. Every reprint after that already matches, so a
one-property edit produces a one-property diff. The reformatting is a single event on the file you
uploaded, not a tax on every edit.

**The exception, and it is a correctness one rather than a taste one: positional order is meaning.**
In `ui.text({ "Add card", color = "#ffffff" })` the string is `[1]`, and in this DSL positional
entries are the child list — reordering them is a different UI. So a table lowers to an *ordered
list of entries*, never to a map of keys. Modelling it as a map loses child order silently, and
silently is what separates a bug from a trade.

---

## 7. Costs that land on the hot path

**The printer runs every reload**, not on save — the VM needs source text, so tree → text is
between every structural edit and the next frame. The reload trigger itself already exists
(`Source`, `reload_if_stale`), so this slots into machinery that works.

Measured, and it is not the thing to worry about: full parse **and** print of `main.lua` (342 lines)
is **1.0 ms** release-mode. Our own print will differ, but the order of magnitude is set, and reload
is not a per-frame path. The cost that matters is the next one.

**`_nid` costs a boundary get per element per frame.** This is exactly the cost the existing `line`
breadcrumb is `dev`-gated to avoid: the comment in `walk` records that it is ~80% of a frame's Lua
cost, and there is a `cost_curve` test measuring it. Carrying ids always means paying it always.

**Measured, and it is affordable.** `cost_curve` already sweeps `dev` both ways, so the delta was
readable without writing anything:

| | ns/element |
|---|---|
| the `dev` breadcrumb — two gets, a `format!`, two allocations | +490 to +820 |
| one additional string prop (the colour probe) | +50 to +130 |

`_nid` is the second shape rather than the first: a single get, no formatting, no allocation. Call
it ~100 ns/element, which is **0.1 ms at a thousand elements against a 16.7 ms frame** — and the
fat version already ships as an option. Provenance is not the ceiling; `walk`'s existing
2–3 µs/element is, and element count is what actually bounds a frame.

The consequence for sequencing: there is no need to prove the read path before building the write
path. Ids can be generated, stored and printed first, and `walk` picks them up when they exist.

**The source doc changes shape.** Today `files` is a map of `LoroText`
(`src.doc.get_map("files")`). A tree of nodes is a different structure, and it touches
`LuaApp::build`, `modules::install`, the MCP write path, and the reload trigger's assumptions.

---

## 8. Open questions

1. **What is a node?** Not full-moon's concrete tree — nobody wants a CRDT container per comma.
   A semantic subset: statements, calls, table literals, fields, literals, function bodies. The
   subset is a schema decision and it is the bulk of the design work.

2. ~~**Trivia.**~~ **Largely answered** (§8½). full-moon attaches leading/trailing trivia to every
   token, so comments arrive already anchored and already positioned. What remains is not a model
   but a policy: when an edit replaces a node, which of its trivia belongs to the replacement and
   which to the hole it left.

3. **Totality, and the opaque node.** Every Lua construct must be representable or import corrupts
   files, silently. The mitigation is an **opaque node** holding raw text for anything the schema
   does not model, so coverage can grow without risking data. This is not optional.

   One half of it is now cheap: malformed input never reaches the schema at all, because the parser
   rejects it (§8½). The remaining risk is narrower and more specific — input that parses fine but
   that *our lowering* has no case for. That is the one the opaque node exists for.

4. **Addressing, for the agent.** It has to name a node to edit one. Ids are visible in the printed
   source, so it can read them — but a *path* (`view/children/2/runs/0`) is more legible and needs
   no annotation, at the cost of shifting when a sibling is inserted. Probably: paths within a turn,
   ids for anything stored. Undecided.

5. **Where ids come from.** Generated by us (`k3f9`), or Loro's own container ids? Loro's are free
   and unique, but they are peer-scoped and they leak the CRDT into printed source that users and
   agents read and quote. Leaning to generating our own, so an exported file's ids mean the same
   thing on every peer.

6. **Loro representation.** `LoroMap` per node, children in a movable list. `loro-gotchas` applies
   directly: eager containers before undo, delete hides subtrees rather than removing them, handles
   need re-resolving. Undo across structural edits is its own design.

---

## 8½. The parser: settled

**`full_moon`, with the `luau` feature.** Not tree-sitter.

tree-sitter was the first answer here for one bad reason — it is already vendored through inkjet
(`code_highlight`, `code_editor`), so it looked free. Checking it turned up two problems that
convenience does not cover:

- **The vendored grammar is tree-sitter-*lua*, and this VM is Luau.** `app_host/Cargo.toml:11` is
  `mlua = { features = ["luau"] }`. No current `.lua` in the repo uses Luau-only syntax, so nothing
  is broken today — but an agent can emit `x += 1` or a type annotation at any moment, the VM will
  run it, and a Lua 5.x grammar will hand back an `ERROR` node for code that works.
- **tree-sitter is error-tolerant by design.** Correct for a highlighter, wrong for an import gate:
  it yields a tree rather than refusing, and a typo becomes an opaque node instead of a rejection.

full-moon inverts both, and it exists specifically to parse → mutate → print without loss, which is
this document's requirement rather than a bonus.

### What was measured

16 real files, 2,007 lines — all of `shell2/src/kanban` plus all of `app_engine/examples`:

| check | result |
|---|---|
| parse → print equals **the original**, byte for byte | 16 / 16 |
| print is idempotent | 16 / 16 |
| comments preserved | 200 / 200 |
| Luau syntax parses, losslessly — annotations, `+=`, `continue`, string interpolation, typed and generic functions, `export type` | 7 / 7 |
| malformed input **rejected** (truncated, unclosed block, garbage, unclosed string) | 4 / 4 |
| trivia reachable per token | yes — `-- leading comment` arrives as leading trivia of `local` |
| parse + print, 342 lines, release | 1.0 ms |

The first row is stronger than this document assumed. Round-tripping is not merely idempotent, it is
**lossless** — which is why §6's formatting trade is now a schema decision rather than a given.

### What this did *not* settle

This measured full-moon's own tree, and full-moon's tree is not our tree. The open question is
whether the **lowering** round-trips — and a lowering is lossy by construction, which is exactly
what makes it the interesting half. What the check bought is that the layer underneath is not a
source of surprises, and that the dialect risk is gone.

---

## 9. The spike

Before any schema, one question, because its answer can change the plan:

> Parse `shell2/src/kanban/main.lua` (342 lines of real code) with full-moon, **lower it to the
> semantic tree**, and print *that* back. Then parse the result and print again.

Pass conditions:

- **Print is idempotent** — the second print equals the first. (Not byte-identical to the input:
  our lowering normalises formatting, and that is intended.)
- **Comments survive** the round trip, in the right places.
- **It still runs** — the printed source loads and behaves identically.

The parse step is now known-good (§8½), so the spike measures only the lowering, which is the part
that can actually fail. Its cost dropped accordingly: the corpus, the harness and the pass/fail
conditions all exist, and what has to be written is the schema and the printer.

**Second spike, only if the first passes:** replace one function body the way an agent would,
reparse that subtree, and confirm untouched siblings keep their ids. That is what makes "surgical"
true rather than aspirational.
