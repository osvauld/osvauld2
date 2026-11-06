//! An update read op by op before it merges (`owned-entries.md` §2.2): what each op does,
//! which device's change carries it, and who created the thing it touches. The last comes
//! off Loro's own IDs — a deleted or moved element, a tree node and a child container all
//! carry their creator's peer — so an app's `author` field is never consulted. A map key
//! names no ID, so its prior value is read off `live` before the import.
//!
//! osvauld1 diffed an update into an empty scratch doc, which made deletes invisible, and on
//! a decode error carried on with no ops (`osvauld1-prior-art.md`). Here a delete names its
//! target, and anything the reader cannot read refuses the whole update.

use loro::{
    ContainerID, ContainerTrait, Frontiers, ID, Index, JsonChange, JsonListOp, JsonMapOp,
    JsonMovableListOp, JsonOp, JsonOpContent, JsonTextOp, JsonTreeOp, LoroDoc, PeerID, TreeID,
    TreeParentId, ValueOrContainer, VersionVector,
};

use crate::{CourierError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A new entry: a list element, a tree node, a key the map did not hold.
    Insert,
    /// An existing entry replaced: a key overwritten, a movable-list element set.
    Set,
    Delete,
    Move,
    /// An edit inside a text container.
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    /// The device whose change carries this op.
    pub author: PeerID,
    /// The container the op lands in.
    pub container: ContainerID,
    pub kind: Kind,
    /// Where the op lands, root name first: a map key, a list position, a tree node as
    /// `peer@counter`, down to the entry the op is about (`cards/c1/title`). A movable-list
    /// `set` names its element by ID only, so its position reads `*`.
    pub path: Vec<String>,
    /// Who created what this op changes: the deleted or moved element, the overwritten key's
    /// value, the child container an edit lands in. `None` when nothing existing is touched —
    /// a new entry in a root container.
    pub touches: Option<PeerID>,
}

/// The update applied to a scratch copy of `live`, and its ops. `live` is untouched; the
/// rules read "before" off it and "after" off the copy.
#[derive(Debug)]
pub struct Read {
    pub after: LoroDoc,
    pub ops: Vec<Op>,
}

/// Refuses whatever it cannot read: a blob that does not decode, an op kind this version
/// does not know, and an update whose causal dependencies `live` lacks (`Incomplete`) — Loro
/// would hold those changes pending and apply them, unread, when their dependencies arrive.
pub fn read(live: &LoroDoc, update: &[u8]) -> Result<Read> {
    let after = live.fork();
    let start = after.oplog_vv();
    let status = after.import(update).map_err(|_| CourierError::Decode)?;
    if status.pending.is_some_and(|p| !p.is_empty()) {
        return Err(CourierError::Incomplete);
    }
    let end = after.oplog_vv();
    let json = after.export_json_updates_without_peer_compression(&start, &end);
    let mut ops = Vec::new();
    for change in &json.changes {
        for op in &change.ops {
            classify(live, &after, change, op, &mut ops)?;
        }
    }
    Ok(Read { after, ops })
}

/// The doc's own changes since `since`, read the same way: for the writer's shell, which
/// applies a Lua write to a scratch copy, reads it, judges it, and only then writes for real
/// (`owned-entries.md` §2.4, site 1).
pub fn read_since(doc: &LoroDoc, since: &VersionVector) -> Result<Read> {
    let before = doc
        .fork_at(&doc.vv_to_frontiers(since))
        .map_err(|_| CourierError::Decode)?;
    let after = doc.fork();
    let json = doc.export_json_updates_without_peer_compression(since, &doc.oplog_vv());
    let mut ops = Vec::new();
    for change in &json.changes {
        for op in &change.ops {
            classify(&before, &after, change, op, &mut ops)?;
        }
    }
    Ok(Read { after, ops })
}

