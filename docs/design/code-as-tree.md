# Code as a tree

**Status:** design, agreed in outline. One spike outstanding before any code.
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

---

## 7. Costs that land on the hot path

**The printer runs every reload**, not on save — the VM needs source text, so tree → text is
between every structural edit and the next frame. The reload trigger itself already exists
(`Source`, `reload_if_stale`), so this slots into machinery that works.

**`_nid` costs a boundary get per element per frame.** This is exactly the cost the existing `line`
breadcrumb is `dev`-gated to avoid: the comment in `walk` records that it is ~80% of a frame's Lua
cost, and there is a `cost_curve` test measuring it. Carrying ids always means paying it always.
Measure against that test before assuming it is fine.

**The source doc changes shape.** Today `files` is a map of `LoroText`
(`src.doc.get_map("files")`). A tree of nodes is a different structure, and it touches
`LuaApp::build`, `modules::install`, the MCP write path, and the reload trigger's assumptions.

---

## 8. Open questions

1. **What is a node?** Not tree-sitter's concrete tree — nobody wants a CRDT container per comma.
   A semantic subset: statements, calls, table literals, fields, literals, function bodies. The
   subset is a schema decision and it is the bulk of the design work.

2. **Trivia.** Comments and blank lines have no home in a syntax tree, and losing them is the
   classic way these systems fail. Leading/trailing trivia per node is the standard answer. The
   requirement is *preservation*, not byte-fidelity — the printer owns formatting.

3. **Totality, and the opaque node.** Every Lua construct must be representable or import corrupts
   files, silently. The mitigation is an **opaque node** holding raw text for anything the schema
   does not model, so coverage can grow without risking data. This is not optional.

4. **Addressing, for the agent.** It has to name a node to edit one. Ids are visible in the printed
   source, so it can read them — but a *path* (`view/children/2/runs/0`) is more legible and needs
   no annotation, at the cost of shifting when a sibling is inserted. Probably: paths within a turn,
   ids for anything stored. Undecided.

5. **Loro representation.** `LoroMap` per node, children in a movable list. `loro-gotchas` applies
   directly: eager containers before undo, delete hides subtrees rather than removing them, handles
   need re-resolving. Undo across structural edits is its own design.

---

## 9. The spike

Before any schema, one question, because its answer can change the plan:

> Parse `shell2/src/kanban/main.lua` (342 lines of real code) with tree-sitter-lua, build the
> semantic tree, print it back. Then parse *that* and print again.

Pass conditions:

- **Print is idempotent** — the second print equals the first. (Not byte-identical to the input:
  the printer normalises formatting, and that is intended.)
- **Comments survive** the round trip, in the right places.
- **It still runs** — the printed source loads and behaves identically.

tree-sitter-lua is already vendored in this repo through inkjet (`code_highlight`, `code_editor`),
so the parser is a known-good dependency rather than new risk.

**Second spike, only if the first passes:** replace one function body the way an agent would,
reparse that subtree, and confirm untouched siblings keep their ids. That is what makes "surgical"
true rather than aspirational.
