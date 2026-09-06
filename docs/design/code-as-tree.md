# Code as a tree

**Status:** parser chosen and measured (§8½), schema drafted from a census (§10), **spike built and
passing** (§9.1, the `lua_tree` crate). §11 revises what actually belongs in the tree; §12 is why
the tree survives that revision. Remaining unbuilt: the Loro representation, the operations, and
`_nid` carried onto `El`.
**Related:** `interactive-document.md` §3 (data and behaviour), `merge-referee.md`,
`view-and-interaction.md`. Memory: *code-as-CRDT* moves out of "future directions" and becomes
the substrate this describes.

---

## 0. The question this answers

Right-click a label and change its colour. Drag a container's edge and resize it. Ask an agent to
restyle one card. All three have to end with **the app's own source changed**, and the next frame
showing it.

> **Revised by §11.** Only the third of those turns out to be a source edit. Resize and text
> editing land in a per-viewer override layer and in document data respectively, and never touch
> the app. What remains genuinely code — agent restructuring, authoring a default, anything
> touching behaviour — is narrower than this section assumed, and §12 is the argument that it is
> still enough to need the tree.

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

> **Disproved by §13.** The `_nid` in the diagram below does not survive contact with the host: a
> table constructor is not always an element, and the seed rejects the key before `view()` runs.
> The rest of the chain — `walk` → `El` → `Placed` → click — still stands; what changes is where
> the id comes *from*.

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

1. ~~**What is a node?**~~ **Answered — §10.** Drafted against a census of the real corpus rather
   than the Lua grammar: 22 node kinds plus an opaque escape.

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

5. ~~**Where ids come from.**~~ **Decided: we generate our own.** Loro's container ids are free and
   unique but peer-scoped, and they would leak the CRDT into printed source that users and agents
   read and quote. Ours mean the same thing on every peer, and survive export and reimport as text.

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

> Condition 3 is now met in its strong form, and it found something — **§13**.

Before any schema, one question, because its answer can change the plan:

> Parse all of `shell2/src/kanban` (6 files, 1,213 lines — every line of Lua this implementation
> has) with full-moon, **lower it to the semantic tree**, and print *that* back. Then parse the
> result and print again.

Pass conditions:

- **Print is idempotent** — the second print equals the first. (Not byte-identical to the input:
  our lowering normalises formatting, and that is intended.)
- **Comments survive** the round trip, in the right places.
- **It still runs** — the printed source loads in Luau, `view()` is called, and the resulting element
  tree is identical. "Parses" is too weak a bar; `app_host` can do the real check headlessly.
- **Opaque rate is 0%** — every construct in the corpus lowers to a real node (§10.7).

The parse step is now known-good (§8½), so the spike measures only the lowering, which is the part
that can actually fail. Its cost dropped accordingly: the corpus, the harness and the pass/fail
conditions all exist, and what has to be written is the schema and the printer.

**Second spike, only if the first passes:** replace one function body the way an agent would,
reparse that subtree, and confirm untouched siblings keep their ids. That is what makes "surgical"
true rather than aspirational.

### 9.1 Result: it passes

Built as the `lua_tree` crate — `schema.rs`, `lower.rs`, `print.rs`, 11 tests. All four conditions
hold across the corpus, plus two the build added:

| | |
|---|---|
| print is idempotent | ✓ |
| comments survive | ✓ — asserted by name, so a failure says *which* comment |
| printed source still parses | ✓ *(weak form — see below)* |
| opaque rate 0% | ✓ |
| ids survive a reprint | ✓ |
| a fresh import gets fresh ids | ✓ |

**Two real bugs, both in trivia, both found by the tests rather than by reading:**

1. **`Node::tokens()` yields in struct-field order, not source order.** A table's braces are
   visited before its fields, so `.last()` lands on the final field rather than on `}` — and a
   statement's trailing comment lives on `}`. Empty tables happened to work, which hid it until
   `_nid` made every table non-empty. Position is the only reliable way to ask which token is
   physically last.
