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

## 3. The one block

```
Block = {
  kind:    paragraph | h1..h3 | li | ol | todo | quote | code | divider | live
  content: LoroText          -- what a human typed. Marks live here.
  meta:    LoroMap           -- lang, done, view, cost, effects
  children: subtree
}
```

A **live** block adds nothing to the storage model. Its `content` is Luau source; its `meta.view`
names the recognizer that renders it. That's the entire delta.

Three faces, one object:

| face | what it is | who uses it |
|---|---|---|
| **view** | a Luau function `(ast, value) -> scene` that recognizes shapes in the source | the default; what a reader sees |
| **source** | the `LoroText`, as code blocks with highlighting | disclosure arrow; what the agent writes |
| **value** | derived, never stored | what downstream blocks and inline computeds read |

**The source is always ground truth.** A view is a renderer *plus* a structured editor for the
shapes it declares. Dragging an axis performs an AST-level edit of the source. Outside the
declared shapes the view is a read-only renderer with a visible "I don't recognise this" state.
Never a conversion. Never a door.

**Views are Luau functions living in the document.** This is the ambitious half and I think it's
right. Glamorous Toolkit measured 131 custom views across 84 objects at **~9.2 lines of code per
view** — when a bespoke visual surface costs ten lines, people write hundreds of them and the
visual/code dichotomy dissolves. If a view is a block, then users author block types without
leaving the document, agents author block types, a document carries its own bespoke UI, and
Boxer's naive realism holds: there is no privileged layer you can't open.

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
read → interact with a widget → edit through the view → read the source
     → edit the source → author a view
```

Six rungs. Each costs an increment of skill, each is reversible, and the current rung is visible.
The failure to avoid is **the cliff** — any point where the next increment of power requires
leaving the environment. Ours would be "to do X, edit a `.lua` file outside the document." The
splitter is what prevents it: the file is already in here.

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

2. **Where does a live block's per-viewer state live?** `ui.doc`'s `"uidoc:{id}"` convention is
   the pattern, but a block's scratch state must survive re-ordering, copy-paste and duplication,
   and must *not* sync (it's per-viewer, like scroll and drag). Probably the retained-island
   store keyed by block `TreeID`, never the doc.

3. **Inline computed values** — `{= runway() }` mid-sentence. Are they a mark with a value, an
   `InlineBox` in parley, or a zero-width block? parley's `InlineBox` is the mechanism; the
   document model question is whether the *source* of an inline computed lives in the text (and
   is therefore visible when you arrow through it) or beside it.

4. **Do we sync the fact that a block is *running*?** Presence-shaped, not document-shaped. Rides
   an awareness channel if it rides anything.

5. **Amb values.** A parameter that holds `{500, 1200}` and renders every downstream block once
   per scenario. "What are my three options" is the most common shape of real thinking and no
   document tool represents it. Cheap to describe, unclear how it interacts with §4.1's read
   tracking. Parked, but wanted.

6. **Luau vs Lua for the splitter.** `code_editor/src/lua.rs` uses a plain Lua tree-sitter
   grammar. Luau type annotations would need grammar and highlight-capture work.
