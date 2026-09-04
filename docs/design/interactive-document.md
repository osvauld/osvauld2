# The interactive document

> Status: **dream + first distillation**. Nothing here is scheduled. Written 2026-09-04 from
> four research passes: two over this repo's own prior art (`doc_editor`/`block_doc`/`rich_text`/
> `text_edit`, and `code_editor`/`code_highlight`/`app_engine`/`sthalam`), two over the outside
> world (interactive/programmable documents; rich text in a CRDT + parley + highlighting).

---

## 0. The question this answers

> *"we want user to edit things visually, could they add lua directly or we can treat something
> as a block that is programmable"*

The answer is **neither, and the second half of the question is the important half.**

Not two kinds of block — one. Its ground truth is source; its default face is a view; and the
thing that decides whether the design lives or dies is whether moving between those faces is a
**toggle** or a **conversion**.

Livebook shipped the "visual surface with code behind it" idea properly — smart cells, a form
whose `to_source/1` generates the Elixir that actually runs, always inspectable, an explicit
anti-magic stance. It works. Its named failure is the **one-way door**: "convert to code cell"
drops the UI permanently, so every user's first non-standard requirement ejects them into raw
code forever and a visual document decays into a code document one block at a time.

mage (UIST 2020, Apple/CMU) found what people actually want, studying nine practitioners: not GUI
*or* code but **fluid movement between them inside a single task, chosen per moment**. The
historic failure of every GUI-in-notebook tool was that reaching for the GUI meant abandoning the
code path.

So: **a view is a recognizer over the source, never a generator of it.** It pattern-matches the
AST, edits the shapes it knows, degrades to read-only on the ones it doesn't, and re-engages when
the code returns to a shape it understands. There is no door because there is nothing to convert.

---

## 1. The dream

### 1.1 A morning with it

You open a document. It's a document — prose, headings, a to-do list. You type `## ` and get a
heading, `- ` and get a bullet. Nothing surprising, and that's the point: **the ordinary case
must be completely ordinary.**

You select a phrase and hit ⌘B. Then you keep typing at the end of it and the new text is bold,
because that is what every editor on earth does. You pick a colour from a swatch; the word turns
red. Someone else has the document open and their caret is visible three lines down; their edits
land while you type and your caret doesn't move.

You paste a table of numbers. It becomes a real typed table — its own document, referenced, not
embedded. You type `/chart`. A block appears with a live bar chart of it. Behind that block is
about eight lines of Luau, and there's a small disclosure arrow on the left of the block. You
click it and the source is right there, in the document, as ordinary code blocks with syntax
highlighting, editable. You change `type = "bar"` to `type = "line"` and the chart above it
changes as you type. You collapse it back and the chart is just a chart again.

You drag the chart's y-axis to a different column. The **source rewrites itself** — `y = "cost"`
becomes `y = "margin"` — because the chart view declared that argument as an editable shape. You
then hand-edit the source into something the view has never seen: a two-series overlay with a
custom scale. The view doesn't break and doesn't vanish. It says, in the corner, *"read-only —
I don't recognise this shape"*, and renders anyway. When you undo back to something it knows, the
drag handles come back.

You write a sentence: "at current burn we have **{= runway()} months** left." That's not a code
block. It's an inline computed value, blue-inked so a reader can tell it isn't something you
typed, and it updates when the numbers do.

You ask the agent to add a cohort breakdown. It writes a new block into the document while you
are typing in the paragraph above it. Your caret doesn't move. You watch the block appear.

You're not sure about the pricing assumption, so you **fork the document**. Both versions sit
side by side, the diff covers both the prose and the outputs, and you keep one.

At the end you press export and get a PDF. Every live block has a static projection, so the paper
version is a real document and not a page of holes.

### 1.2 The three claims

**A document, a source file and an app are the same object at different densities.**
`main.lua` returning a `view()` is a document with one enormous programmable block. A memo is a
document with zero. Everything between is the interesting space, and there is no format
boundary to cross to move along it.

**The substrate is the dependency graph.** Observable and marimo each had to *invent* one — parse
the code, extract free variables, topologically sort, re-run the dirty cells. That machinery is
where their complexity and their debugging opacity live. We don't need it: blocks read the doc
through a mediated mirror, so the reads *are* the edges.

**The agent writes blocks; the human edits views.** This is the direction already chosen
(`agent-native-direction`), and the research supports the split. Livebook's smart cells were
hand-written form UIs because generating them was hard in 2022. That constraint is gone, and with
it the plugin-marketplace ceiling that caps Livebook at whatever its maintainers curate.

---

## 2. Why this is possible here and not elsewhere

Five properties this stack already has that the systems in the survey had to fake, buy, or do
without:

**One storage substrate for prose, code and data.** `block_doc` is a `LoroTree` of blocks, each a
`LoroMap` of metadata around a `LoroText` of content. `code_editor` stores *Lua source* in the
identical structure — a tree-sitter splitter cuts a file at top-level construct boundaries into
`Function | Statement | Comment` blocks, with `emit(&split(src)) == src` asserted byte-exact for
every input including invalid Lua. So a `.lua` file is literally a block document. Prose and code
are the same shape, and the only difference is the kind vocabulary.

**A block-granular agent seam that already worked.** The payoff of that splitter, in the old
shell, was MCP merging an edit into **one function block** of a running app while the human's
caret sat untouched in another. Every other system in the survey treats "the agent edits the
document" as a whole-file replace.

**Real collaboration, already correct.** Loro marks are Peritext-compliant: paired style anchors
riding the same Fugue sequence as the text, resolved LWW by `(lamport, peer)`. Concurrent
overlapping bolds merge to the union — the case Markdown delimiters and Yjs's unpaired control
characters both get wrong. Two people colouring overlapping ranges get *three* runs, resolved
per-character, which no "one attribute per range" model can express.

**Retained islands with an immediate description around them.** The framework bet
(`framework-differentiation`) is exactly a document's shape: prose is cheap to re-describe every
frame; the editing surface and live blocks are retained. The distinction we already need for
performance is the same one the document needs for semantics.

**Branch, diff and merge as a document primitive.** Patchwork's claim is that these are universal
tools every creative surface should have, not programmer tools. With Loro they are nearly free —
and nobody has applied them to a *programmable* document, where "what if this computed
differently" becomes a fork with a diff over both source and output. This is the highest-leverage
thing in the whole brief that is uniquely available to us.

---

## 3. The one block: data and behaviour

A block is **data**, plus optionally **behaviour**. Both live in the CRDT. Both are editable — by
a human, a peer, or the agent. Neither is privileged.

```
Block = {
  kind,
  data:      LoroText | LoroMap   -- what it IS.   Prose + marks, or typed values.
  behaviour: block subtree | nil  -- what it DOES. Luau, itself split into blocks (§3.6).
  props:     LoroMap              -- declared editable slots (§3.2)
  children:  subtree
}
```

This costs **nothing in storage**. `block_doc` blocks already carry a `LoroMap` of metadata around
a `LoroText` of content: `content` is the data, and behaviour is one more `LoroText` in the map.

The two ends of the spectrum turn out to be the same object:

- a **paragraph** is a block with data and no behaviour;
- a **function** — one of the splitter's `Function` blocks in a `.lua` file — is a block with
  behaviour and no data;
- everything interesting is in between.