2. **`Last` had nowhere to put trivia.** `-- ROOT` above a file's `return function()` is a comment
   on the last statement, and `Last` was a bare enum. It is a struct with `leading`/`trailing` now,
   the same shape as `Stmt`.

**One sabotage passed, which is its own finding.** Sorting a table's entries — destroying child
order, the thing §6 calls *meaning* — was caught by nothing. The test case was degenerate: its
entries were already in sorted order. Rewritten to interleave named and positional entries so that
neither sorting by name nor grouping by kind survives it.

That gap points at the real one: **nothing yet checks that the printed program means what the input
meant.** Condition 3 is implemented in its weak form, "still parses". The strong form — load in
Luau, call `view()`, compare the element tree — needs `app_host`, and it is the next thing to add,
because it is what would have caught the sorting sabotage on its own.

### 9.2 What the printer costs in readability

Every table gets a printed `_nid`, and the corpus has 184 of them. That is the §3 decision working
as designed — ids are visible metadata — but it is worth seeing at scale before it is settled:

```lua
ui.text({ "Add card", color = "#ffffff", font_size = 13, _nid = "b09bf97d" })
```

A stripped projection for reading remains the escape hatch (§3), still not required.

---

## 10. The schema

Drafted from a **census of the corpus**, not from the Lua grammar. The corpus is
`shell2/src/kanban` — **6 files, 1,213 lines**, which is all the Lua this implementation currently
has. Designing against the grammar produces a schema for a language; designing against the census
produces one for *this* code, and names what is missing rather than leaving it implied.

`app_engine/examples` is deliberately **not** in the corpus. It is sthalam's DSL, from the previous
implementation, and it is a reference-only crate. It was censused separately as corroboration and is
reported below as such — never as a fixture, and never as a coverage target.

### 10.1 What the corpus actually uses

| statements (438) | | expressions (1,786) | | table fields (635) | |
|---|---:|---|---:|---|---:|
| Assignment | 82 | Var | 594 | **NameKey** `k = v` | 490 |
| LocalAssignment | 82 | String | 223 | **NoKey** positional | 145 |
| If | 70 | BinaryOperator | 220 | *ExpressionKey* `[k] = v` | **0** |
| FunctionCall | 66 | Number | 211 | | |
| Return | 53 | TableConstructor | 184 | | |
| FunctionDeclaration | 26 | FunctionCall | 146 | | |
| LocalFunction | 22 | Symbol | 102 | | |
| NumericFor | 19 | UnaryOperator | 60 | | |
| Break | 10 | Function (anon) | 40 | | |
| GenericFor | 8 | Parentheses | 6 | | |

**Corroboration, from the old implementation.** The same census over sthalam's 17 `app_engine`
examples — 2,490 lines of a *different* DSL by the same author — turns up **exactly this construct
set**. Same ten statement kinds, same ten expression kinds, still zero `ExpressionKey`, still zero
opaque. Two generations of UI-DSL Lua agreeing on the same twenty constructs is the best available
evidence that this list is the shape of the idiom rather than the shape of one app. It is evidence
only; the fixtures and the coverage assertion are kanban's alone.

Three things fall out of this that reading the grammar would not have told us:

- **Table fields have two shapes, not three.** `[expr] = value` never appears. `Entry` is therefore
  `Named | Positional`, which is the ordered-entries decision from §6 arriving as a two-variant enum
  rather than a general map.
- **`Do`, `While` and `Repeat` are entirely unused**, as is every Luau-only statement. They go
  straight to opaque with nothing lost today.
- **Positional entries are the child list.** 444 of them, and they are what `ui.col({ ..., ui.text(…) })`
  is made of — which is why their order is meaning and not formatting.

### 10.2 Two structural decisions

**Depth is total; ids are not.**

The tree is structured all the way down — that was the call in §2, and surgical edits need it. But an
`_nid` is *printed* only where a pixel can land, which is table constructors. Two independent
constraints agree on this:

1. **Lua syntax.** You cannot hang a field on a statement or on a string literal. Only a table can
   carry `_nid = "k3f9"`.
