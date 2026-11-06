use loro::{ContainerTrait, ExportMode, LoroDoc, LoroMap, LoroText, PeerID, TreeID, TreeParentId};

use super::{Kind, Op, read};
use crate::CourierError;

const ALICE: PeerID = 1;
const BOB: PeerID = 2;

fn doc(peer: PeerID) -> LoroDoc {
    let doc = LoroDoc::new();
    doc.set_peer_id(peer).unwrap();
    doc
}

/// What `who` would push after `edit` on their copy of `live`.
fn change(live: &LoroDoc, who: PeerID, edit: impl FnOnce(&LoroDoc)) -> Vec<u8> {
    let mine = live.fork();
    mine.set_peer_id(who).unwrap();
    let since = mine.oplog_vv();
    edit(&mine);
    mine.commit();
    mine.export(ExportMode::updates(&since)).unwrap()
}

/// Bob's ops as `(kind, path, who it touches)`.
fn ops(live: &LoroDoc, update: &[u8]) -> Vec<(Kind, String, Option<PeerID>)> {
    read(live, update)
        .unwrap()
        .ops
        .into_iter()
        .map(|Op { author, kind, path, touches, .. }| {
            assert_eq!(author, BOB);
            (kind, path.join("/"), touches)
        })
        .collect()
}

fn node(id: TreeID) -> String {
    format!("{}@{}", id.peer, id.counter)
}

/// Alice's doc: a map of cards, one card `c1` with a title and a text body.
fn alices_cards() -> LoroDoc {
    let live = doc(ALICE);
    let cards = live.get_map("cards");
    let c1 = cards.insert_container("c1", LoroMap::new()).unwrap();
    c1.insert("title", "alice's").unwrap();
    c1.insert_container("body", LoroText::new()).unwrap();
    live.commit();
    live
}

#[test]
fn a_map_insert_is_new_an_overwrite_and_a_delete_name_the_keys_creator() {
    let live = alices_cards();
    let update = change(&live, BOB, |d| {
        let cards = d.get_map("cards");
        cards.insert_container("c2", LoroMap::new()).unwrap();
        cards.insert("c1", "gone").unwrap();
    });
    assert_eq!(
        ops(&live, &update),
        [
            (Kind::Insert, "cards/c2".into(), None),
            (Kind::Set, "cards/c1".into(), Some(ALICE)),
        ]
    );

    let update = change(&live, BOB, |d| d.get_map("cards").delete("c1").unwrap());
    assert_eq!(ops(&live, &update), [(Kind::Delete, "cards/c1".into(), Some(ALICE))]);
}

/// An edit inside someone's entry touches that entry: the child container names its creator.
#[test]
fn an_edit_inside_an_entry_touches_the_entrys_creator() {
    let live = alices_cards();
    let c1 = live.get_map("cards").get("c1").unwrap().into_container().unwrap().into_map().unwrap();
    let update = change(&live, BOB, |d| {
        let c1 = d.get_map(c1.id());
        c1.insert("title", "bob's").unwrap();
        c1.insert("edited", true).unwrap();
        d.get_text(c1.get("body").unwrap().into_container().unwrap().id())
            .insert(0, "hi")
            .unwrap();
    });
    assert_eq!(
        ops(&live, &update),
        [
            (Kind::Set, "cards/c1/title".into(), Some(ALICE)),
            (Kind::Insert, "cards/c1/edited".into(), Some(ALICE)),
            (Kind::Text, "cards/c1/body".into(), Some(ALICE)),
        ]
    );
}

#[test]
fn a_list_delete_names_the_elements_creator_and_an_insert_touches_nothing() {
    let live = doc(ALICE);
    live.get_list("messages").insert(0, "alice's").unwrap();
    live.commit();
    let update = change(&live, BOB, |d| {
        let list = d.get_list("messages");
        list.insert(1, "bob's").unwrap();
        list.delete(0, 1).unwrap();
    });
    assert_eq!(
        ops(&live, &update),
        [
            (Kind::Insert, "messages/1".into(), None),
            (Kind::Delete, "messages/0".into(), Some(ALICE)),
        ]
    );
}