### 3.1 Why the split matters

The earlier draft of this document made a live block's *content* be its Luau source, which forced
every visual edit to be **AST surgery**: dragging a chart's axis meant rewriting `y = "cost"` in
the source text. That works — mage proves it's tractable for a bounded set of shapes — but it
makes the common case as expensive as the rare one.

Separating them collapses that:

| | data edit | behaviour edit |
|---|---|---|
| example | change a colour, retitle an axis, tick a box | change how the value is computed |
| mechanism | an ordinary CRDT write | a text edit that invalidates a compiled chunk |
| cost | free — no recompile, no re-analysis | recompile, re-derive dependencies |
| safety | **always safe** | sandboxed, budgeted, effects declared |
| frequency | constant | rare |

The last two rows are the argument. A right-click colour change must never be a security question
or a recompile, and with the split it structurally cannot be. **AST recognition is then reserved
for what it's actually good at** — editing behaviour visually — instead of being the only
mechanism available.

### 3.2 Declared property slots, and the context menu

A block declares its editable slots:

```lua
props = {
  fill  = { type = "color",  default = C.card },
  title = { type = "string" },
  y     = { type = "column", of = "budget" },
}
```

One declaration, **three consumers**:

- the **context menu** is generated from it — right-click a block and you get exactly the slots it
  declares, with the right editor per type: a swatch for a colour, a picker for a column;
- the **view** reads the same slots to render;
- the **agent** reads them to know what it can change without touching code.

