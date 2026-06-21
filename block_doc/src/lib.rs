use std::cell::RefCell;

use loro::{
    ExportMode, LoroDoc, LoroText, LoroTree, LoroValue, TreeID, TreeParentId, UndoManager,
    ValueOrContainer,
};
use loro::cursor::{Cursor, Side};

pub use loro::LoroError;
pub use loro::cursor::Cursor as TextCursor;

pub type BlockId = TreeID;

pub struct BlockDoc {
    doc: LoroDoc,
    // which LoroTree in `doc` holds the blocks — `BODY` by default, a custom name when several
    // block trees share one doc (the embed seam: a doc living inside a host app's CRDT).
    tree: String,
    // undo/redo need &mut UndoManager while doc's own methods take &self
    undo: RefCell<UndoManager>,
}

impl BlockDoc {
    const BODY: &'static str = "body";
    // Burst edits within this window fold into one undo step
    const UNDO_MERGE_MS: i64 = 250;

    pub fn new() -> Self {
        Self::new_with(|_| {})
    }

    /// `setup` runs before the UndoManager exists — seeding/config done there is not undoable.
    pub fn new_with(setup: impl FnOnce(&LoroDoc)) -> Self {
        Self::new_on(Self::BODY, setup)
    }

    /// Like [`new_with`](Self::new_with) but on a named tree, so several block trees can live in
    /// one doc.
    pub fn new_on(tree: &str, setup: impl FnOnce(&LoroDoc)) -> Self {
        let doc = LoroDoc::new();
        // Must enable before any node is created (fractional indexing for stable sibling order)
        doc.get_tree(tree).enable_fractional_index(0);
        Self::finish(doc, tree.to_string(), setup)
    }

    /// Build over an existing (shared) `LoroDoc` on a named tree — the embed seam: the host owns
    /// the doc (persistence, sync, other containers) and this just operates a block tree inside it.
    pub fn on_shared(doc: LoroDoc, tree: &str, setup: impl FnOnce(&LoroDoc)) -> Self {
        doc.get_tree(tree).enable_fractional_index(0);
        Self::finish(doc, tree.to_string(), setup)
    }

    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, LoroError> {
        Self::from_snapshot_with(bytes, |_| {})
    }

    pub fn from_snapshot_with(
        bytes: &[u8],
        setup: impl FnOnce(&LoroDoc),
    ) -> Result<Self, LoroError> {
        let doc = LoroDoc::new();
        doc.import(bytes)?;
        doc.get_tree(Self::BODY).enable_fractional_index(0);
        Ok(Self::finish(doc, Self::BODY.to_string(), setup))
    }

    /// For `setup` closures: seed one empty block when the doc has none, pre-undo.
    pub fn seed_if_empty(doc: &LoroDoc, kind: &str) {
        Self::seed_if_empty_on(doc, Self::BODY, kind)
    }

    /// Seed an empty block on a named tree when it has none, pre-undo (the embed-seam counterpart
    /// of [`seed_if_empty`](Self::seed_if_empty)).
    pub fn seed_if_empty_on(doc: &LoroDoc, tree: &str, kind: &str) {
        let tree = doc.get_tree(tree);
        if tree.children(TreeParentId::Root).is_none_or(|c| c.is_empty()) {
            let id = tree.create_at(TreeParentId::Root, 0).expect("seed block");
            Self::init_block_raw(&tree, id, kind, "");
        }
    }

    // UndoManager created after setup+commit so imported/seeded history is not undoable
    fn finish(doc: LoroDoc, tree: String, setup: impl FnOnce(&LoroDoc)) -> Self {
        setup(&doc);
        doc.commit();
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(Self::UNDO_MERGE_MS);
        Self { doc, tree, undo: RefCell::new(undo) }
    }

    pub fn undo(&self) -> bool {
        self.undo.borrow_mut().undo().unwrap_or(false)
    }

    pub fn redo(&self) -> bool {
        self.undo.borrow_mut().redo().unwrap_or(false)
    }

    pub fn can_undo(&self) -> bool {
        self.undo.borrow().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.borrow().can_redo()
    }

    pub fn undo_count(&self) -> usize {
        self.undo.borrow().undo_count()
    }

    pub fn import(&self, bytes: &[u8]) -> Result<(), LoroError> {
        self.doc.import(bytes)?;
        Ok(())
    }

    pub fn export_snapshot(&self) -> Vec<u8> {
        self.doc.commit();
        self.doc.export(ExportMode::Snapshot).expect("snapshot export never fails")
    }

    pub fn commit(&self) {
        self.doc.commit();
    }