2. **Hit-testing.** Only elements are clickable, and in this DSL an element *is* a table
   constructor — `ui.text({ … })`.

Everything else is reached as **nearest id + path**. So §8's addressing question resolves itself:
ids for what a click produces, paths for everything below it.

**Numbers are doubles.** Luau has no integer subtype, so `13` and `13.0` are the same value and
normalising the spelling is safe. In Lua 5.3+ it would not have been.

### 10.3 The nodes

```
Block  { stmts: [Stmt], last: Last? }          -- a file body or a function body

Stmt   ( + leading: [Comment], trailing: Comment? )
  Local   { names: [Name], values: [Expr] }
  Assign  { targets: [Expr], values: [Expr] }
  Call    { call: Expr }                        -- a call in statement position
  If      { arms: [(Expr, Block)], else: Block? }
  Func    { scope: local|global, name: Path, params: [Name], body: Block }
  NumFor  { name, from: Expr, to: Expr, step: Expr?, body: Block }
  GenFor  { names: [Name], exprs: [Expr], body: Block }
  Opaque  { text }

Last
  Return  { values: [Expr] }
  Break
  Continue                                      -- luau

Expr
  Name   { name }                               -- x
  Index  { base: Expr, key: Expr, dot: bool }   -- C.text, t[i]
  Call   { callee: Expr, method: Name?, args: [Expr] }
  Str    { value }
  Num    { value: f64 }
  Sym    { true | false | nil | ... }
  Table  { id: Nid, entries: [Entry] }          -- ← the only node with a printed id
  Bin    { op, lhs: Expr, rhs: Expr }
  Un     { op, expr: Expr }
  Fn     { params: [Name], body: Block }        -- anonymous
  Paren  { expr: Expr }
  Opaque { text }

Entry  ( + leading: [Comment], trailing: Comment? )
  Named      { name, value: Expr }
  Positional { value: Expr }
```

22 kinds, two of them opaque. `Var` and `FunctionCall` from the census decompose into
`Name`/`Index`/`Call`, which composes cleanly: `C.text` is `Index(Name "C", Str "text")`, and
`ui.text({…})` is `Call(Index(Name "ui", Str "text"), [Table])`. `Paren` earns its place because
`(a + b) * c` is not `a + b * c`.

### 10.4 Where trivia lives

`leading` and `trailing` on **`Stmt` and `Entry` only**. Those are the two places comments actually
occur in the corpus, and both are list members — so a comment travels with the thing beneath it when
that thing moves. A comment buried inside an expression attaches to the nearest enclosing statement.
That is a real loss, and it is the acceptable one: formatting is already normalised (§6).

### 10.5 Loro mapping

| schema | Loro |
|---|---|
| a node | `LoroMap` with a `kind` field |
| `[Stmt]`, `[Entry]`, `[Expr]` | `LoroMovableList` |
| `Nid` | our own generated id, a field on `Table` |

`MovableList` rather than `List` because reordering is a first-class edit — dragging a card between
columns is a *move*, and modelling it as delete+insert loses concurrent edits to the moved subtree.
`loro-gotchas` applies throughout: eager containers before undo, delete hides rather than removes.

### 10.6 What the operations become

- `set(nid, key, value)` — find the `Table` by id, find its `Named` entry, replace `value`. Append
  the entry if absent.
- `insert(nid, index, lua)` — parse to `Expr`, insert a `Positional` entry at `index`.
- `replace(nid, lua)` — parse to `Expr`, swap the subtree. **The root keeps `nid`**; new tables
  inside it get fresh ids. That is precisely what makes siblings survive an agent edit.

### 10.7 The opaque node, and why the schema need not be complete

An `Opaque` node holds the raw source text of a construct the schema has no case for. It parses
(full-moon covers all of Lua), it prints back verbatim, and the tree does not model its insides:

```lua
while running do step() end     -->  Opaque { text: "while running do step() end" }
```

The distinction it buys is the one that matters for every app not yet written:

