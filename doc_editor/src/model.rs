//! The document model — a **Loro CRDT**.
//!
//! One `.doc` is one [`loro::LoroDoc`], and it is the single **merge point**: the editor
//! program applies block/text operations to it, the network (courier, later) feeds it
//! remote updates via [`Doc::import`], and persistence reads a snapshot via
//! [`Doc::export_snapshot`]. Because it's a CRDT, concurrent edits from those sources
//! merge without conflict.
//!
//! **Schema (block tree):**
//! ```text
//! LoroTree "body"                     ← the block hierarchy: real parent/child nesting
//!   each node (TreeID) = one block; a node's children are its nested blocks
//!     node.meta : LoroMap
//!       ├─ "kind"    : "paragraph" | "h1".."h3" | "li" | "ol" | "todo" | "quote" | "code" | "divider"
//!       ├─ "done"    : bool            ← to-do checked state
//!       ├─ "lang"    : string          ← code-block language tag
//!       └─ "content" : LoroText        ← the block's text (marks come later)
//! ```
//! **Nesting is structural**, not a stored number: [`indent`](Doc::indent) / [`outdent`](Doc::outdent)
//! reparent the node (`indent` → child of its previous sibling; `outdent` → sibling after its
//! parent), and [`depth`](Doc::depth) is the count of ancestors. Document order is a pre-order
//! DFS ([`blocks`](Doc::blocks)), so a parent is immediately followed by its subtree — exactly
//! the visual top-to-bottom order the editor lays out and navigates by.
//!
//! Block identity is the stable `TreeID` (survives reorders and remote edits); the caret
//! is anchored to it, not to a list index. Text positions are Unicode code points, which
//! is exactly what egui's caret (`CCursor`) counts — so caret offsets map straight across.

use std::cell::RefCell;

use loro::{
    ExportMode, LoroDoc, LoroError, LoroText, LoroTree, LoroValue, TextDelta, TreeID,
    TreeParentId, UndoManager, ValueOrContainer,
};

/// What a block *is* — drives the type scale and editing behaviour. Stored as a short
/// string in the node's `meta["kind"]`.
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

/// A styled span of a block's text — a substring plus the inline marks active on it. This is
/// the *read shape* the editor lays out: one block's [`LoroText`] becomes a sequence of
/// `Run`s (Loro stores marks as range annotations and hands them back already split into
/// runs via its delta). Marks ride along with edits for free, since they're anchored in the
/// CRDT, not by offset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    /// The link target, if this run is a link.
    pub link: Option<String>,
}

impl Run {
    /// Whether the boolean mark `key` (`"bold"`/`"italic"`/`"strike"`/`"code"`/`"link"`) is
    /// set on this run — used for coverage checks when toggling.
    fn has(&self, key: &str) -> bool {
        match key {
            "bold" => self.bold,
            "italic" => self.italic,
            "strike" => self.strike,
            "code" => self.code,
            "link" => self.link.is_some(),
            _ => false,
        }
    }
}

/// The document: a thin, schema-aware wrapper over a [`LoroDoc`]. All mutating methods
/// take `&self` — Loro is interior-mutable — so the document can be shared as the merge
/// point without `&mut` juggling.
pub struct Doc {
    doc: LoroDoc,
    /// CRDT-aware undo/redo for *this peer's* edits (a remote peer's concurrent edits are
    /// preserved across an undo). `RefCell` because `undo`/`redo` need `&mut` while the
    /// document's methods take `&self`.
    undo: RefCell<UndoManager>,
}

impl Doc {
    /// The tree container that holds the blocks.
    const BODY: &'static str = "body";

    /// Keystrokes this close together (ms) fold into one undo step, so Ctrl+Z removes a
    /// word/burst rather than a single codepoint.
    const UNDO_MERGE_MS: i64 = 250;

    /// A fresh document with one empty paragraph (so the caret always has a home).
    pub fn new() -> Self {
        let doc = LoroDoc::new();
        // Fractional indexing gives blocks a stable, mergeable sibling order — needed for
        // ordered insert and (later) drag-reorder. Enable before any node is created.
        doc.get_tree(Self::BODY).enable_fractional_index(0);
        Self::finish(doc)
    }

    /// Load a document from a Loro snapshot (e.g. decrypted from the vault).
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, LoroError> {
        let doc = LoroDoc::new();
        doc.import(bytes)?;
        doc.get_tree(Self::BODY).enable_fractional_index(0);
        Ok(Self::finish(doc))
    }

