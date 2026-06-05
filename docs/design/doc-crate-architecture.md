# Architecture — the `doc_editor` crate (the `.doc` engine)

> Engineering architecture + decisions for Osvauld's `.doc` block editor. Companion to
> [`document-editor-brief.md`](./document-editor-brief.md) (which covers the *visual /
> interaction* design) — this doc covers the **compute / logical model**: data model,
> nesting vs subdocuments, sync/load boundaries, rendering & performance, and the build
> order. Decisions here are **locked** unless explicitly revisited.
>
> Status (2026-06-01): Phase A (Loro model) and the design's reading surface + slash menu
> are implemented. This doc records the architecture those rest on and the plan forward.

---

## 0. The one-line conclusion

Keep the **CRDT-native, immediate-mode** core. Bound **load** with subdocuments and
**render** with a Loro-diff-driven galley cache + paint-visible (chunked virtualization only
as a documented escape hatch). Borrow **ProseMirror's *shape*** for the authoring layers we
still owe (commands, selection, marks, decorations, node-views) while letting **Loro own**
merge, undo, and stable anchors.

---

## 1. The core idea

A `.doc` is one `loro::LoroDoc`. **The CRDT is the document and the single source of truth.**
The editor is a *borrow-only, immediate-mode* widget: every frame it reads the current CRDT
state, derives a throwaway view, turns input into CRDT mutations, and reports whether to
persist. There is no retained editor-state tree and no virtual DOM.

```
            ┌────────────────── the one source of truth ───────────────┐
   network ─►   LoroDoc   ◄─ editor mutations          export_snapshot ─► vault
  (import)  └──────────┬───────────────────────────────────────────────┘
                       │ read (per frame)
                       ▼
        DocEditor.show(ui, &Doc)  ──►  per-frame view  ──► paint
                       ▲
                       └── input (pointer/keys) ──► Doc mutations
```

This is closer to **Elm / immediate-mode** than to React/ProseMirror. The payoff: there is
no "two sources of truth" sync problem (the hard part of `y-prosemirror`-style bindings) —
input *is* CRDT ops, and the next frame re-derives everything.

### Inspired by ProseMirror, not adopting it

ProseMirror / Lexical / Tiptap are our reference for the **authoring layer** — *how they
structure* schema, selection, commands, marks, decorations, node-views. We borrow those
**shapes**. We do **not** adopt their **engine** (immutable doc + invertible Steps + OT
rebasing), because **Loro is our engine**: merge, undo, and stable anchors all come from the
CRDT. The clearest example is undo — ProseMirror builds it from inverted Steps; we get it
from `loro::UndoManager`.

---

## 2. The data model

```
LoroDoc
 └─ LoroTree "body"            block hierarchy; sibling order via fractional index
      └─ node = TreeID         ← stable block identity (survives reorder & remote edits)
           ├─ meta: LoroMap    { kind, indent*, done, lang }   (*indent is a stopgap — see §3)
           └─ content: LoroText   ← block text; inline marks live here (§ build-step 4)
```

Two choices keep the editor simple:
- **Identity is the `TreeID`, not a list index.** Caret, comments, and presence anchor to it,
  so a block moving or a remote insert above it never breaks references.
- **Text offsets are Unicode codepoints**, which is exactly what egui's `CCursor.index`
  counts — caret math maps 1:1, no conversion.

`Doc` (in `model.rs`) is UI-agnostic (depends only on `loro`) and exposes three faces — and
that tri-split *is* the modular seam:
- **read**: `block_ids`, `kind`, `text`, `text_len`, `indent`, `done`, `lang`
- **mutate**: `set_*`, `insert_text`, `delete_text`, `create_block`, `delete_block`
- **sync/persist**: `import(bytes)`, `export_snapshot()`, `from_snapshot()`, `commit()`

All mutators take `&self` (Loro is interior-mutable) — which is *why* one `Doc` can be the
shared merge point for editor + network + persistence without `&mut` juggling.

---

## 3. Nesting vs subdocuments — split on identity

