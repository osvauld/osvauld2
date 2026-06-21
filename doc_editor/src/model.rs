//! The document model — a typed layer over [`block_doc::BlockDoc`] (the shared CRDT block
//! container). One `.doc` is one Loro doc, the single merge point for editor ops, remote
//! updates ([`Doc::import`]), and persistence ([`Doc::export_snapshot`]); concurrent edits
//! merge without conflict.
//!
//! Schema (block tree):
//! ```text
//! LoroTree "body"                     ← the block hierarchy: real parent/child nesting
//!   each node (TreeID) = one block; a node's children are its nested blocks
//!     node.meta : LoroMap
//!       ├─ "kind"    : "paragraph" | "h1".."h3" | "li" | "ol" | "todo" | "quote" | "code" | "divider"
//!       ├─ "done"    : bool            ← to-do checked state
//!       ├─ "lang"    : string          ← code-block language tag
//!       └─ "content" : LoroText        ← the block's text (inline marks live here)
//! ```
//! `block_doc` owns the structure (tree ops, text, undo, sync); this layer adds what only
//! `.doc` needs: the [`BlockKind`] vocabulary, inline marks/[`Run`]s, and the seeded-paragraph
//! invariant (the caret always has a home). Block identity is the stable `TreeID`; text
//! positions are Unicode code points, matching egui's `CCursor`.

use block_doc::{BlockDoc, TextCursor};
use loro::{ExpandType, LoroDoc, LoroError, LoroValue, StyleConfig, StyleConfigMap, TextDelta, TreeID};
use rich_text::{Marks, Run};

/// What a block is — drives the type scale and editing behaviour. Stored as a short string in
/// `meta["kind"]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockKind {
    Paragraph,
    H1,
    H2,
    H3,
    BulletList,
    NumberedList,
    Todo,
    Quote,
    Code,
    Divider,
}

impl BlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockKind::Paragraph => "paragraph",
            BlockKind::H1 => "h1",
            BlockKind::H2 => "h2",
            BlockKind::H3 => "h3",
            BlockKind::BulletList => "li",
            BlockKind::NumberedList => "ol",
            BlockKind::Todo => "todo",
            BlockKind::Quote => "quote",
            BlockKind::Code => "code",
            BlockKind::Divider => "divider",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "h1" => BlockKind::H1,
            "h2" => BlockKind::H2,
            "h3" => BlockKind::H3,
            "li" => BlockKind::BulletList,
            "ol" => BlockKind::NumberedList,
            "todo" => BlockKind::Todo,
            "quote" => BlockKind::Quote,
            "code" => BlockKind::Code,
            "divider" => BlockKind::Divider,
            _ => BlockKind::Paragraph,
        }
    }

    /// Whether this kind is a list item — the only kinds that nest and continue on Enter.
    pub fn is_list(self) -> bool {
        matches!(self, BlockKind::BulletList | BlockKind::NumberedList | BlockKind::Todo)
    }
}

/// The boolean inline-mark keys (`link` is separate: it carries a URL value).
const FLAG_MARKS: [&str; 4] = ["bold", "italic", "strike", "code"];

/// The document: the schema-aware `.doc` layer over a [`BlockDoc`]. All mutating methods take
/// `&self` (Loro is interior-mutable), so it can be shared as the merge point without `&mut`.
pub struct Doc {
    inner: BlockDoc,
}

impl Doc {
    /// A fresh document with one empty paragraph (so the caret always has a home).
    pub fn new() -> Self {
        Self { inner: BlockDoc::new_with(Self::setup) }
    }

    /// Load a document from a Loro snapshot (e.g. decrypted from the vault).
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, LoroError> {
        Ok(Self { inner: BlockDoc::from_snapshot_with(bytes, Self::setup)? })
    }

    /// A document embedded inside an existing (shared) `LoroDoc`, on its own named tree — the seam
    /// a host app uses to host a block doc inside its runtime CRDT (so the host's persistence/sync
    /// carry it). The host keeps the `doc`; this just drives the `tree` block hierarchy within it.
    pub fn on_tree(doc: LoroDoc, tree: &str) -> Self {
        let name = tree.to_string();
        Self { inner: BlockDoc::on_shared(doc, tree, move |d| Self::setup_on(d, &name)) }
    }