#[test]
fn a_movable_list_move_set_and_delete_name_the_elements_creator() {
    let live = doc(ALICE);
    let list = live.get_movable_list("todo");
    list.insert(0, "a").unwrap();
    list.insert(1, "b").unwrap();
    live.commit();
    let update = change(&live, BOB, |d| {
        let list = d.get_movable_list("todo");
        list.mov(0, 1).unwrap();
        list.set(0, "B").unwrap();
        list.delete(1, 1).unwrap();
    });
    assert_eq!(
        ops(&live, &update),
        [
            (Kind::Move, "todo/0".into(), Some(ALICE)),
            (Kind::Set, "todo/*".into(), Some(ALICE)),
            (Kind::Delete, "todo/1".into(), Some(ALICE)),
        ]
    );
}

#[test]
fn a_tree_create_is_new_and_a_move_or_delete_names_the_nodes_creator() {
    let live = doc(ALICE);
    let tree = live.get_tree("blocks");
    let root = tree.create(TreeParentId::Root).unwrap();
    let child = tree.create(TreeParentId::Node(root)).unwrap();
    live.commit();
    let update = change(&live, BOB, |d| {
        let tree = d.get_tree("blocks");
        let mine = tree.create(TreeParentId::Root).unwrap();
        tree.mov(child, TreeParentId::Node(mine)).unwrap();
        tree.delete(root).unwrap();
    });
    let got = ops(&live, &update);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].0, Kind::Insert);
    assert!(got[0].1.starts_with("blocks/2@"), "{}", got[0].1);
    assert_eq!(got[0].2, None);
    // Moved under Bob's node: the path says so, the creator is still Alice.
    assert_eq!(got[1].0, Kind::Move);
    assert_eq!(got[1].1, format!("{}/{}", got[0].1, node(child)));
    assert_eq!(got[1].2, Some(ALICE));
    assert_eq!(got[2], (Kind::Delete, format!("blocks/{}", node(root)), Some(ALICE)));
}

/// O8: what cannot be read refuses, and `live` is never touched.
#[test]
fn an_unreadable_update_is_refused_and_the_live_doc_untouched() {
    let live = alices_cards();
    let before = live.oplog_vv();
    assert_eq!(read(&live, b"not loro").unwrap_err(), CourierError::Decode);

    // Bob's second change without his first: Loro would hold it pending and apply it, unread,
    // once the first arrived.
    let bobs = live.fork();
    bobs.set_peer_id(BOB).unwrap();
    bobs.get_map("cards").insert("c2", 1).unwrap();
    bobs.commit();
    let after_first = bobs.oplog_vv();
    bobs.get_map("cards").insert("c3", 2).unwrap();
    bobs.commit();
    let second_only = bobs.export(ExportMode::updates(&after_first)).unwrap();
    assert_eq!(read(&live, &second_only).unwrap_err(), CourierError::Incomplete);
    assert_eq!(live.oplog_vv(), before);
}

/// The scratch copy holds the update; `live` does not.
#[test]
fn the_read_applies_to_a_copy_only() {
    let live = alices_cards();
    let update = change(&live, BOB, |d| d.get_map("cards").delete("c1").unwrap());
    let read = read(&live, &update).unwrap();
    assert!(read.after.get_map("cards").get("c1").is_none());
    assert!(live.get_map("cards").get("c1").is_some());
}

/// The writer's own view: what it just committed reads the same as the push would.
#[test]
fn a_writers_own_changes_read_the_same_as_its_push() {
    let live = alices_cards();
    let bobs = live.fork();
    bobs.set_peer_id(BOB).unwrap();
    let since = bobs.oplog_vv();
    bobs.get_map("cards").delete("c1").unwrap();
    bobs.get_map("cards").insert_container("c2", LoroMap::new()).unwrap();
    bobs.commit();
    let mine = super::read_since(&bobs, &since).unwrap();
    let pushed = read(&live, &bobs.export(ExportMode::updates(&since)).unwrap()).unwrap();
    assert_eq!(mine.ops, pushed.ops);
    assert_eq!(mine.ops.len(), 2);
    assert!(mine.after.get_map("cards").get("c1").is_none());
}