> The schema does not need to be **complete**. It needs to be **total.**

*Complete* would mean a case for every Lua construct. *Total* means every Lua program is
representable without loss — which opaque delivers today, at zero coverage. So a future app cannot
break import, cannot lose code, and cannot fail to run. It can only be **less editable**, and only
inside the regions using constructs we have not modelled yet.

**The gap is bounded and known**, which is the other half of the reassurance. To go from this
schema to complete is roughly ten variants: `Do`, `While`, `Repeat`, `goto`/`::label::`,
`[expr] = value` fields, and Luau's compound assignment, type declarations, type assertions,
if-expressions and string interpolation. Not an open horizon — a finite list, and every addition is
purely additive, so trees written under an older schema keep working.

**The cost is capability, not correctness.** An opaque region contains no `Table` nodes, so it
carries no ids, so everything inside it is **dark to the UI** — unclickable, unresizable,
unaddressable. The list of what is unmodelled is therefore also the list of what a user cannot yet
right-click.

**So coverage should be measured, not assumed.** The census already computes an opaque rate; it
belongs in the test suite rather than in a throwaway probe — assert it stays at 0% for the corpus,
so the day an app introduces a construct we do not model, a test says so instead of a user finding
a region where right-click silently does nothing. Per-file coverage surfaced in the app itself is
the same idea aimed at the person rather than the build.

---

## 11. What is *not* code: the data/override split

Everything above assumes the gesture ends in a source edit. Working through resize and rich text
showed that most of them do not — and the ones that do are a narrower, clearer set than §0 implied.

### 11.1 Three destinations, not one

| edit | lands in | reload |
|---|---|---|
| document text and its formatting | **data** — `LoroText` + marks | never |
| per-viewer panel width, filters, caret, pending marks | **override layer** | never |
| transient drag state | **override layer** | never |
| agent restructuring, authoring a default, any `on_click` | **the tree** | once |

The rule underneath: **the CRDT holds what a human typed plus structure a human intended**
(§4.2 of `interactive-document.md`), and *which* CRDT depends on whether the thing being edited is
the app or the content the app displays.

### 11.2 The override layer

A map from `nid` (plus path) to a property value, applied to `El` **after `walk` and before
layout**:

```
view() ──walk──▶ El ──[ overrides ]──▶ El′ ──▶ layout ──▶ Placed
```

Lua produced `w = 300`; the shell patches it to `320`. No reparse, no reprint, no `reload()`.

This exists because `reload()` (`app_host/src/lib.rs:193`) is not cheap — it builds a whole new VM,
runs `carry_state`, executes a trial frame, runs `_sweep`, then swaps, and it can *drop*
`ui.state`. A drag at 60fps would mean sixty VM rebuilds a second. The layer makes interactive
editing free and reserves reload for commits.

It has **three tenants**, arrived at independently, which is the argument that it is a real thing
rather than a convenience:

1. resize and other uncommitted authoring gestures, folded into source on release;
2. per-viewer view state that is never folded in — the same call as *filters are per-viewer*;
3. `stored_marks` and the caret — §5.2 already says pending formatting at a collapsed caret
   "cannot live in the CRDT and must not try to".

Overrides can orphan when a nid disappears. `_sweep` already solves exactly this shape for carried
`ui.state`; the same sweep applies.

### 11.3 Two address spaces

A click resolves to **both**, and conflating them is a bug waiting to happen:

> **Code edits address by `nid`. Content edits address by data path.**

The reason is a loop. `for _, b in ipairs(doc.notes.blocks) do ui.rich({ bind = b.text }) end` is
**one code node with one `nid`** rendering **N paragraphs**. The nid says which code drew it and
cannot say which paragraph was clicked. Recolour the font → `nid`. Type a character → path.

### 11.4 Binding, and the handle

The mirror returns a **handle** for text, not a string:

```lua
ui.rich({ bind = doc.notes.body, editable = true })
```