This kills two traps at once. The *avocado slicer* (#15): no bespoke menu per block kind, so a
user-authored block gets a context menu the moment it declares props. And *schema drift* (#9):
Embark's map view implicitly hunted for a `location` property with nothing enforcing or
documenting that contract, which they named as one of their main challenges. Declared slots make
the mismatch a nameable error instead of an empty render.

### 3.3 The rung between data and behaviour

**A property slot accepts a literal or an expression.** This is what the ladder needs and what the
data/behaviour split lacks on its own — without it there are two rungs (pick a value / write a
program) with a cliff between them.

```lua
fill = "#2b6a3f"                                     -- a literal
fill = expr [[ if row.overdue then C.danger else C.card end ]]   -- same slot, computed
```

Inkbase built exactly this: any property holds a literal *or* a reactive expression, edited in the
same place by the same gesture. Their stated reason for choosing a Lisp was that "everything in
Lisp is an expression… which composes nicely with the idea of reactive properties." Luau
expressions serve the same role.

So the slope is **right-click and pick a value → bind the slot to an expression → open the
behaviour and write a script.** Three rungs where there were two, and the middle one is where most
real tailoring will land.

### 3.4 Behaviour syncs — so an import must never fire an effect

Behaviour belongs in the CRDT: it's something a human typed, so it merges, diffs, undoes, and the
agent's block-granular edits work on it exactly as they work on prose.

The consequence has to be said out loud. **A peer can change what your document does.** That's
ordinary — Notion and Coda have the same property — but combined with §4.3 it becomes a rule:

> An imported behaviour change never executes an effect. Effects run once, on the actor who
> triggered them.

A pure block re-deriving on import is just a repaint. An *effectful* block re-running because a
peer edited its source is a document that mails someone when a colleague fixes a typo. The effect
declaration and the capability grants are what stand between those two, and this is the case that
proves they're load-bearing rather than ceremony.

### 3.5 Views are still Luau functions living in the document

Unchanged, and now cheaper: a view is `(data, props, value) -> scene`, and it only falls back to
AST recognition when the *behaviour itself* is being edited visually.

Glamorous Toolkit measured 131 custom views across 84 objects at **~9.2 lines of code per view**.
When a bespoke visual surface costs ten lines, people write hundreds of them and the visual/code
dichotomy dissolves. If a view is a block, users author block types without leaving the document,
agents author block types, a document carries its own bespoke UI, and Boxer's naive realism holds:
there is no privileged layer you can't open.

### 3.6 Behaviour is itself a block tree — and this was already tried

Behaviour is not a flat string. It is parsed and stored as blocks, the same way prose is, using
the splitter that already exists: `code_editor/src/lua.rs` cuts Lua at top-level construct
boundaries into `Function | Statement | Comment` blocks, and `store.rs` writes them into a
`block_doc::BlockDoc` — the same `LoroTree` + `LoroText` shape. A `.lua` file on disk is persisted
as **Loro snapshot bytes, not UTF-8**.

The invariant that makes it safe to do this at all, asserted for every input including invalid
Lua:

```
emit(&split(src)) == src        // byte-exact, always
```

Every byte lands in exactly one block; whitespace between constructs is peeled into `Comment`
blocks; a tree-sitter `ERROR` node classifies as `Statement` so a half-typed program still
round-trips. That is what lets the CRDT hold code at block granularity while the VM still gets an
exact source string to `load()`.

So there is **one representation for everything.** A document's blocks and a block's behaviour's
blocks are the same structure — recursion, not a special case. And the payoff is the seam that
already worked once: MCP merging an edit into *one function block* of a running app while the
human's caret sat untouched in another.

**Two lessons from the earlier attempt, both load-bearing:**

**Never re-split a live document.** `doc_from_bytes` loads a snapshot *directly* to preserve block
IDs, because `doc_from_source` re-splits and **mints fresh `TreeID`s, desyncing every anchor** —
agent references, MCP block edits, and carets all point at blocks that no longer exist. Splitting
is an *import-time* operation, once, at the boundary. After that the block tree is the truth and
the text is derived from it.

**Re-splitting on edit is unsolved.** `code_editor/src/edit.rs` deliberately never changes the
block count: cross-block edits no-op, Enter inserts a literal `"\n"` rather than splitting, and
blocks fully inside a deleted span are emptied but kept so identity survives. That was the right
call for the old scope and it leaves a real gap — **type a new function into a behaviour and the
block structure does not follow.** Options, none yet chosen: re-split on blur into a diff against
the existing tree (matching surviving blocks by content similarity to preserve ids); split only on
an explicit gesture; or accept a coarser granularity and let a block hold several constructs. This
is the piece to design before behaviour-as-blocks ships.

### 3.7 The block is also the unit of failure

A view tree ought to let one bad part fail while the rest draws, and half of that already works.
Walk-time errors — a bad node shape, an unknown tag, a wrong prop type — are already localised,
because by then the tree exists and Rust is reading it:

```rust
fn fail<M>(context: &mut Ctx<M>, msg: String) -> El<M> {
    let msg = format!("{} > {msg}", context.path);   // breadcrumb to the offending node
    context.errors.push(msg.clone());
    err_box(&msg)                                    // stands in place of the bad node
}
```

**A Lua throw is the case that cannot be localised this way.** If the app's `view()` throws while
building, there is no partial tree — the call unwound and nothing came back. `view()` falling back
to one screen-sized `text("View error: …")` is not a poor implementation; it is genuinely all the
information that exists.

The fix is not in the renderer. It is to stop having one `view()`: give each block its **own**
builder, called separately and `pcall`ed individually. Then a throwing block gets a box in its own
slot and every other block still renders, because every other block was a separate call that
succeeded.

So **per-block error isolation is a consequence of the block model rather than a feature added to
it** — and it is the strongest practical argument for the model, over and above §3.1's cost
asymmetry. `app_engine` had this: every Lua entry point returned a `Result` and errors rendered
inline in the cell rather than crashing the app.

It also amends §4.7's reload policy, which is currently too strict. The failure unit becomes the
block, not the app:

| what failed | reload |
|---|---|
| the root — no tree at all | **reject**, keep the old VM |
| one block's behaviour | **accept**, and render that block as an error box |

Rejecting a reload that fixed four blocks and broke a fifth leaves the user unable to see what they
did. And it gives the runaway-loop case a decent answer too: `fires` catches a block that never
returns, and with per-block calls that is one box rather than a dead screen.

### 3.8 `Frame` is what a block's behaviour produces

`visual-substrate.md` already defines the output type, and it is exactly the shape this needs:

```rust
Frame { w: f32, h: f32, baseline: f32, items: Vec<(f32, f32, Item)> }
```

Four consequences, and together they close the largest hole in this document.

**It is the intrinsic-sizing contract.** §4.7 and open question 1 flag that nothing in the runtime
measures to fit a width. `Frame` is the answer: a producer is asked for a width and returns
`w`/`h`/`baseline`, which is precisely what a block in a flowing document has to report for block
offsets, scroll extent and PDF pagination to be right. *"Given child frames with w/h/baseline"* is
that doc's stated layout rule at every depth.

**Block-level and inline are the same production.** A chart on its own line and `{= runway() }`
in the middle of a sentence both produce a `Frame`; the difference is only which pass consumes it —
the block flow, or the inline pass. That collapses open question 3 entirely: an inline computed
value is not a mark, not a zero-width block and not a special case, it is a `Frame` with a baseline.

**It is pure, so blocks are testable headlessly.** `str → Frame` is specified as *"no scene, no
GPU, no parley, no window."* A block's output can be asserted in a unit test without a window ever
opening.

**It is what `runtime::el::custom` should have been.** `custom` takes a closure that paints itself
given a rect. A closure cannot report its size, cannot be cached, cannot be diffed, and cannot be
tested without a `Scene`. `Frame` is the same idea as *data*, which is why it gets all four.

Two things this forces:

- **`err_box` needs a `Frame` form.** §3.7's isolation works for block-level failures because the
  box owns a line. An *inline* computed that throws must fail as an atom with a sensible width and
  baseline, or one bad value breaks the paragraph's line breaking. Error rendering has to exist at
  both placements or isolation is only half-true.
- **A Frame cache must key on the source version, not just the data.** Frame caches are clause 2
  of §4.9 — reconstructible, so a reload drops them. But a cache that survived and keyed only on
  inputs would serve a Frame built by code that no longer exists. §4.2's content-addressed key
  already says `hash(source, input values, block id)`; this is the same rule, and the two must not
  drift apart.

**Where `El`/Taffy stops and `Frame` starts: a Frame is a Taffy leaf.**
`view-and-interaction.md` §9 already names the pattern for the hardest case — *"a graph/canvas
(positions come from a simulation: one taffy leaf, fixed size, place nodes inside)."* Generalised,
the leaf has **two modes**, and picking between them is the real decision:

| | who decides the size | example |
|---|---|---|
| **fixed leaf** | Taffy, outside-in — "you get 600×400" | canvas, a physics scene |
| **measured leaf** | the Frame, inside-out — "I am 240×90, baseline 62" | a chart, inline math, a live block, **a wrapped paragraph** |

Taffy 0.12's `compute_layout_with_measure` hands its closure both `known_dimensions` and
`available_space`, so **one hook serves both**: dimensions known → canvas mode; only an available
width → measure mode, and the Frame reports `w`/`h`/`baseline` back. That is the same hook open
question 1 needs for text wrapping, which means intrinsic sizing is **one mechanism with five
consumers** — text, charts, math, canvas, live blocks — rather than five pieces of work.

Coordinates compose through a single transform. A `Frame` nested in an `El` inherits the El's
accumulated transform, and `Group { transform: Affine, … }` composes onto that. There is no second
coordinate system, which is exactly why `view-and-interaction.md`'s decisions log puts scale in
`emit`'s accumulator rather than paint-only: visuals stay equal to hits.

The case that stays genuinely awkward is **a paragraph with inline atoms, which is Frame-level
composition all the way down**, since inline layout is neither Taffy's job nor parley's.
Everything else is "Taffy outside, Frame inside."

**And a measured leaf is cheaper to add than it sounds, because the runtime already re-lays-out
every frame.** `runtime/src/lib.rs:206` calls `layout::solve(app.view(), …)` inside `frame()`, and
`solve` (`layout.rs:217`) does `TaffyTree::new()` followed by `compute_layout` — a full Lua view
rebuild and a fresh tree, unconditionally, every frame. So there is no incremental-layout machinery
to integrate with: the measure closure just runs during that frame's solve, and
`compute_layout_with_measure` is a drop-in for `compute_layout` in the same function.

Three consequences worth stating plainly, because they are easy to get backwards:

- **Animating a size costs nothing extra.** The relayout was already happening. What is expensive
  is **text shaping**, not Taffy — solving a few thousand flexbox nodes is cheap; re-shaping
  paragraphs through parley is not. The old `doc_editor` cached galleys on a fingerprint over
  (runs, style, width, lang), and its author flagged computing `runs` per frame as *"the cost to
  revisit (Loro diff) at scale."* A content cache keyed on (text, marks, width) is the mitigation,
  and it is orthogonal to animation.
- **"Reserve the space" is a visual rule, not a performance one.** The kanban renders its drop
  guides on every gap so nothing *shifts* when a target appears. Words jumping between lines
  mid-animation reads as broken however cheap the relayout was. The discipline stands — just not
  for a perf reason.
- **The real ceiling is node and shaping count**, which is the O(n)-per-frame problem the old
  editor hit: fine at 50 blocks, dead at 5000. Animation only means sitting at that ceiling
  continuously rather than occasionally.

### 3.9 Structural editing: what the agent actually calls

The splitter as built is deliberately **shallow** — it cuts at top-level construct boundaries and
function *bodies* stay flat text in one block. So today an agent can replace a whole function and
nothing finer. To insert a statement into a function, add a field to a table, or change one
argument, it is back to string matching and line numbers, which is where agent edits go wrong.

What's wanted is the full parse, addressed structurally. The design question is **how deep the
CRDT itself goes**, and there are three answers:

| | CRDT stores | agent edits | human edits | invalid states |
|---|---|---|---|---|
| **A** — today | top-level blocks of text | whole constructs | free text | fine |
| **B** — full AST as CRDT | every node | surgical, merges structurally | constrained: typing must restructure | **no tree exists while you type** |
| **C** — text + derived tree | block text (as today) | surgical, applied as text ranges | free text | fine |

**C is the answer.** B is the seductive one and it breaks on the thing you do most: while you are
typing `if x the`, there is no valid AST, so a store that holds only AST nodes has nothing to
hold. It also throws away `emit(&split(src)) == src`, and it replaces Loro's well-understood text
merge with a structural merge whose semantics you'd have to invent and explain.

C keeps text as ground truth and makes the *addressing* structural:

> The agent names a node. The server parses, resolves the node to a byte range, and applies an
> ordinary Loro text edit. **Structural request, textual application.**

That's enough to get everything the ask wants, and it costs no new merge theory.

**The API shape**, in place of "write this file":

```
outline(block)                  -- the tree: kinds, names, node paths, ranges
read(node_path)                 -- source of one node
insert_before/after(node_path, src)
replace(node_path, src)
delete(node_path)
set_field(node_path, field, src)   -- an argument, a table entry, a condition
```

**The safety gate is the parser, and it is the point of "perfect parsing."** Every edit is applied
to a scratch copy, re-parsed, and rejected if it introduces an `ERROR` node that wasn't already
there — so a malformed agent edit never reaches the document. Then re-resolve the node path and
confirm the node that changed is the node it named. This is W4's types-gate idea moved one step
earlier: validate *before* the write, not before the hot-swap.

Two prerequisites, both concrete:

**The grammar must be Luau, not Lua.** `code_editor/src/lua.rs` parses with a plain Lua grammar.
Luau's type annotations, string interpolation, compound assignment and `continue` are not Lua, so
that grammar produces `ERROR` nodes on perfectly valid source — which silently defeats the safety
gate above, since "did this edit introduce an error" becomes unanswerable. Either adopt a Luau
tree-sitter grammar or extend the Lua one; **verify what exists before assuming.**

**Node paths must survive an edit.** A path like `function:update > body > stmt:3` is resolved
against a parse that the next edit invalidates. Either the agent re-reads `outline` between edits
(simple, chatty), or paths are anchored to Loro cursors at the node's start and end so they
survive concurrent human typing (the same trick §5.7 uses for the caret, and the better answer).

This is what makes the deferred MCP work (W3 §5–§7) worth doing properly rather than as a
file-write shim — and the old bridge already proved the hard half, merging a per-block edit into a
running app while the human kept typing.

---

## 4. The decisions

### 4.1 Dependencies: mediated reads, at path granularity

Observable and marimo both derive the graph from **static AST analysis**, and marimo's defence of
that choice is correct on its own terms: runtime tracing is *more complete* and therefore *worse*,
because it produces a model users cannot predict — "an uncanny valley with steep usability
cliffs." Its knowable incompleteness (it doesn't see `list.append`) is a teachable rule.

We can do better than both, because our reads are not arbitrary. A block reads the doc through
the mirror: `doc.board.cards`, `doc.budget.rows`. So **record the paths a block actually read
during its run, and invalidate on exactly those paths.** That is:

- **precise** — no false edges from a name that happens to appear in a comment;
- **parser-free** — no Luau AST walk, works with any control flow;
- **inspectable** — "this block read `budget.rows` and `assumptions.burn`" is a sentence you can
  put in the UI, which is the antidote to reactive opacity;
- **not the tracing trap** — mutation isn't invisible, because mutation of the doc goes through
  the same mediated interface and bumps the same version counter.

The incompleteness is honest and nameable: anything read from *outside* the doc — `now()`, a
network fetch, a Lua global — is not an edge. Those are exactly the blocks that must declare
themselves impure, and impure blocks don't auto-run (§4.3).

**Blocks do not share a Lua namespace.** No shared globals, no notebook-style top-level bindings.
Cross-block communication goes *through the doc*, which makes it persistent, collaborative,
inspectable and undoable. Hidden state is structurally impossible because there is no hidden
place to put it. This is marimo's "deleting a cell removes its definitions" guarantee, obtained
for free rather than enforced by a rule.

### 4.2 Outputs never enter the CRDT

Non-negotiable. Jupyter's canonical wound is a one-character source edit (`x**2` → `x**3`)
producing a **42,571-character diff**, because outputs live in the file; of 1.4M notebooks on
GitHub, **4.03%** reproduced their stored outputs.

In a CRDT it is strictly worse than in git. Outputs are derived data with a unique correct value.
Two peers run the same block, produce byte-different PNGs, and Loro **merges** them into a blob
neither produced. There is no conflict to resolve because neither value is wrong.

> **The CRDT holds only what a human typed, plus structure a human intended.**

Outputs are local, content-addressed by `hash(source, values at the read paths, block id)`.
That key is exactly what §4.1 already gives us, and several features fall out of it: instant
undo/redo of *results* and not just text; a peer who already computed a value can gossip the
cache entry so others skip the work; deterministic replay for audit; and "this output came from
*this exact* source" as free provenance.

The same rule covers syntax highlighting, layout caches and the dependency graph itself. All
derived. All local.

### 4.3 Execution: reactive, with cost and effects declared per block

Every system that chose imperative run-on-demand reproduced hidden state — Jupyter, Mathematica,
Livebook. Every system that chose reactive eliminated it structurally. So: reactive by default.

Nobody has solved the expensive node. marimo shipped a config toggle; Observable Framework moved
expensive work to **build time** — same document, two clocks; Coda carved out actions. The
synthesis: the escape valve is a **property of the block, not a global mode.**

```
meta.cost   = cheap | expensive     -- expensive: mark stale, don't auto-run
meta.effect = pure | effectful      -- effectful: explicit trigger only
```

Coda's formulas-vs-actions split is the most reusable idea in the low-code space, and
local-first makes it sharper: **an effect that fires on every peer when a CRDT change lands is a
bug factory.** So effects run **once, on the actor who triggered them**, and their results enter
the document as ordinary data.

### 4.4 Never fork the language

Observable spent seven years and a company on `viewof`, and **retracted the dialect entirely** in
Notebooks 2.0 (2025) — vanilla JS, HTML file format, open source, runs locally. Val Town
independently made and retracted the same mistake with its `@` mention syntax. Two teams, same
conclusion, and the cost isn't aesthetic: you lose every parser, formatter, linter and LSP that
knows your language — and the model that would otherwise write it for you, which for an
agent-native product is the whole game.

marimo demonstrates the entire benefit at zero syntactic cost: make the widget a **value**, and
an ordinary variable reference is the dependency edge.

```lua
-- no new keywords, ever
local n = ui.slider{ min = 1, max = 10 }   -- block A
```

### 4.5 Naming and reuse ship in v1

Spreadsheets are the most successful end-user programming system ever built and Panko's field
audits find **~94% of operational spreadsheets contain errors**. The cause is the same thing that
makes them succeed: no abstraction mechanism. No named function, no reuse, no test — so
everything is copy-paste, and copy-paste is how errors replicate. Notion, Coda and Potluck all
hit the same ceiling; Potluck hit it *at team scale in a research prototype* ("time-consuming to
understand and modify tools built by others within our team").

**We already shipped this by accident.** `require` landed this week: a document can `require` a
sibling file or another block, and because a `.lua` file *is* a block document, "extract this
into a named thing" is a structural move inside the same substrate rather than an export.

### 4.6 The ladder, made visible

HyperCard's user levels (Browse / Type / Paint / Author / Script) are named by Ink & Switch as the
canonical gentle slope. Copy them literally:

```
read → interact with a widget → right-click and set a property
     → bind that property to an expression → read the behaviour
     → edit the behaviour → author a view
```

Seven rungs, and §3.2–§3.3 are what put the middle three there. Each costs an increment of skill, each is reversible, and the current rung is visible.
The failure to avoid is **the cliff** — any point where the next increment of power requires
leaving the environment. Ours would be "to do X, edit a `.lua` file outside the document." The
splitter is what prevents it: the file is already in here.

### 4.7 The two loops

Writing behaviour goes to the doc, the doc signals that it changed, and the refresh comes from
that. Same shape as a data write — but *refresh* means something different on each side, and
that difference is the whole design.

**The data loop, which exists and is tested:**

```
write (local edit, or a peer/MCP import)
  → subscribe_root fires → version.fetch_add(1)
  → if triggered_by == Import: wake() → proxy.send_event(Msg::DocChanged)
  → user_event: app.update(msg); redraw()
  → view(): v > mirrored ⇒ patch_into(mirror, doc.get_deep_value()); mirrored = v
  → the Lua view() runs against a fresh mirror
  → flush(): v > saved ⇒ persist
```

**The code loop, which does not exist at all:**

```
write to the source doc
  → [no subscription on src]                              ← missing
  → [no wake]                                             ← missing
  → rebuild the VM: fresh Lua, reinstall crdt + modules,
    re-eval main.lua, take the new view_fn                ← missing
  → keep:  the data LoroDocs, the retained islands
  → drop:  the module cache (per-VM, so this is automatic)
```

`LuaApp` already holds `src: LoroDoc` — and the compiler says `field 'src' is never read`. That
dead field is exactly the hook. Today the only way new code enters is `LuaApp::open`, which is why
every Lua edit costs a fresh upload *and* a fresh item.

Three things this makes concrete:

**You cannot repatch code.** A mirror repatch fixes stale *data* in place. Changed code needs a new
`view_fn`, which needs a new chunk — and every closure the old chunk created captured the old VM's
globals and upvalues, so it needs a new VM. That's the cost asymmetry from §3.1, stated in
mechanism rather than in a table.

**The module cache dies with the VM, by design.** `modules.rs` caches per-VM precisely so that a
reload — a fresh VM against the same docs — is what invalidates it. A `require`d file's edit
therefore takes effect on reload with no extra machinery, and there is deliberately no way for an
app to clear the cache itself.

**The two loops share everything but the check.** An earlier draft of this section claimed the
opposite twice — that the source needed its own message, and that its subscription "must not gate
on `Import`" because an in-app code edit is a local write that still needs a rebuild. Both were
wrong, and the thing that makes them wrong is *where the check runs*.

`App::view` takes `&self`; `reload` needs `&mut self`. So the staleness check cannot live in the
render path at all — it goes in `update`, which is `&mut self`. And once it is there, **every
writer already reaches it**:

| writer | how the check runs |
|---|---|
| a code block edited in-app | the message that carried the edit — the check runs on its way out |
| the MCP bridge, a peer | `Import` → `wake()` → `Msg::DocChanged` → `update` |

So `DocChanged` needs no payload and no second message: it exists to *produce a frame*, and the
check rides the frame. And the `Import` gate is the same one the data docs use, for the same
reason — it decides whether to schedule an **extra** frame, never whether an edit counts. The
version bump sits above the gate; only the `wake()` sits inside it.

**Debounce is deferrable, and was deferred.** A Loro commit is already a batching point, and
today's source writes arrive from MCP or an upload — both coarse. It becomes necessary the day an
in-app code editor writes per keystroke, and not before.

What the trigger actually needs turns out to be two watermarks and an error, all in
`reload_if_stale`:

- `src_seen` is read **before** the build, so an edit landing mid-rebuild stays unseen rather than
  being marked current for source the VM never read.
- it advances **even when the reload fails**, so a source that does not compile is retried once per
  *edit* rather than once per mouse move — and the next edit is the fix.
- the failure is **recorded and rendered as a banner above the still-running app**. A failed reload
  that says nothing is the worst outcome available: you change a file, the old code keeps running,
  and nothing connects the two.

### 4.8 Stateful reload: what actually survives a VM rebuild

This is where the code loop stops being plumbing. Four categories of state, and only two of them
survive on their own:

| state | lives in | survives a rebuild? |
|---|---|---|
| the data | `LoroDoc` | **yes** — a separate object; the mirror is rebuilt, the doc is not |
| scroll, drag gesture, text fields, transitions | runtime `Store`, keyed by `Id` | **yes** — outside the VM |
| `ui.state(id, init)` | `_state`, a **Lua global in the VM** | **no** |
| module-scope locals (`S.drag`, `S.col_modal`) | Lua upvalues **in the VM** | **no** |

**`ui.state` is a retained island living in the wrong place.** Look at what it already is: a flat
`id → table` map of plain data, string-keyed, with a `_live`/`_sweep` mark-and-drop lifecycle that
is the same idea as the `Store`'s liveness pass. It is the *same category of thing* as a scroll
offset and it is only in the VM by accident. Two ways out — snapshot `_state` before the rebuild
and restore after (smallest change, Lua API untouched), or move it into the runtime `Store`
outright (correct, and then it never needed rescuing). The second is right.

**Module-scope locals don't survive, and mostly shouldn't.** A drag in flight when a file is saved
is not worth preserving. But the kanban shows exactly where that rule bites: the new-column modal
keeps its *open-ness* in `S.col_modal` (a module local, dies) and its *typed text* in
`ui.state("col_modal", { name = "" })` (survivable). One modal, two storage classes, two different
survival semantics — and the split is invisible in the source. The guidance that falls out:
**anything a user would be annoyed to lose belongs in `ui.state`, never in a module local.**

**And the setup chunk re-runs.** Every reload re-executes `main.lua`'s top level — `doc:open`, the
seeding, any module-scope side effect. The old `app_engine` hit this and its fix is recorded in
its own source: load the CRDT snapshot *before* the script runs so seeds don't duplicate. The
kanban's seeding is guarded (`if #columns == 0`), which is the pattern — but reload turns an
unguarded module-scope write into a bug that multiplies once per save.

The hinge under all of it is **id stability**. Retained islands, `ui.state`, and the caret all key
off ids; a reload that silently renames one scrolls the user to the top, drops their draft, and
looks like data loss. That is the thing to test first, and it is exactly the lesson §3.6 already
records from the code-as-blocks attempt — `doc_from_source` minting fresh `TreeID`s desynced every
anchor. Same failure, different layer.

### 4.9 The retained-state survival contract

`view-and-interaction.md` Bundle D already has this as a checklist item, and states the important
part: *"physics worlds, Frame caches, and coroutines are one problem (§8.5), not three."* It is
now five, because this document adds `ui.state` and the document editor. So the contract is worth
writing once, as a rule the runtime enforces rather than a list each consumer reimplements.

**Four clauses, ordered by what they cost:**

1. **Never in the VM → untouched.** It doesn't survive the reload; the reload never reaches it.
2. **Reconstructible from the doc → rebuilt.** Cheaper to re-derive than to carry.
3. **Plain data → carried.** Has to be copied out and back, because nothing else backs it.
4. **Holds VM identity → dies with the VM.** Cannot be carried by any mechanism.

Every consumer lands in exactly one:

| state | clause | why |
|---|---|---|
| document content, app data | 1 | `LoroDoc`, a separate object |
| scroll, drag, text fields, transitions | 1 | runtime `Store`, keyed by `Id` |
| caret, selection, undo, IME, focus | 1 | the editor is a Rust retained island (§4.8) |
| doc mirrors | 2 | `mirrored: 0` and the first `view()` repatches from the doc |
| Frame caches | 2 | a cache by definition — re-derive rather than carry |
| `ui.state` plain entries — drafts, toggles | 3 | nothing backs them |
| coroutines (`ui.run`), anything holding a function | 4 | a `thread` holds a live stack of old-chunk closures |
| module-scope locals (`S.drag`) | 4 | upvalues; the host has no name for them |
| physics worlds | 3 or 4 | **undecided** — depends whether the world is Lua tables or a Rust object |

**Clause 4 is correct, not a compromise.** `view-and-interaction.md` §8.4 already gives the reason
in its own terms — *"the animation dies with the thing it was animating"* — and it generalises: a
suspended stack from code that no longer exists is precisely the hidden state we refused in §4.1.
Carrying it would be Jupyter's disease wearing a nicer hat.

**The failure mode to design against is silence.** Put a function in `ui.state` today and a reload
gives a confusing error, or worse, quietly does nothing. The partition has to be *visible*: the
snapshot pass names what it dropped and why, so "my animation restarted" has an answer at the
point it happens rather than in a doc nobody reads.

Two consequences for the code:

- The `ui.state` snapshot is a **filter, not a copy** — it walks `_state` and carries only what is
  plain, reporting the rest. That is a different function from "serialize `_state`", and getting
  it wrong is how clause 4 becomes silent.
- **Physics worlds need the Lua-or-Rust decision made** before they're built, because it picks
  their clause. Rust-side puts them in clause 1 for free, alongside the editor and the `Store` —
  which is an argument for building them there.

---

## 5. Rich text, concretely

The old editor's marks were `bold | italic | strike | code | link`, all registered
`ExpandType::None`, and colour was a per-render field the highlighter injected, deliberately never
persisted. Both of those change.

### 5.1 The model: ProseMirror's, exactly

Flat inline runs with marks as key→value. **Marks stay strictly inline.** ProseMirror's own
regret is block-level marks — added under pressure, with no coherent answer to whether an empty
paragraph carries the mark or whether parent and child both do. Heading level, list kind,
alignment, code language, paragraph direction are **block attributes**, which is already how
`block_doc` stores them.

Nested inline elements for formatting are the model to avoid. Slate removed marks-as-a-concept in
0.50 and moved *toward* flat runs with properties; Lexical's regret is `style: "color:#f00"`, an
untyped CSS blob that merges as an opaque LWW string so two concurrent colour changes to
different halves of a run can't merge at all.

### 5.2 Expand policy — the fix for "you can't turn bold on and type"

Peritext's insight is that expansion isn't a flag, it's *which of the two anchor slots around a
character you attach to*. Loro exposes it as `ExpandType`:

| | type at **start** | type at **end** |
|---|---|---|
| `Before` | inherits | — |
| `After` | — | **inherits** |
| `Both` | inherits | inherits |
| `None` | — | — |

```
After  →  bold, italic, underline, strike, colour, font size, font family
None   →  link, inline code, comment, highlight
```

`After` is Word/Docs/Notion behaviour: inherit forward, not backward. Loro's own defaults agree
(`bold`/`italic`/`underline` = `After`; `link`/`highlight`/`comment`/`code` = `None`). Inline
code is `None` for a concrete reason — if it expanded forward there'd be no keyboard way out of
the span. `Before` and `Both` are almost never wanted, and were broken until Loro 1.13.3 anyway.

The old code's blanket `None` is *correct for links and code and wrong for everything else*, and
it is why you must select text before you can bold it. The other half of the fix is
**`stored_marks` in view state**: `mark()` requires `start < end`, so a zero-width mark is an
`ArgErr`, not a no-op. Pending formatting at a collapsed caret cannot live in the CRDT and must
not try to.

### 5.3 Colour

Ordinary marks, with three sharp edges.

**Foreground and background expand differently.** Peritext classes text colour with the growing
marks → `After`. Background/highlight is a region annotation, not a character property — typing
past the end of a highlight and having it follow reads as a bug → `None`. *Text colour follows
the pen; background follows the region.*

**Concurrent conflicting values are LWW and there is no better answer.** Peritext says it
outright: the word must be either red or blue, it can't be both. But *partial* overlap resolves
per-character — Alice reds 0..10, Bob blues 5..15, and you get three runs with only the middle
contested. That's almost always right, and it's what a one-attribute-per-range model cannot give.

**Canonicalize the encoding.** Loro's redundant-mark skip only fires on exact value equality, so
`#ff0000` over a range already `#FF0000` records a new op and a new immortal anchor pair. Use
packed RGBA in an `I64`, or strictly-lowercase `#rrggbbaa`. Never `Null` — that's `unmark`.

### 5.4 Loro discipline

- **Register every style key up front.** An unregistered key is a hard `StyleConfigMissing`
  error, not a fallback. `config_text_style` **replaces** the map, so a fresh `StyleConfigMap`
  loses the built-in defaults. Style config is runtime state and is **not in the snapshot** —
  every construction path must re-register. (The old code got bitten by this.)
- **Diff before you mark.** Never blind-apply the editor's current mark state on each keystroke.
  Loro 1.15.0 fixed unbounded degradation when re-asserting identical marks — the no-op skip used
  a range lookup that returns `None` whenever the range spans more than one style leaf, i.e.
  always, on any already-styled document, so re-asserting recorded a fresh op every time and
  "every styled read walks all of them." **That fix is not in a published Rust release** (crate
  is 1.13.9; the fix shipped in JS 1.15.0 and on `main`).
- `unmark` is literally `mark(range, key, Null)`. There is no `unmark_utf8` — use
  `mark_utf8(range, key, LoroValue::Null)` to stay in byte offsets end to end.
- **Know [loro#1009](https://github.com/loro-dev/loro/issues/1009) before shipping formatting
  undo.** Open: a *concurrent* remote insert at the **start** of an `After` mark gets absorbed
  into it, and undo leaves residue on text the marker never touched. Workaround is
  `add_exclude_origin_prefix` on style commits plus an app-side format undo stack.
- Overlappable marks use a `key:suffix` convention (`comment:alice`, `comment:bob`) — one config
  entry governs all suffixes. Don't use it for colour; one `color` key, LWW.

### 5.5 parley

- **Use `StyleRunBuilder`, not `RangedBuilder`.** You push one resolved `TextStyle` per distinct
  run and get back a `u16` index — which is exactly the shape `to_delta()` already hands you.
  `RangedBuilder` makes you push one property per property per range (8 × 500 runs = 4000 pushes
  plus a resolve pass).
- **Keep a parallel style table indexed by `style_index`.** parley has 23 `StyleProperty`
  variants — family, size, weight (variable), style, width, brush, underline and strikethrough
  each with independent offset/size/*brush*, letter and word spacing, line height, locale, font
  features and variations. It has **no background colour**, no baseline shift, no sub/superscript.
  Paint backgrounds yourself per `GlyphRun` from `run.offset()`/`baseline()`/`advance()` and the
  line's block coords, looking up `run.style_index()` in your own table. That indirection is the
  clean seam for everything parley doesn't model: token class, spell squiggle, comment tint.
- `Cursor`/`Selection` are **byte-indexed** with `Affinity`, a sticky `h_pos` for up/down, and an
  `anchor_base` that makes double-click-drag extend by word and triple-click by line. Selection
  geometry emits one rect per line. AccessKit integration is behind a feature — take it.
- **Pin the version and read the changelog on every bump.** 0.11.1 is current; on `main` a
  `Cluster` becomes a full grapheme cluster instead of a character (changing whether
  `next_visual` moves by grapheme), `Glyph::style_index` is removed, and `LineMetrics::{ascent,
  descent, leading}` are gone in favour of the CSS line-box model. Any rect code written against
  ascent/descent is already stale. Complex-script breaking (CJK, Thai, Khmer, Lao, Myanmar) is
  behind the off-by-default `complex-scripts` feature.

### 5.6 Code highlighting

Derived state. **Never in the CRDT** — writing token colours as marks would create an anchor pair
per token per keystroke (§5.4) and sync megabytes of derived colour to every peer.

We're already in good shape: `code_highlight` is a pure function `(lang, &str) -> Vec<Span>` over
tree-sitter, 10 languages, a 15-variant semantic `HlKind`, **no colours** (the consumer maps kind
→ theme), and a documented invariant that the span list is a gap-free contiguous cover with
`Text` filling the gaps. Consumers just walk it. That is exactly the seam the research recommends
building, already built, with the better engine.

Two things to carry over: it is **not incremental**, and it doesn't need to be — the unit of work
is one block, cached by `(lang, content_hash)`, and re-highlighting a function on each keystroke
is free. If it ever isn't, `tree-house` (Helix's rewrite, which re-parses only changed injection
layers and tracks locals at parse time) is the upgrade; upstream `tree-sitter-highlight` is not
incremental and Helix threw their fork of it away.

### 5.7 IME, undo, selection

**Preedit lives in view state, not the CRDT.** parley's own `PlainEditor` holds
`compose: Option<Range<usize>>` and returns its text as a `SplitString` precisely because the
preedit is spliced into a buffer that is otherwise the document. Do the same: build the layout
from `doc[..off] + preedit + doc[off..]` so it wraps and hit-tests inline, and write **nothing**
to Loro until commit. A CJK composition is dozens of intermediate states per word; writing them
means remote peers watch you type garbage that then vanishes, every composition keystroke becomes
an undo step, and you have recreated the contenteditable desync bug without having a
contenteditable. During composition, **buffer incoming remote patches and apply them at commit** —
safer than re-anchoring the preedit offset under a live IME.

The old editor had **zero** IME handling. For any CJK or Indic user it was unusable. Design it in
from day one.

**Undo is local-only and is not a rewind.** It undoes this peer's operations transformed against
everything since; it never reverts a remote edit. The reason is concrete: if it did, you'd press
⌘Z, see nothing change in your viewport because the other person was editing elsewhere, press
again, and silently destroy their work. A true rewind is a *different feature* (`checkout` /
`revert_to`) that needs a different, clearly labelled affordance. Never bind it to ⌘Z.

Carry over from the old code: **never commit around undo/redo** (an extra commit clears the redo
stack), and **eagerly create nested containers** before the first text op — bundling a lazy
container creation with a text insert in one undo step breaks Loro's redo, which is the
"nested-container regression" the old tests pin. *Re-verify whether that's still true upstream;
the eager create is harmless either way.* Set `set_merge_interval` to ~500ms — it defaults to 0,
which makes every keystroke an undo step. Use `add_exclude_origin_prefix` so derived and
programmatic writes never enter the user's stack. Restore selection through `set_on_push` /
`set_on_pop`, storing **cursor IDs**, because offsets are already wrong by the time you pop.

**Selection is two Loro cursors, never offsets.** This is the best idea in the old codebase:
capture stable cursors at frame end, resolve them at frame start, so a remote insert before your
caret carries it along instead of stranding it. Keep the deliberate split it had — cursor-remap
for *remote* edits, clamp-only after *your own*, since your indices are already in current
coordinates and a pre-edit cursor would fight them. Anchor `Side::Left`, focus `Side::Right`, so
concurrent inserts at the selection's edges land outside it. Honour
`PosQueryResult::update` when Loro hands one back — it's telling you the anchor character is gone
and it has re-anchored. And **invalidate `stored_marks` when a remote edit lands at the caret**,
or the user's pending bold gets applied to text they never intended.

---

## 6. Traps, named

Each with the antidote we've chosen.

| # | Trap | Where it killed something | Our antidote |
|---|---|---|---|
| 1 | **The dialect** | Observable's `viewof`, retracted after 7 years; Val Town's `@` | plain Luau; widget-as-value (§4.4) |
| 2 | **Hidden state** | Jupyter: 4% of notebooks reproduce | blocks share no namespace; the doc is the only state (§4.1) |
| 3 | **Blobs in the CRDT** | ipynb's 42,571-char diff | outputs/highlighting/layout all derived and local (§4.2) |
| 4 | **The one-way door** | Livebook "convert to code cell" | views recognize, never generate (§3) |
| 5 | **Round-trip tax** | Enso, mage, Sketch-n-Sketch | enumerate editable shapes per view; degrade explicitly |
| 6 | **The parsing cliff** | Potluck: works on typed micro-syntax, collapses on pasted text | build on typed blocks a human created, never on inference over prose |
| 7 | **Shared-abstraction break** | Potluck: editing one search breaks every document using it | versioned + pinned; propagation is a reviewable act (branch/diff) |
| 8 | **Concrete-argument staleness** | Embark chose it deliberately for predictability | say which side you're on **in the UI** — the failure is silent either way |
| 9 | **Schema drift** | Embark's map view implicitly hunting for `location` | blocks declare typed outputs; views declare typed requirements; mismatch is a nameable error, not an empty render |
| 10 | **The abstraction ceiling** | spreadsheets, ~94% error rate | naming and reuse in v1 — `require` already landed (§4.5) |
| 11 | **Invisible code** | Inkbase: "most code hidden from canvas view" | a document-level "show me everything programmable here" |
| 12 | **Reactive opacity** | Observable, Mathematica `Dynamic` | **build the graph inspector before the graph**: per block, last run, inputs and their values, what it invalidated, why it re-ran |
| 13 | **Expensive-node fan-out** | unsolved by everyone | cost declared per block; expensive marks stale; content-addressed memo (§4.3) |
| 14 | **Over-atomization** | Val Town: "vals are too small" | the block is not the unit of *work*; `require` and the doc compose them |
| 15 | **The avocado slicer** | shipping a "chart block" and a "query block" | one general block kind, many views |
| 16 | **The cliff** | browser extensions | the splitter keeps the file inside the document |
| 17 | **Isolation** | HyperCard had no networking and died | we have sync — don't make live blocks the one thing that doesn't |
| 18 | **The reader can't tell** | general | Potluck's blue ink is the minimum bar: computed, stale and interactive must all be visible |

---

## 7. What already exists

| piece | where | state |
|---|---|---|
| Block tree in Loro (tree + map + text, stable `TreeID`) | `block_doc` | **portable as-is**, loro-only |
| Block schema, marks, `runs()`, `mark_covers` | `doc_editor/src/model.rs` | portable once one `Color32` field is dropped |
| Markdown input rules, Enter/Backspace/Tab semantics, paste's blank-line rule | `doc_editor/src/editor/input.rs`, `selection.rs` | algorithmic; port the logic |
| Loro-cursor caret anchoring | `doc_editor/src/editor/mod.rs` | the best idea here; port it |
| Eager-container undo fix + commit discipline | `block_doc/src/lib.rs` | port, re-verify upstream |
| Single-run caret/selection kernel + `TextBuffer` seam | `text_edit` | edit half portable; galley half rewritten |
| Lua splitter, byte-exact round-trip | `code_editor/src/lua.rs` | **portable**, tree-sitter only |
| Code-as-blocks codec (`.lua` → Loro snapshot) | `code_editor/src/store.rs` | portable |
| Syntax highlighting, gap-free spans, no colours | `code_highlight` | **portable**, right seam already |
| `ui.doc` — a document inside an app, over a named tree | `app_engine` | proves the embed seam works |
| Named-tree embed (`on_shared` / `new_on` / `on_tree`) | `block_doc` | the mechanism `ui.doc` used |
| Scene → PDF, one display list two backends | `doc_editor/src/pdf.rs`, `pdf_paint` | idea excellent; types are egui |
| Luau VM, sandbox, doc mirror, `require`, handlers | `app_host` | **live** |
| Version counter + repaint poke | `app_host/src/crdt.rs` | **live**, this is the reactivity |

And what has to be *built*, not ported:

- **`rich_text` in parley.** The whole crate is egui. The contract survives (open string-keyed
  marks → resolved style, theme injected, explicit run colour wins); no implementation does.
- **Text colour, background, underline, size, family as marks.** Never existed.
- **`stored_marks`** and the whole pending-format path.
- **IME.** Never existed.
- **Intrinsic height.** Every engine host in the old system got a fixed rect from a parent
  flexbox. A block in a flowing document must measure-to-fit-width and report its height or
  block offsets, scroll extent and PDF pagination all go wrong.
- **The composition itself** — a block whose content is a script and whose rendered form is its
  output. This never existed in either direction that matters.
- **Virtualization.** The old editor is O(n) per frame *and per keystroke*: `block_ids()` does a
  full DFS several times per keypress, `layout_all` shapes every block with no viewport window,
  `runs(id)` runs a Loro `to_delta` per block per frame. Fine at 50 blocks, dead at 5000. The
  author flagged it himself. `Diff::Text` gives the run-level diff to decide what's dirty, and
  Zed's pattern — re-layout on a worker from a snapshot, render the last good layout meanwhile —
  is the shape to copy.

Two things to delete rather than port: **index-based block insertion** (`create_block(index, …)`
takes a top-level sibling index while every other API speaks DFS order — the old tests name the
gotcha and work around it), and **plain-text-only clipboard** (internal copy-paste loses every
mark, kind and nesting level).

---

## 8. The distillation

If this becomes work, it is four steps and the first two are worth doing whether or not the rest
ever happens.

**① The rich text leaf.** `rich(runs)` over parley `StyleRunBuilder` with a parallel style table.
Bold, italic, underline, strike, **colour**, background, size, family. Correct expand policy.
`stored_marks`. This is already W8 in the plan, `text.rs` already wires a ranged builder, and its
consumers are the grid's cells, code blocks and labels regardless of whether a document editor
ever ships. **Nothing else here is possible without it.**

**② A read-only document viewer.** `block_doc` lifted as-is + the rich text leaf + a block loop +
per-block layout cache keyed by fingerprint. No caret, no marks UI, no IME. This is genuinely
small, and it gets a `.doc` on screen.

**③ The editing surface.** Caret and selection on Loro cursors, the markdown input rules, the
mark toggles, the slash palette and floating toolbar (as paint-only overlays — the old
egui-workaround architecture turned out to be a good design), block drag, and IME from day one.
This is the expensive step. It is most of `doc_editor` rewritten against parley.

**④ The live block.** Intrinsic height first, because everything else is blocked on it. Then one
block kind with three faces, path-granular dependencies, content-addressed outputs, and **the
graph inspector shipped in the same commit as the graph.** Start with two or three recognizers
(a slider literal, a chart spec table, a colour) to prove the toggle-not-conversion rule bites.

---

## 9. Open questions

1. ~~**Does `runtime`'s layout do intrinsic sizing?**~~ **Answered: no — and it blocks step ①,
   not step ④.** `runtime/src/layout.rs` measures a text leaf *unconstrained* —
   `text_engine.measure(&ts.text, ts.family, ts.size)` takes no width — and bakes the result into
   `style.size` as a fixed length before Taffy runs. `solve` then calls `tree.compute_layout`, not
   `compute_layout_with_measure`. So **text does not wrap to an available width anywhere in the
   runtime today.** A wrapped prose paragraph is currently impossible, never mind a live block
   reporting its height as a function of width.

   The fix is well-defined and both halves exist: Taffy 0.12 has `compute_layout_with_measure`,
   which hands a measure closure `known_dimensions` and `available_space`; parley's builders take
   a max advance for wrapping. Route text leaves — and later, live blocks — through that hook.
   **This is the first piece of engine work and it precedes everything in §8.**

   §3.8 sharpens this: it is not text-specific work. The same hook is the **measured leaf** that
   charts, math, canvas and live blocks all need, so it is one mechanism with five consumers. And
   because `solve` already builds a fresh `TaffyTree` and re-lays-out every frame, there is no
   incremental-layout machinery to integrate with — `compute_layout_with_measure` is a drop-in for
   `compute_layout` in the same function.

2. **Where does a live block's per-viewer state live?** `ui.doc`'s `"uidoc:{id}"` convention is
   the pattern, but a block's scratch state must survive re-ordering, copy-paste and duplication,
   and must *not* sync (it's per-viewer, like scroll and drag). Probably the retained-island
   store keyed by block `TreeID`, never the doc.

3. ~~**Inline computed values** — mark, `InlineBox`, or zero-width block?~~ **Answered in §3.8:**
   a `Frame`, same as a block-level one. What remains is narrower and still open — whether the
   *source* of an inline computed lives in the text (and is therefore visible when you arrow
   through it) or beside it. Note also that parley's `push_inline_box` may already do most of the
   inline pass `view-and-interaction.md` §9 assumes we must write; worth checking before building.

4. **Do we sync the fact that a block is *running*?** Presence-shaped, not document-shaped. Rides
   an awareness channel if it rides anything.

5. **Amb values.** A parameter that holds `{500, 1200}` and renders every downstream block once
   per scenario. "What are my three options" is the most common shape of real thinking and no
   document tool represents it. Cheap to describe, unclear how it interacts with §4.1's read
   tracking. Parked, but wanted.

6. ~~**Luau vs Lua for the splitter.**~~ **Promoted to a prerequisite — see §3.7.** It stops being
   a nicety once the parser is the safety gate for agent edits: with a Lua grammar, valid Luau
   produces `ERROR` nodes, so "did this edit break the source" has no reliable answer.

7. **Right-click inside a live block's rendered output.** Right-clicking the *block* is easy — it
   has declared props (§3.2). Right-clicking a **bar in a chart** means the rendered scene has to
   carry provenance back to the datum that produced it. That's the same machinery a view needs to
   be a structured editor, so it's one problem rather than two, but it's the harder half and it
   isn't designed.

8. **Does a `props` change belong in undo with the text?** A colour picked from a context menu and
   a sentence typed into a paragraph are both CRDT writes on the same block. One undo stack or
   two? Loro's `UndoManager` merge interval will happily fold a colour change into the keystroke
   burst before it, which is probably wrong.