    /// Every block in document order (pre-order DFS) with its nesting depth.
    pub fn blocks(&self) -> Vec<(BlockId, usize)> {
        fn walk(tree: &LoroTree, parent: TreeParentId, depth: usize, out: &mut Vec<(BlockId, usize)>) {
            if let Some(kids) = tree.children(parent) {
                for id in kids {
                    out.push((id, depth));
                    walk(tree, TreeParentId::Node(id), depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.tree(), TreeParentId::Root, 0, &mut out);
        out
    }

    /// Block ids in document order (pre-order DFS).
    pub fn block_ids(&self) -> Vec<BlockId> {
        self.blocks().into_iter().map(|(id, _)| id).collect()
    }

    /// Top-level block count (not the DFS total).
    pub fn len(&self) -> usize {
        self.tree().children(TreeParentId::Root).map_or(0, |c| c.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn index_of(&self, id: BlockId) -> Option<usize> {
        self.block_ids().iter().position(|&x| x == id)
    }

    // After a delete the node lingers in the oplog — use this to distinguish "empty" from "gone"
    pub fn contains(&self, id: BlockId) -> bool {
        matches!(self.tree().is_node_deleted(&id), Ok(false))
    }

    /// Nesting depth: 0 at top level, +1 per ancestor.
    pub fn depth(&self, id: BlockId) -> usize {
        let tree = self.tree();
        let mut depth = 0;
        let mut cur = id;
        while let Some(TreeParentId::Node(p)) = tree.parent(cur) {
            depth += 1;
            cur = p;
        }
        depth
    }

    pub fn parent_of(&self, id: BlockId) -> Option<BlockId> {
        match self.tree().parent(id) {
            Some(TreeParentId::Node(p)) => Some(p),
            _ => None,
        }
    }

    pub fn children(&self, id: BlockId) -> Vec<BlockId> {
        self.tree().children(TreeParentId::Node(id)).unwrap_or_default()
    }

    /// The block immediately before `id` among its siblings (same parent), if any.
    pub fn prev_sibling(&self, id: BlockId) -> Option<BlockId> {
        let tree = self.tree();
        let siblings = tree.children(tree.parent(id)?)?;
        let pos = siblings.iter().position(|&x| x == id)?;
        pos.checked_sub(1).map(|i| siblings[i])
    }

    pub fn kind(&self, id: BlockId) -> String {
        match self.meta_value(id, "kind") {
            Some(LoroValue::String(s)) => s.to_string(),
            _ => String::new(),
        }
    }

    pub fn text(&self, id: BlockId) -> String {
        self.content(id).to_string()
    }

    pub fn text_len(&self, id: BlockId) -> usize {
        self.content(id).len_unicode()
    }

    /// Capture a *stable* cursor anchored at code-point `pos` in block `id`'s text. Unlike a
    /// bare offset, it tracks the character it sits beside by op-id, so a concurrent remote
    /// edit that inserts/deletes text earlier in the block carries it along instead of leaving
    /// it stranded at a now-wrong offset. Resolve it later with [`resolve_cursor`](Self::resolve_cursor).
    pub fn cursor_at(&self, id: BlockId, pos: usize) -> Option<Cursor> {
        // Side::Left: a remote insertion at exactly `pos` lands to the cursor's right (the
        // caret stays before it), the conventional text-caret bias.
        self.content(id).get_cursor(pos, Side::Left)
    }

    /// Resolve a cursor from [`cursor_at`](Self::cursor_at) to a live code-point offset in the
    /// current document state. `None` if it can't be located (e.g. its block was deleted).
    pub fn resolve_cursor(&self, cursor: &Cursor) -> Option<usize> {
        self.doc.get_cursor_pos(cursor).ok().map(|r| r.current.pos)
    }

    pub fn meta(&self, id: BlockId, key: &str) -> Option<String> {
        match self.meta_value(id, key) {
            Some(LoroValue::String(s)) => Some(s.to_string()),
            _ => None,
        }
    }

    pub fn meta_bool(&self, id: BlockId, key: &str) -> Option<bool> {
        match self.meta_value(id, key) {
            Some(LoroValue::Bool(b)) => Some(b),
            _ => None,
        }
    }

    pub fn set_kind(&self, id: BlockId, kind: &str) {
        self.meta_map(id).insert("kind", kind).expect("set kind");
    }

    pub fn set_meta(&self, id: BlockId, key: &str, value: &str) {
        self.meta_map(id).insert(key, value).expect("set meta");
    }

    pub fn set_meta_bool(&self, id: BlockId, key: &str, value: bool) {
        self.meta_map(id).insert(key, value).expect("set meta bool");
    }

    pub fn set_block_text(&self, id: BlockId, text: &str) {
        let len = self.text_len(id);
        self.delete_text(id, 0, len);
        self.insert_text(id, 0, text);
    }

    pub fn insert_text(&self, id: BlockId, at: usize, s: &str) {
        self.content(id).insert(at, s).expect("insert text");
    }

    pub fn delete_text(&self, id: BlockId, at: usize, len: usize) {
        if len > 0 {
            self.content(id).delete(at, len).expect("delete text");
        }
    }

    pub fn push(&self, kind: &str, text: &str) -> BlockId {
        self.insert_at(self.len(), kind, text)
    }

    /// Create a block at sibling `index` among the top-level blocks.
    pub fn insert_at(&self, index: usize, kind: &str, text: &str) -> BlockId {
        let index = index.min(self.len());
        let id = self.tree().create_at(TreeParentId::Root, index).expect("create");
        self.init_block(id, kind, text);
        id
    }

    /// Create a block as the sibling immediately after `sibling` (same parent, same depth).
    pub fn insert_after(&self, sibling: BlockId, kind: &str, text: &str) -> BlockId {
        let tree = self.tree();
        let parent = tree.parent(sibling).unwrap_or(TreeParentId::Root);
        let index = tree
            .children(parent)
            .and_then(|sibs| sibs.iter().position(|&x| x == sibling))
            .map_or(0, |i| i + 1);
        let id = tree.create_at(parent, index).expect("create");
        self.init_block(id, kind, text);
        id
    }

    /// Delete a block, promoting its children into its slot first (loro's `delete` would hide
    /// the whole subtree).
    pub fn delete_block(&self, id: BlockId) {
        let tree = self.tree();
        // Reverse so each mov_after lands ahead of the previously moved one, preserving order
        for &child in tree.children(TreeParentId::Node(id)).unwrap_or_default().iter().rev() {
            tree.mov_after(child, id).expect("promote child");
        }
        tree.delete(id).expect("delete block");
    }

    /// Nest `id` under its previous sibling (Tab). No-op without one.
    pub fn indent(&self, id: BlockId) -> bool {
        let Some(prev) = self.prev_sibling(id) else { return false };
        self.tree().mov(id, prev).expect("indent");
        true
    }

    /// Lift `id` to be the sibling after its parent (Shift-Tab). No-op at top level.
    pub fn outdent(&self, id: BlockId) -> bool {
        let Some(parent) = self.parent_of(id) else { return false };
        self.tree().mov_after(id, parent).expect("outdent");
        true
    }

    /// No-op when `target` is in `block`'s subtree (would make a cycle).
    pub fn move_before(&self, block: BlockId, target: BlockId) -> bool {
        !self.subtree_contains(block, target) && self.tree().mov_before(block, target).is_ok()
    }

    pub fn move_after(&self, block: BlockId, target: BlockId) -> bool {
        !self.subtree_contains(block, target) && self.tree().mov_after(block, target).is_ok()
    }

    /// Move `block` to be the last child of `parent` — the drop-INTO / nest gesture.
    pub fn move_into(&self, block: BlockId, parent: BlockId) -> bool {
        !self.subtree_contains(block, parent) && self.tree().mov(block, parent).is_ok()
    }

    /// The block's content text container — the seam rich layers (marks, deltas) build on.
    pub fn content(&self, id: BlockId) -> LoroText {
        self.meta_map(id).get_or_create_container("content", LoroText::new()).expect("content text")
    }

    /// Whether `node` lies in `root`'s subtree (including `root`).
    fn subtree_contains(&self, root: BlockId, node: BlockId) -> bool {
        let mut cur = node;
        loop {
            if cur == root {
                return true;
            }
            match self.parent_of(cur) {
                Some(p) => cur = p,
                None => return false,
            }
        }
    }

    fn init_block(&self, id: BlockId, kind: &str, text: &str) {
        Self::init_block_raw(&self.tree(), id, kind, text);
    }

    // Eagerly create the content text container before any text op — bundling a lazy
    // nested-container create with a text insert in one undo step breaks Loro's redo
    fn init_block_raw(tree: &LoroTree, id: BlockId, kind: &str, text: &str) {
        let meta = tree.get_meta(id).expect("live node");
        meta.insert("kind", kind).expect("kind");
        let content: LoroText = meta.get_or_create_container("content", LoroText::new()).expect("content");
        if !text.is_empty() {
            content.insert(0, text).expect("seed text");
        }
    }

    fn tree(&self) -> LoroTree {
        self.doc.get_tree(self.tree.as_str())
    }

    fn meta_map(&self, id: BlockId) -> loro::LoroMap {
        self.tree().get_meta(id).expect("live node")
    }

    fn meta_value(&self, id: BlockId, key: &str) -> Option<LoroValue> {
        match self.meta_map(id).get(key) {
            Some(ValueOrContainer::Value(v)) => Some(v),
            _ => None,
        }
    }
}

impl Default for BlockDoc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