    /// Seed an empty paragraph if the tree is empty, commit, then attach a fresh
    /// `UndoManager` — created *after* the seed (and any imported history) so neither is
    /// undoable; the first real edit is the first undo step.
    fn finish(doc: LoroDoc) -> Self {
        let tree = doc.get_tree(Self::BODY);
        if tree.children(TreeParentId::Root).is_none_or(|c| c.is_empty()) {
            let id = tree.create_at(TreeParentId::Root, 0).expect("seed block");
            let meta = tree.get_meta(id).expect("fresh node");
            meta.insert("kind", BlockKind::Paragraph.as_str()).expect("seed kind");
            // Pre-create the content container (see `create_block` for why).
            meta.get_or_create_container("content", LoroText::new()).expect("seed content");
        }
        doc.commit();
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(Self::UNDO_MERGE_MS);
        Self { doc, undo: RefCell::new(undo) }
    }

    // --- Undo / redo (CRDT-aware) -----------------------------------------------------

    /// Undo this peer's last edit-group. The `UndoManager` records a checkpoint of any
    /// pending edits internally before reverting, so callers must **not** commit around it
    /// (an extra commit after an undo clears the redo stack). Returns whether anything was
    /// undone.
    pub fn undo(&self) -> bool {
        self.undo.borrow_mut().undo().unwrap_or(false)
    }

    /// Redo the last undone edit-group. Returns whether anything was redone.
    pub fn redo(&self) -> bool {
        self.undo.borrow_mut().redo().unwrap_or(false)
    }

    /// Whether there is anything to undo / redo (for graying out UI later).
    pub fn can_undo(&self) -> bool {
        self.undo.borrow().can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.undo.borrow().can_redo()
    }

    // --- Network + persistence seam ---------------------------------------------------

    /// Apply remote CRDT updates (a snapshot or update bytes) from the network. This is
    /// how the courier-style transport feeds the document; the merge is conflict-free.
    pub fn import(&self, bytes: &[u8]) -> Result<(), LoroError> {
        self.doc.import(bytes)?;
        self.ensure_nonempty();
        Ok(())
    }

    /// A full snapshot of the document, for encrypted persistence to the vault.
    pub fn export_snapshot(&self) -> Vec<u8> {
        self.doc.commit();
        self.doc.export(ExportMode::Snapshot).expect("snapshot export never fails")
    }

    /// Flush pending operations into the oplog (so a following export/snapshot sees them).
    pub fn commit(&self) {
        self.doc.commit();
    }

    // --- Reading ----------------------------------------------------------------------