/// One op becomes one `Op`, except a list delete, which becomes one per deleted element —
/// a span can cross elements of different creators.
fn classify(
    before: &LoroDoc,
    after: &LoroDoc,
    change: &JsonChange,
    op: &JsonOp,
    out: &mut Vec<Op>,
) -> Result<()> {
    let author = change.id.peer;
    let container = op.container.clone();
    let within = creator_of(&container);
    // A container the update created is in `after` only; one it deleted, in `before` only.
    let base = container_path(after, &container).or_else(|_| container_path(before, &container))?;
    let at = |slot: String| {
        let mut path = base.clone();
        path.push(slot);
        path
    };
    let mut push = |kind, path: Vec<String>, touches| {
        out.push(Op {
            author,
            container: container.clone(),
            kind,
            path,
            touches,
        })
    };
    let (kind, path, touches) = match &op.content {
        JsonOpContent::List(JsonListOp::Insert { pos, .. })
        | JsonOpContent::MovableList(JsonMovableListOp::Insert { pos, .. }) => {
            (Kind::Insert, at(pos.to_string()), within)
        }
        JsonOpContent::Tree(JsonTreeOp::Create { target, .. }) => {
            (Kind::Insert, tree_path(after, &base, &container, *target), within)
        }
        // A delete names the slot, not the element: a movable list's move re-makes the slot
        // in the mover's name, and a span may cross inserts. The elements are read off the
        // list as the author saw it, by position.
        JsonOpContent::List(JsonListOp::Delete { pos, len, .. }) => {
            let seen = just_before(after, change, op)?;
            let list = seen.get_list(container.clone());
            for i in span(*pos, *len) {
                let id = list.get_id_at(i).ok_or(CourierError::Decode)?;
                push(Kind::Delete, at(i.to_string()), Some(id.peer));
            }
            return Ok(());
        }
        JsonOpContent::MovableList(JsonMovableListOp::Delete { pos, len, .. }) => {
            let seen = just_before(after, change, op)?;
            let list = seen.get_movable_list(container.clone());
            for i in span(*pos, *len) {
                let creator = list.get_creator_at(i).ok_or(CourierError::Decode)?;
                push(Kind::Delete, at(i.to_string()), Some(creator));
            }
            return Ok(());
        }
        JsonOpContent::MovableList(JsonMovableListOp::Move { from, elem_id, .. }) => {
            (Kind::Move, at(from.to_string()), Some(elem_id.peer))
        }
        JsonOpContent::MovableList(JsonMovableListOp::Set { elem_id, .. }) => {
            (Kind::Set, at("*".to_string()), Some(elem_id.peer))
        }
        JsonOpContent::Map(JsonMapOp::Insert { key, .. }) => match key_creator(before, &container, key) {
            Some(prior) => (Kind::Set, at(key.clone()), Some(prior)),
            None => (Kind::Insert, at(key.clone()), within),
        },
        JsonOpContent::Map(JsonMapOp::Delete { key }) => (
            Kind::Delete,
            at(key.clone()),
            key_creator(before, &container, key).or(within),
        ),
        JsonOpContent::Tree(JsonTreeOp::Move { target, .. }) => (
            Kind::Move,
            tree_path(after, &base, &container, *target),
            Some(target.peer),
        ),
        JsonOpContent::Tree(JsonTreeOp::Delete { target }) => (
            Kind::Delete,
            tree_path(before, &base, &container, *target),
            Some(target.peer),
        ),
        JsonOpContent::Text(
            JsonTextOp::Insert { .. }
            | JsonTextOp::Delete { .. }
            | JsonTextOp::Mark { .. }
            | JsonTextOp::MarkEnd,
        ) => (Kind::Text, base.clone(), within),
        JsonOpContent::Future(_) => return Err(CourierError::Decode),
    };
    push(kind, path, touches);
    Ok(())
}

/// A container's place in the doc, root name first.
fn container_path(doc: &LoroDoc, id: &ContainerID) -> Result<Vec<String>> {
    if let ContainerID::Root { name, .. } = id {
        return Ok(vec![name.to_string()]);
    }
    let steps = doc.get_path_to_container(id).ok_or(CourierError::Decode)?;
    Ok(steps.iter().map(|(_, index)| segment(index)).collect())
}

fn segment(index: &Index) -> String {
    match index {
        Index::Key(k) => k.to_string(),
        Index::Seq(i) => i.to_string(),
        Index::Node(id) => node(*id),
    }
}

fn node(id: TreeID) -> String {
    format!("{}@{}", id.peer, id.counter)
}

/// A tree node's place: the tree, its ancestors, itself — so `blocks/*` reaches the top
/// level and `blocks/**` every depth.
fn tree_path(doc: &LoroDoc, base: &[String], tree: &ContainerID, target: TreeID) -> Vec<String> {
    let tree = doc.get_tree(tree.clone());
    let mut chain = vec![target];
    let mut cur = target;
    while let Some(TreeParentId::Node(parent)) = tree.parent(cur) {
        chain.push(parent);
        cur = parent;
    }
    let mut path = base.to_vec();
    path.extend(chain.into_iter().rev().map(node));
    path
}

/// The positions a delete of `len` from `pos` covers; a negative `len` runs backwards.
fn span(pos: i32, len: i32) -> std::ops::Range<usize> {
    let (start, count) = if len < 0 { (pos + len + 1, -len) } else { (pos, len) };
    start.max(0) as usize..(start + count).max(0) as usize
}

/// The doc as the op's author saw it — the op's causal past, which its position indexes:
/// after the previous op of the same change, or at the change's dependencies.
fn just_before(after: &LoroDoc, change: &JsonChange, op: &JsonOp) -> Result<LoroDoc> {
    let frontiers = if op.counter > change.id.counter {
        Frontiers::from(ID::new(change.id.peer, op.counter - 1))
    } else {
        Frontiers::from(change.deps.as_slice())
    };
    after.fork_at(&frontiers).map_err(|_| CourierError::Decode)
}

/// A child container's creator; a root container has none.
fn creator_of(id: &ContainerID) -> Option<PeerID> {
    match id {
        ContainerID::Root { .. } => None,
        ContainerID::Normal { peer, .. } => Some(*peer),
    }
}

/// Who created what `key` held before this update: a child container's creator, or for a
/// plain value its last editor — the same device as long as the rules have held.
fn key_creator(before: &LoroDoc, container: &ContainerID, key: &str) -> Option<PeerID> {
    if !before.has_container(container) {
        return None;
    }
    let map = before.get_map(container.clone());
    match map.get(key)? {
        ValueOrContainer::Container(child) => creator_of(&child.id()),
        ValueOrContainer::Value(_) => map.get_last_editor(key),
    }
}

#[cfg(test)]
mod tests;