    /// Pre-undo setup: register mark styles and seed the first paragraph, so neither the seed
    /// nor imported history is undoable; the first edit is step one.
    fn setup(doc: &LoroDoc) {
        Self::setup_on(doc, "body");
    }

    /// As [`setup`](Self::setup) but seeds a named tree — used by [`on_tree`](Self::on_tree) so an
    /// embedded doc gets its first paragraph in its own tree, not the default `body`.
    fn setup_on(doc: &LoroDoc, tree: &str) {
        // Register inline-mark styles. Loro's defaults cover bold/italic/underline/link but NOT
        // `strike`/`code`, and marking an unconfigured key errors — so declare the full set
        // (config is runtime, not in the snapshot, so it must run on every new/from_snapshot).
        // `None` = a mark covers exactly the chars it was applied to: typing at either edge does
        // *not* inherit it, so bolding a span then typing past it doesn't make the new text bold.
        // Insertions strictly inside a marked run still inherit. Continuing a mark while typing at
        // the edge needs a pending-format ("active mark") UI state we don't have yet.
        let mut styles = StyleConfigMap::new();
        for key in FLAG_MARKS {
            styles.insert(key.into(), StyleConfig { expand: ExpandType::None });
        }
        styles.insert("link".into(), StyleConfig { expand: ExpandType::None });
        doc.config_text_style(styles);
        BlockDoc::seed_if_empty_on(doc, tree, BlockKind::Paragraph.as_str());
    }

    // --- Undo / redo (CRDT-aware) -----------------------------------------------------

    /// Undo this peer's last edit-group. Callers must NOT commit around it (an extra commit
    /// after an undo clears the redo stack); the manager checkpoints pending edits itself.
    pub fn undo(&self) -> bool {
        self.inner.undo()
    }

    /// Redo the last undone edit-group. Returns whether anything was redone.
    pub fn redo(&self) -> bool {
        self.inner.redo()
    }

    /// Whether there is anything to undo / redo (for graying out UI later).
    pub fn can_undo(&self) -> bool {
        self.inner.can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.inner.can_redo()
    }

    // --- Network + persistence seam ---------------------------------------------------

    /// Apply remote CRDT updates (snapshot or update bytes) from the network; conflict-free.
    pub fn import(&self, bytes: &[u8]) -> Result<(), LoroError> {
        self.inner.import(bytes)?;
        // Guarantee at least one block (an empty doc has nowhere for the caret to live)
        if self.inner.is_empty() {
            self.create_block(0, BlockKind::Paragraph, "");
        }
        Ok(())
    }

    /// A full snapshot of the document, for encrypted persistence to the vault.
    pub fn export_snapshot(&self) -> Vec<u8> {
        self.inner.export_snapshot()
    }

    /// Flush pending operations into the oplog (so a following export/snapshot sees them).
    pub fn commit(&self) {
        self.inner.commit();
    }

    // --- Reading ----------------------------------------------------------------------

    /// Every block in document order (pre-order DFS) paired with its depth — the editor's row
    /// order, a parent immediately followed by its nested subtree.
    pub fn blocks(&self) -> Vec<(TreeID, usize)> {
        self.inner.blocks()
    }

    /// The block ids, in document order (pre-order DFS).
    pub fn block_ids(&self) -> Vec<TreeID> {
        self.inner.block_ids()
    }

    /// A block's nesting depth: 0 at the top level, +1 per ancestor.
    pub fn depth(&self, id: TreeID) -> usize {
        self.inner.depth(id)
    }

    /// The block's parent, or `None` if it is top-level.
    pub fn parent_of(&self, id: TreeID) -> Option<TreeID> {
        self.inner.parent_of(id)
    }

    /// A block's direct children, in order.
    pub fn children(&self, id: TreeID) -> Vec<TreeID> {
        self.inner.children(id)
    }

    /// The block immediately before `id` among its siblings (same parent), if any.
    pub fn prev_sibling(&self, id: TreeID) -> Option<TreeID> {
        self.inner.prev_sibling(id)
    }