`doc.notes.body` is a `TextRef { path }`. The document's bytes never cross the Lua boundary during
a frame — the runtime resolves the path itself. That keeps §4.1's mediated-read discipline (the
access is recorded, mutation bumps the version) while keeping a large document out of `walk`.

Writability is four independent gates: the tree says *binding, not literal*; the mirror says *text
container, not computed value*; caps say *this app may write there*; and the app says
`editable = true`. All four, or the affordance lies.

The write loop skips Lua entirely — `keystroke → LoroText insert → version bump → next frame
re-resolves → repaint` — which is what lets concurrent typing merge per character instead of
LWW-ing a source literal.

**`Vec<Run>` is a render type, not storage.** Marks are stored; runs are derived per frame and
cached on `hash(text version, marks, width)`. §4.2's rule about outputs covers this directly.

### 11.5 Two operations called "insert"

- **A paragraph in a document** is a *data* insert into a Loro list. The code already says "render
  every block"; no source changes.
- **A container in the app** is a *tree* insert. Source changes, one reload.

So the tree's `insert` is for app structure only, and document editing needs none of it.

### 11.6 Affordance props

`resizable` and `editable` are a third kind of prop: not how an element looks, not what it
dispatches, but **what the editor may do to it**. Worth naming as one category now rather than
inventing them separately later. They split by destination — **`resizable` writes code,
`editable` writes data.**

Declaration is policy and does not replace the tree's check. `resizable` on a `grow = true` box is
a handle producing a value Taffy ignores, so writability is still a precondition. And the bounds
belong in the declaration: `resizable = { min = 160, max = 600 }`, because "what stops it
collapsing" is part of offering the gesture.

### 11.7 Resize needs two layout knobs the DSL does not expose

Taffy needs a concrete value, and **which** value depends on the edge rather than the element:

| boundary | knob | the edit writes |
|---|---|---|
| fixed ↔ `grow` | `size.width` | `w` on the fixed side; the grow side absorbs |
| fixed ↔ fixed | `size.width` ×2 | both, sum preserved — brittle under parent resize |
| `grow` ↔ `grow` | `flex_grow` | ratios on **both** siblings |

**One side of a dragged boundary must be elastic**, or the drag fights the parent and any window
resize undoes it. Row 1 — the sidebar splitter — works with today's layout and is most of what is
wanted.

Row 3 is currently inexpressible: `grow()` sets `flex_grow = 1.0` (`runtime/src/el.rs:458`,
`prop!(grow)`), a flag rather than a ratio, so two `grow` siblings are permanently 50/50. And there
is no `min_size`/`max_size` exposed at all. Both are already `f32` in Taffy; only the DSL flattens
them. Two small additions.

This also sharpens the writability check from §2: a splitter edits **two sibling nodes**, so the
question is what the *edge* resolves to, one level up at the parent — not what the node's own `w` is.

---

## 12. Why the tree still has to be the artifact

§11 removes most of §0's motivation. Rich text and resize both leave the source untouched, so the
fair question is whether the CRDT tree is needed at all, or whether `lua_tree` as a parser over
text storage is enough.

Three levels:

| | what it is | state |
|---|---|---|
| **1** | parser only — parse, splice, print, write the file as text | **built** (`lua_tree`) |
| **2** | ids printed into the source, files still text | small — needs `_nid` carried |
| **3** | the tree itself in Loro | the large one |

Level 2 looks like a resting point and is not one, for a reason specific to this design:

> **A normalizing printer turns a local semantic edit into a wide textual one.**

Change one property, the table crosses the printer's single-line/multi-line threshold, and forty
lines rewrite. Two agents editing genuinely unrelated nodes then produce *overlapping* text diffs,
and a text CRDT merges them into Lua neither wrote. Hand-written code does not have this problem;
printed code does. Text storage is therefore **worse** here than it would be for a normal repo.

And flattening an addressed operation to text before merging discards exactly the precision that
made the edit surgical. `set(k3f9, "fill", …)` and `set(k7a2, "w", …)` merge trivially as *ops* —
different nodes, no conflict — and same-node-same-property is LWW, which is the only correct answer
anyway (§5.3 makes the identical argument for colour).