    /// Every block in document order — a pre-order DFS of the tree — paired with its depth
    /// (0 = top level). This *is* the editor's row order: a parent is immediately followed by
    /// its nested subtree, so the flat list the layout walks matches what the reader sees.
    pub fn blocks(&self) -> Vec<(TreeID, usize)> {
        fn walk(tree: &LoroTree, parent: TreeParentId, depth: usize, out: &mut Vec<(TreeID, usize)>) {
            if let Some(kids) = tree.children(parent) {
                for id in kids {
                    out.push((id, depth));
                    walk(tree, TreeParentId::Node(id), depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.doc.get_tree(Self::BODY), TreeParentId::Root, 0, &mut out);
        out
    }

    /// The block ids, in document order (pre-order DFS).
    pub fn block_ids(&self) -> Vec<TreeID> {
        self.blocks().into_iter().map(|(id, _)| id).collect()
    }

    /// A block's nesting depth: 0 at the top level, +1 per ancestor.
    pub fn depth(&self, id: TreeID) -> usize {
        let tree = self.doc.get_tree(Self::BODY);
        let mut depth = 0;
        let mut cur = id;
        while let Some(TreeParentId::Node(p)) = tree.parent(cur) {
            depth += 1;
            cur = p;
        }
        depth
    }

    /// The block's parent, or `None` if it is top-level.
    pub fn parent_of(&self, id: TreeID) -> Option<TreeID> {
        match self.doc.get_tree(Self::BODY).parent(id) {
            Some(TreeParentId::Node(p)) => Some(p),
            _ => None,
        }
    }

    /// A block's direct children, in order.
    pub fn children(&self, id: TreeID) -> Vec<TreeID> {
        self.doc.get_tree(Self::BODY).children(TreeParentId::Node(id)).unwrap_or_default()
    }

    /// The block immediately before `id` among its siblings (same parent), if any.
    pub fn prev_sibling(&self, id: TreeID) -> Option<TreeID> {
        let tree = self.doc.get_tree(Self::BODY);
        let siblings = tree.children(tree.parent(id)?)?;
        let pos = siblings.iter().position(|&x| x == id)?;
        pos.checked_sub(1).map(|i| siblings[i])
    }

    /// The kind of a block.
    pub fn kind(&self, id: TreeID) -> BlockKind {
        let meta = self.doc.get_tree(Self::BODY).get_meta(id).expect("live node");
        match meta.get("kind") {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => BlockKind::from_str(s.as_ref()),
            _ => BlockKind::Paragraph,
        }
    }

    /// A block's text as a plain string (for layout).
    pub fn text(&self, id: TreeID) -> String {
        self.content(id).to_string()
    }

    /// A block's length in Unicode code points (matches caret offsets).
    pub fn text_len(&self, id: TreeID) -> usize {
        self.content(id).len_unicode()
    }

    /// A block's text as styled [`Run`]s — the substring spans with their inline marks, in
    /// order. (Loro hands the rich text back already split into runs via its delta.)
    pub fn runs(&self, id: TreeID) -> Vec<Run> {
        self.content(id)
            .to_delta()
            .into_iter()
            .filter_map(|d| {
                // A text container's delta is all `Insert`s; ignore anything else defensively.
                let TextDelta::Insert { insert, attributes } = d else { return None };
                let attrs = attributes.unwrap_or_default();
                let flag = |k: &str| matches!(attrs.get(k), Some(LoroValue::Bool(true)));
                let link = match attrs.get("link") {
                    Some(LoroValue::String(s)) => Some(s.to_string()),
                    _ => None,
                };
                Some(Run {
                    text: insert,
                    bold: flag("bold"),
                    italic: flag("italic"),
                    strike: flag("strike"),
                    code: flag("code"),
                    link,
                })
            })
            .collect()
    }

    /// Whether the mark `key` is set across the **entire** `[start, end)` range — i.e. a
    /// toggle of `key` over that range should *remove* it rather than add it.
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
            if run_start.max(start) < run_end.min(end) && !run.has(key) {
                return false;
            }
        }
        true
    }

    /// Whether a to-do is checked.
    pub fn done(&self, id: TreeID) -> bool {
        matches!(self.meta_value(id, "done"), Some(LoroValue::Bool(true)))
    }

    /// A code block's language tag, if set.
    pub fn lang(&self, id: TreeID) -> Option<String> {
        match self.meta_value(id, "lang") {
            Some(LoroValue::String(s)) => Some(s.to_string()),
            _ => None,
        }
    }

    // --- Mutations (block program ops) ------------------------------------------------

    pub fn set_kind(&self, id: TreeID, kind: BlockKind) {
        let meta = self.doc.get_tree(Self::BODY).get_meta(id).expect("live node");
        meta.insert("kind", kind.as_str()).expect("set kind");
    }

    /// Nest `id` under its previous sibling (appended as that sibling's last child) — the
    /// **Tab** gesture. No-op (returns `false`) when there is no previous sibling to nest
    /// under (e.g. the first block among its peers).
    pub fn indent(&self, id: TreeID) -> bool {
        let Some(prev) = self.prev_sibling(id) else { return false };
        self.doc.get_tree(Self::BODY).mov(id, prev).expect("indent");
        true
    }

    /// Outdent `id`: lift it to be the sibling immediately after its current parent — the
    /// **Shift-Tab** gesture. No-op (returns `false`) when it is already top-level.
    pub fn outdent(&self, id: TreeID) -> bool {
        let Some(parent) = self.parent_of(id) else { return false };
        self.doc.get_tree(Self::BODY).mov_after(id, parent).expect("outdent");
        true
    }

    /// Set a to-do's checked state.
    pub fn set_done(&self, id: TreeID, done: bool) {
        let meta = self.doc.get_tree(Self::BODY).get_meta(id).expect("live node");
        meta.insert("done", done).expect("set done");
    }

    /// Set a code block's language tag.
    pub fn set_lang(&self, id: TreeID, lang: &str) {
        let meta = self.doc.get_tree(Self::BODY).get_meta(id).expect("live node");
        meta.insert("lang", lang).expect("set lang");
    }

    /// Insert `s` at code-point offset `at` in the block's text.
    pub fn insert_text(&self, id: TreeID, at: usize, s: &str) {
        self.content(id).insert(at, s).expect("insert text");
    }

    /// Delete `len` code points starting at offset `at`.
    pub fn delete_text(&self, id: TreeID, at: usize, len: usize) {
        if len > 0 {
            self.content(id).delete(at, len).expect("delete text");
        }
    }

    /// Apply a boolean inline mark (`"bold"`/`"italic"`/`"strike"`/`"code"`) over `[start,
    /// end)` of a block's text. Marks are CRDT range annotations, so they survive concurrent
    /// edits and shift with insertions/deletions.
    pub fn mark(&self, id: TreeID, start: usize, end: usize, key: &str) {
        if start < end {
            self.content(id).mark(start..end, key, true).expect("mark");
        }
    }

    /// Apply a `"link"` mark carrying its target URL over `[start, end)`.
    pub fn mark_link(&self, id: TreeID, start: usize, end: usize, url: &str) {
        if start < end {
            self.content(id).mark(start..end, "link", url).expect("mark link");
        }
    }

    /// Remove the inline mark `key` over `[start, end)`.
    pub fn unmark(&self, id: TreeID, start: usize, end: usize, key: &str) {
        if start < end {
            self.content(id).unmark(start..end, key).expect("unmark");
        }
    }

    /// Create a block at sibling `index` among the **top-level** blocks.
    pub fn create_block(&self, index: usize, kind: BlockKind, text: &str) -> TreeID {
        let id =
            self.doc.get_tree(Self::BODY).create_at(TreeParentId::Root, index).expect("create block");
        self.init_block(id, kind, text);
        id
    }

    /// Create a block as the sibling immediately after `sibling` (same parent, so the same
    /// depth) — how Enter, paste, and the gutter `+` grow the document *in place*, whatever
    /// nesting level the caret is at.
    pub fn insert_after(&self, sibling: TreeID, kind: BlockKind, text: &str) -> TreeID {
        let tree = self.doc.get_tree(Self::BODY);
        let parent = tree.parent(sibling).unwrap_or(TreeParentId::Root);
        let index = tree
            .children(parent)
            .and_then(|sibs| sibs.iter().position(|&x| x == sibling))
            .map_or(0, |i| i + 1);
        let id = tree.create_at(parent, index).expect("create block");
        self.init_block(id, kind, text);
        id
    }

    /// Delete a block. Its children are **promoted** into its slot first (kept in order, one
    /// level shallower), because loro's `delete` hides the whole subtree — so a parent line
    /// can be removed (merged away, say) without taking its nested blocks down with it.
    pub fn delete_block(&self, id: TreeID) {
        let tree = self.doc.get_tree(Self::BODY);
        // Re-home each child as a sibling right after `id`, preserving order: walking in
        // reverse means each `mov_after` lands ahead of the previously moved one.
        for &child in tree.children(TreeParentId::Node(id)).unwrap_or_default().iter().rev() {
            tree.mov_after(child, id).expect("promote child");
        }
        tree.delete(id).expect("delete block");
    }

    // --- Internals --------------------------------------------------------------------

    /// Set a fresh node's kind and **eagerly create its `content` text container**, so the
    /// first text edit into it is a *pure* text op. (A text-insert bundled with a lazy
    /// nested-container creation in one undo step breaks loro's redo for tree-nested text —
    /// regression-guarded by `raw_tree_nested_text_undo_redo`.)
    fn init_block(&self, id: TreeID, kind: BlockKind, text: &str) {
        let meta = self.doc.get_tree(Self::BODY).get_meta(id).expect("fresh node");
        meta.insert("kind", kind.as_str()).expect("kind");
        let content: LoroText =
            meta.get_or_create_container("content", LoroText::new()).expect("content");
        if !text.is_empty() {
            content.insert(0, text).expect("seed text");
        }
    }

    /// The block's content text container, created on first access.
    fn content(&self, id: TreeID) -> LoroText {
        let meta = self.doc.get_tree(Self::BODY).get_meta(id).expect("live node");
        meta.get_or_create_container("content", LoroText::new()).expect("content text")
    }

    /// Read a plain (non-container) value from a block's meta map.
    fn meta_value(&self, id: TreeID, key: &str) -> Option<LoroValue> {
        let meta = self.doc.get_tree(Self::BODY).get_meta(id).expect("live node");
        match meta.get(key) {
            Some(ValueOrContainer::Value(v)) => Some(v),
            _ => None,
        }
    }

    /// Guarantee at least one block exists (a doc with no blocks has nowhere for the
    /// caret to live). Called after construction and after a remote import.
    fn ensure_nonempty(&self) {
        let tree = self.doc.get_tree(Self::BODY);
        if tree.children(TreeParentId::Root).is_none_or(|c| c.is_empty()) {
            self.create_block(0, BlockKind::Paragraph, "");
        }
    }
}

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