    /// The kind of a block.
    pub fn kind(&self, id: TreeID) -> BlockKind {
        BlockKind::from_str(&self.inner.kind(id))
    }

    /// A block's text as a plain string (for layout).
    pub fn text(&self, id: TreeID) -> String {
        self.inner.text(id)
    }

    /// A block's length in Unicode code points (matches caret offsets).
    pub fn text_len(&self, id: TreeID) -> usize {
        self.inner.text_len(id)
    }

    /// Capture a stable cursor at code-point `pos` in block `id` — survives concurrent edits
    /// (the editor anchors its caret/selection here so a remote insert can't strand it). See
    /// [`BlockDoc::cursor_at`].
    pub fn cursor_at(&self, id: TreeID, pos: usize) -> Option<TextCursor> {
        self.inner.cursor_at(id, pos)
    }

    /// Resolve a cursor from [`cursor_at`](Self::cursor_at) to a live offset in the current state.
    pub fn resolve_cursor(&self, cursor: &TextCursor) -> Option<usize> {
        self.inner.resolve_cursor(cursor)
    }

    /// A block's text as styled [`Run`]s — the substring spans with their inline marks, in
    /// order (Loro hands the rich text back already split into runs via its delta).
    pub fn runs(&self, id: TreeID) -> Vec<Run> {
        self.inner
            .content(id)
            .to_delta()
            .into_iter()
            .filter_map(|d| {
                // A text container's delta is all `Insert`s; ignore anything else defensively.
                let TextDelta::Insert { insert, attributes } = d else { return None };
                let attrs = attributes.unwrap_or_default();
                let mut marks = Marks::new();
                for key in FLAG_MARKS {
                    if matches!(attrs.get(key), Some(LoroValue::Bool(true))) {
                        marks = marks.flag(key);
                    }
                }
                if let Some(LoroValue::String(s)) = attrs.get("link") {
                    marks = marks.with("link", s.to_string());
                }
                // doc_editor carries no per-run explicit colour: block ink rides on the layout
                // `Style`, and code-block syntax colour is applied in `layout`, not here.
                Some(Run { text: insert, marks, color: None })
            })
            .collect()
    }

    /// Whether `key` is set across the entire `[start, end)` range (so a toggle removes it).
    pub fn mark_covers(&self, id: TreeID, start: usize, end: usize, key: &str) -> bool {
        if start >= end {
            return true;
        }
        let mut pos = 0usize;
        for run in self.runs(id) {
            let len = run.text.chars().count();
            let (run_start, run_end) = (pos, pos + len);
            pos = run_end;
            // The part of this run inside [start, end). If any such part lacks the mark, the
            // range isn't fully covered.
            if run_start.max(start) < run_end.min(end) && !run.marks.has(key) {
                return false;
            }
        }
        true
    }

    /// Whether a to-do is checked.
    pub fn done(&self, id: TreeID) -> bool {
        self.inner.meta_bool(id, "done").unwrap_or(false)
    }

    /// A code block's language tag, if set.
    pub fn lang(&self, id: TreeID) -> Option<String> {
        self.inner.meta(id, "lang")
    }

    // --- Mutations (block program ops) ------------------------------------------------

    pub fn set_kind(&self, id: TreeID, kind: BlockKind) {
        self.inner.set_kind(id, kind.as_str());
    }

    /// Nest `id` under its previous sibling (as that sibling's last child) — the Tab gesture.
    /// No-op (returns `false`) when there is no previous sibling to nest under.
    pub fn indent(&self, id: TreeID) -> bool {
        self.inner.indent(id)
    }

    /// Outdent `id`: lift it to be the sibling immediately after its parent — the Shift-Tab
    /// gesture. No-op (returns `false`) when already top-level.
    pub fn outdent(&self, id: TreeID) -> bool {
        self.inner.outdent(id)
    }

    /// Move `block` to be the sibling immediately before `target` (drag-reorder). No-op when
    /// `target` lies in `block`'s own subtree (would make a cycle).
    pub fn move_before(&self, block: TreeID, target: TreeID) -> bool {
        self.inner.move_before(block, target)
    }