These are **two different axes**, not the same feature. The decision rule is: *does this
thing need its own sync / permission / encryption lifecycle?*

### Block nesting (intra-document) → real subtrees in one `LoroDoc`
Toggles, list children, columns, callouts: hierarchy *inside* one page. Use the `LoroTree`'s
real parentage — `create_at(parent, index)`, `children(parent)`, and `mov_to`/`mov_before`
for reorder & drag-into.

> **Stopgap to retire:** today every block is `create_at(Root, …)` and "indent" is a
> cosmetic `i64` in meta — the tree is **flat**. Real nesting means deriving depth from tree
> position, not the int. This is build-step 2. It also unlocks drag-**into**, which has no
> honest representation without real parentage.

### Subdocuments (inter-document) → separate `.doc` files
A sub-page / linked doc is **another `LoroDoc` file**, referenced by id from a block — *not* a
deeply nested subtree. Why files, not subtrees, for documents:
- independent **sync granularity** (don't ship the whole tree to edit one sub-page),
- independent **permissions & encryption** per file (central to the DID/vault model),
- bounded per-file size and load.

This matches the collab-OS vision ("spaces of typed files"): a sub-page is just a `.doc`
referenced by another `.doc`.

### Document bundle (decided: **option A**) — *deferred until the single-page editor is solid*

A `.doc` is a **bundle of layers**, not one layer. Naming (option A): **space → `.doc`
(bundle/notebook) → pages**. A `.doc` with one page is just the degenerate single-document
case.

```
.doc bundle                         ← the sync + permission unit
├─ meta layer      one Layer/LoroDoc — a LoroTree of page-references
│    node = a page  → meta { title, layer_key, icon, collapsed }
│    (parent/child + fractional order = the page hierarchy)   → renders the LEFT SIDEBAR
│    + a relations edge-list (LoroMovableList of {from,to,kind}) for backlinks/graph (later)
└─ content layers  one Layer/LoroDoc each — a single block tree per page
     page A, page B, …               → each is exactly what `DocEditor` edits today
     (loaded lazily, only when opened)
```

- The **meta layer** is small, always loaded (cheap sidebar), and is the **sync manifest** —
  it lists every content layer, so the courier knows the set to batch, and a meta update that
  references an unknown layer is the signal to fetch it.
- **Two trees, cleanly separated:** the *meta tree* = page hierarchy (sidebar); the *block
  tree* (per content layer) = in-page structure (build-step 2). "Relations" beyond hierarchy
  live in the meta layer's edge-list.
- **`DocEditor` is unaffected** — it edits one content `LoroDoc` = one page. The bundle is an
  **additive shell layer** (sidebar + page loading) *above* the editor. Per-page undo falls
  out naturally (one `UndoManager` per content layer).
- Storage: e.g. `file/<doc>/meta` + `file/<doc>/page/<id>`, each sealed.

**Build this *after* the single-page editor is solid** (see §8).

> Note: Notion gets cheap partial load/sync for free because it has *no* document container
> at all — it's block-records in a DB, server-authoritative, loaded per-block. We are
> CRDT-native and local-first, so **the container boundary *is* the load/sync boundary** —
> which is exactly why drawing it well matters for us and not for Notion.

---

## 4. The three boundaries — decouple them

| Boundary | Unit | Why |
|---|---|---|
| **Load / materialize** | one `LoroDoc` (a page) | you always import a *whole* container; keep them page-sized |
| **Sync / transport** | a *space* (a batch of docs) | courier batches a space's docs; coarse, convenient replication |
| **Permission / encryption** | per-doc or per-space | your choice; per-doc seal today |

Key facts that force this:
- **You cannot partially load a `LoroDoc`.** `import` materializes the entire current state.
  (Shallow/gc snapshots trim *history*, not current *state*.)
- **Loro replicates per-doc.** There is no "sync just this subtree" and no per-subtree
  access control. So "sync the space as one boundary" is a *courier policy* (batch the docs),
  not a Loro feature — and **synced-to-disk ≠ materialized-in-memory** (pull a space's bytes
  locally, but only `import` the page you open).

Accepted caveat: **no cross-doc atomic transactions** (CRDTs merge per object). Moving a
block across pages is copy-then-delete; mid-sync it may briefly appear in both/neither. Fine
for a notes app.

---

## 5. Rendering & performance

### What a frame costs
Per block, `layout_all` currently does: read text (`String` alloc) → build a `LayoutJob` →
`fonts.layout_job(job)` (which **hashes the text** for a cache lookup) → maybe shape → paint.
egui already **caches the shaped galley** (so unchanged blocks aren't re-shaped) and **clips
off-screen shapes** during tessellation. So the real per-frame tax is **O(N) String allocs +
LayoutJob builds + text hashing**, ~60×/sec while the caret blinks.

A **galley** = egui's laid-out text (positioned glyphs + rows + size + the index↔position map
used for the caret). Shaping it is the expensive step; holding it is the memory cost.

### The plan: Loro-diff-driven galley cache + paint-visible
Loro tells us *exactly* what changed, so we never hash to detect changes:

```
each frame:
  let now = doc.state_frontiers();
  if now != last_rendered {
      for changed_container in doc.diff(&last_rendered, &now) {  // per-block precision
          // Tree diff  → block added/removed/moved → fix order index, add/drop cache entries
          // Text diff  → that block's text changed  → invalidate its galley
          // Map  diff  → kind/indent/done/lang      → invalidate its galley
          invalidate(galley_cache, block_of(changed_container));
      }
      last_rendered = now;
  }
  // layout: reuse cached galley+height for clean blocks (Arc clone + add height);
  //         re-shape only invalidated blocks.
  // paint:  only the blocks intersecting the viewport (ScrollArea::show_viewport).
```

- **Loro** → the dirty set (covers local edits, remote imports, *and* undo uniformly).
- **egui** → the shaped-galley cache + tessellation clipping.
- **us** → hold galley+height per `TreeID`; paint only the visible window.

This kills the O(N) hash tax: a clean block costs an `Arc` clone + a height add.

### Realistic sizing (order-of-magnitude; measure once built)

A block ≈ a paragraph (~40 words). Assumptions: Loro state ~1 KB/block; an egui galley
~10 KB/block (avg); clean-path CPU ~0.08 µs/block with the cache (vs ~0.7 µs naive). Budget
= 16.6 ms/frame at 60 fps.

| Document | ≈ Blocks |
|---|---|
| Typical working note | 30–100 |
| Long spec / essay / book chapter | 150–400 |
| **A whole novel in one file** | **2,000–3,000** |
| Doorstopper / reference book | 5,000–10,000 |
| Pathological paste / log dump | 10k–50k |

| Blocks | Loro RAM | Galleys (all held) | naive CPU/frame | cached CPU/frame |
|---|---|---|---|---|
| 100 | 0.1 MB | 1 MB | 0.07 ms | ~0 |
| 1,000 | 1 MB | 10 MB | 0.7 ms | 0.1 ms |
| 5,000 (big book) | 5 MB | 50 MB | 3.5 ms | 0.4 ms |
| 10,000 | 10 MB | 100 MB | 7 ms | 0.8 ms |
| 50,000 (abuse) | 50 MB | 500 MB | 35 ms | 4 ms |

Thresholds:
- **Full Loro in memory is never the bottleneck** (~50 MB even at 50k blocks).
- **Naive (current):** smooth to ~1–2k blocks; janks ~5–10k; bad ≥ 20k.
- **Cached + paint-visible:** sub-ms CPU to ~10k; the limiter becomes **galley memory**
  (~100 MB at 10k, because we hold a galley per block).
- **Full chunked/viewport virtualization:** O(visible) for CPU *and* memory — flat at any N.

### Decision: cache + paint-visible from day 1; chunked virtualization is an escape hatch
Because subdocs keep a single page page-sized (a book → chapter files), the galley-memory
wall (~10k blocks) sits **above** the realistic ceiling of one page. So the cache approach is
sufficient for every realistic page. Full **chunked/viewport virtualization** is *designed
for* (keyed off a block-count threshold, e.g. ~10k) but probably never *needed* — and if it
is, that page should have been split into subdocs.

> Virtualization fixes *render* cost only. It does **not** fix *load* cost — that's the
> subdoc boundary (§3–4).

---

## 6. Undo / redo

`loro::UndoManager` is **CRDT-aware**: it undoes *your* operations while preserving concurrent
*remote* ones (Ctrl+Z never clobbers a collaborator). It has `set_merge_interval` (time-group
keystrokes into one step) and explicit `group_start`/`group_end`. We have it available, not
wired. It needs a **commit discipline** (sensible `commit()` boundaries) so steps are grained
as "a word / a block op," not "a codepoint" — which is exactly what the command layer
provides. This **replaces** ProseMirror's invertible-Steps undo machinery entirely.

---

## 7. Ownership map

| Concern | Owner |
|---|---|
| document state, merge, history, undo, stable anchors, **diff/invalidation** | **Loro** |
| text shaping cache (galley), off-screen tessellation clipping | **egui** |
| galley/height cache (by `TreeID`), paint-visible, all authoring layers | **`doc_editor`** |
| space sync (batching a space's docs), transport | **courier** (not built yet) |
| per-account store, encryption-at-rest | **vault / identity** |

---

## 8. Build sequence (highest leverage first)

1. **Layout core** — `doc.diff(last, now)`-driven galley/height cache + paint-visible via
   `ScrollArea::show_viewport`. Foundational (perf + correctness). Add a `--stress N` flag to
   the standalone example to measure real frame times against §5.
2. **Real tree nesting** — depth from tree parentage, retire the `indent` int; slots into the
   cached walk; unlocks drag-into.
3. **Selection model + command layer** — ranges (not just a caret); editing intents as
   composable commands; **wire `UndoManager`** + commit boundaries here.
4. **Marks on `LoroText`** — bold / italic / code / link; inline atoms (math / mentions) via a
   sentinel char (U+FFFC) + a mark.
5. **Decoration layer** — generalize `overlays.rs` for drop-lines, comment highlights,
   presence carets (view-only, off the document).
6. **Node-view registry + schema** — per-kind rendering as a registry; a schema describing
   kinds / allowed children / allowed marks. (When embeds / tables / math blocks arrive.)

> **Milestone — "solid single-page editor" = steps 1–5** plus the remaining keyboard map and
> multi-select/bulk-bar (§3/§11). **Build this before the bundle.** Math, comments, and
> presence are the rich-collab tier *after* this milestone.

7. **PDF export** — shell-chrome output (export the doc to a file), not in-canvas. **Renderer
   decision is shared with math**: `typst` would do both math typesetting *and* PDF
   (one dependency); alternatives (`genpdf`/`printpdf`) do PDF only. Decide at the marks/math
   stage.
8. **Document bundle + sidebar** (option A, §3) — meta layer + content layers + the left
   sidebar. Single-doc is the one-page degenerate case, so this is purely additive above the
   now-solid editor.

Later / parallel: the **courier** crate plugs into `Doc::import`/`export` (space sync, §4);
debounce per-keystroke persistence.

---

## 9. Current module map

```
doc_editor/
  src/model.rs     Doc — the Loro CRDT wrapper (UI-agnostic; the merge point)
  src/editor.rs    DocEditor — egui widget; owns caret + slash state, borrows &Doc
  src/theme.rs     design tokens, type scale, geometry
  src/overlays.rs  slash palette (paint-only + geometry; editor owns hit-testing)
  tests/slash_repro.rs   headless event-driven tests (regression guard)
sthalam/src/home_doc.rs  encrypted persistence of the single home doc (ECIES self-seal)
```

---

## 10. Escape hatches & open questions (not blocking)
- **Chunked virtualization** (§5) — implement only past a block-count threshold.
- **Math renderer** — still open: ReX vs typst (typst would double as PDF export).
- **Cross-doc references / transclusion UX** — how a block links/embeds another `.doc`.
- **Per-subtree vs per-doc encryption** — currently per-doc; revisit if shared sub-pages need
  finer ACLs (would push toward the file model even harder).