So the addressing the agent needs and the merge concurrent editing needs are **the same
mechanism**, and Level 3 is where it lives.

**It is also smaller than it first appears**, because of §11: the tree holds *app structure only*.
Document content is separate `LoroText` with its own merge. For kanban that is 184 tables — a
small, low-churn object that changes when someone edits the app, not when someone types. Node-
granular undo and click-to-node provenance come with it rather than as separate work.

---

## 13. The strong spike, and what it found

§9 condition 3 was "the printed source still parses", and §9.1 admitted that is the weak form: it
says the output is Lua, not that it is the *same* Lua. The sorting sabotage passed every one of
`lua_tree`'s tests while destroying child order, which named the missing judge — the only thing
that can say whether a program means what it meant is the host that runs it.

`app_host/src/tests/round_trip.rs` is that test. It runs the real kanban twice, once from the
source that ships and once from that source printed through `lua_tree`, and compares the three
things an app actually produces:

| output | what it catches |
|---|---|
| the node tree `view()` returns | reordering, dropped children, changed operators |
| the `El` tree `walk` builds | props the host will not accept |
| the doc `model.lua` seeds | anything the printer leaks into data |

### 13.1 Verified by breaking it

Three sabotages in the printer, each chosen to look like formatting:

| sabotage | `lua_tree` (11 tests) | `round_trip` |
|---|---|---|
| `and` printed as `or` | **all green** | 2 fail |
| `a.b` printed as `a[b]` | **all green** | 4 fail |
| children stably sorted by printed text | 10 green, 1 fails | 1 fails |

The first two are the point. Each produces output that parses, is idempotent, keeps every comment
and has no opaque node — and means something else. Text-level tests cannot see any of it.

The third is the original sabotage, and its `lua_tree` catch is *incidental*: sorting changes the
depth-first visit order, so `ids_survive_a_reprint` notices the ids come back in a different
sequence. That test only exists because ids are in the text. Under §12 level 3 it would not exist,
and the sort would go through clean. The catch that survives the redesign is the tree comparison.

### 13.2 The finding: `_nid` in text does not run

> The proposed answer is `nid-channel.md`: the id is stamped by the **tagger**, not by the printer,
> because the tagger is the one place in the system that knows a table is an element.

The first run failed somewhere I had not predicted, and the error is worth quoting:

```
doc.list takes positional entries only, got the key `_nid`
  [string "model.lua"]:18
```

§4 assumed an id could ride into the VM as an ordinary field. It cannot. The printer stamps `_nid`
into **every table constructor**, and only some table constructors are elements — `model.lua` seeds
the board through `doc.list({ … })`, which rejects named keys on purpose so that a stray field
cannot vanish into a list silently. That guard is right. `_nid` violates it.

So the app does not render wrong. It **fails to load**, at module scope, before `view()` is reached
and well before `props::apply` gets its own chance to reject the key on an element.

The narrow reading is that two host functions need teaching, plus `STRUCTURAL`, plus a filter so
`doc.map` does not write `_nid` into the CRDT and ship it to every peer. The wider reading is the
one that matters:

> **Stamping ids into source is a claim about every table in the file, and the design only ever
> meant to make a claim about elements.** Syntax cannot tell the two apart; only the runtime can.

That is §12's Level 2 failing in the specific way §12 predicted it would — and it is an argument
for Level 3 that does not depend on concurrency at all. Under Level 3 identity lives on the Loro
node, the text carries none of it, and the question never arises.

`lua_tree::print_bare` is that text: the same printer with the stamping off. It is what the
meaning tests compare, and what an app actually runs today. `print` keeps the ids, and
`stamping_ids_into_source_breaks_the_data_path` pins the defect as a live assertion rather than a
note here — so it fails on the day the question is answered, whichever way it is answered.

### 13.3 What it still does not judge

Handlers render as `fn`. Swapping two `on_click` bodies would pass. The comparison is over
structure, and behaviour is the next weakest link.