    /// Move `block` to be the sibling immediately after `target` (drag-reorder).
    pub fn move_after(&self, block: TreeID, target: TreeID) -> bool {
        self.inner.move_after(block, target)
    }

    /// Move `block` to be the last child of `parent` — the drop-INTO / nest gesture.
    pub fn move_into(&self, block: TreeID, parent: TreeID) -> bool {
        self.inner.move_into(block, parent)
    }

    /// Set a to-do's checked state.
    pub fn set_done(&self, id: TreeID, done: bool) {
        self.inner.set_meta_bool(id, "done", done);
    }

    /// Set a code block's language tag.
    pub fn set_lang(&self, id: TreeID, lang: &str) {
        self.inner.set_meta(id, "lang", lang);
    }

    /// Replace the full text of a block (delete all then insert).
    pub fn set_block_text(&self, id: TreeID, text: &str) {
        self.inner.set_block_text(id, text);
    }

    /// Insert `s` at code-point offset `at` in the block's text.
    pub fn insert_text(&self, id: TreeID, at: usize, s: &str) {
        self.inner.insert_text(id, at, s);
    }

    /// Delete `len` code points starting at offset `at`.
    pub fn delete_text(&self, id: TreeID, at: usize, len: usize) {
        self.inner.delete_text(id, at, len);
    }

    /// Apply a boolean inline mark over `[start, end)`. Marks are CRDT range annotations, so
    /// they survive concurrent edits and shift with insertions/deletions.
    pub fn mark(&self, id: TreeID, start: usize, end: usize, key: &str) {
        if start < end {
            self.inner.content(id).mark(start..end, key, true).expect("mark");
        }
    }

    /// Apply a `"link"` mark carrying its target URL over `[start, end)`.
    pub fn mark_link(&self, id: TreeID, start: usize, end: usize, url: &str) {
        if start < end {
            self.inner.content(id).mark(start..end, "link", url).expect("mark link");
        }
    }

    /// Remove the inline mark `key` over `[start, end)`.
    pub fn unmark(&self, id: TreeID, start: usize, end: usize, key: &str) {
        if start < end {
            self.inner.content(id).unmark(start..end, key).expect("unmark");
        }
    }

    /// Create a block at sibling `index` among the **top-level** blocks.
    pub fn create_block(&self, index: usize, kind: BlockKind, text: &str) -> TreeID {
        self.inner.insert_at(index, kind.as_str(), text)
    }

    /// Create a block as the sibling immediately after `sibling` (same parent, same depth) —
    /// how Enter, paste, and the gutter `+` grow the document in place at any nesting level.
    pub fn insert_after(&self, sibling: TreeID, kind: BlockKind, text: &str) -> TreeID {
        self.inner.insert_after(sibling, kind.as_str(), text)
    }

    /// Delete a block, promoting its children into its slot first (in order, one level
    /// shallower), so removing a parent line doesn't take its nested blocks down with it.
    pub fn delete_block(&self, id: TreeID) {
        self.inner.delete_block(id);
    }
}

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

/// One block's text as a [`text_edit::TextBuffer`], so in-block editing runs through the shared
/// [`text_edit::TextField`] kernel; cross-block structure stays the editor's job.
pub(crate) struct BlockBuf<'a> {
    pub doc: &'a Doc,
    pub id: TreeID,
}

impl text_edit::TextBuffer for BlockBuf<'_> {
    fn char_len(&self) -> usize {
        self.doc.text_len(self.id)
    }
    fn text(&self) -> String {
        self.doc.text(self.id)
    }
    fn insert(&mut self, at: usize, s: &str) {
        self.doc.insert_text(self.id, at, s);
    }
    fn delete(&mut self, at: usize, len: usize) {
        self.doc.delete_text(self.id, at, len);
    }
    fn mark_covers(&self, a: usize, b: usize, key: &str) -> bool {
        self.doc.mark_covers(self.id, a, b, key)
    }
    fn set_mark(&mut self, a: usize, b: usize, key: &str, on: bool) {
        if on {
            self.doc.mark(self.id, a, b, key);
        } else {
            self.doc.unmark(self.id, a, b, key);
        }
    }
}

#[cfg(test)]
mod tests;
