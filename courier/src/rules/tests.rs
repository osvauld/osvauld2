use std::collections::BTreeSet;

use loro::{ExportMode, LoroDoc, LoroMap, PeerID, TreeParentId};
use manifest::Manifest;

use super::{Actor, check};
use crate::CourierError;
use crate::ops::read;

const ALICE: PeerID = 1;
const BOB: PeerID = 2;

const BOARD: &str = r#"
app "board" {
  roles admin, editor, member
  role admin { grant editor, member }
  doc board/{id} {
    read member  write member
    rule cards/* {
      forbid set, delete, move  unless actor is entry.creator or actor has admin
      forbid set where changed state
             unless state from todo to doing, doing to done, doing to todo
      forbid insert, set where after.title is missing or len after.title above 20
      forbid set unless changed only title, state, body
    }
    own blocks/** override editor
  }
  doc chat/{id} { write member  own messages/* override admin }
  doc sheet/{id} { write member }
}
"#;

fn board() -> Manifest {
    Manifest::parse(BOARD).unwrap()
}

fn actor(peer: PeerID, roles: &[&str]) -> Actor {
    Actor {
        peer,
        roles: roles.iter().map(|r| r.to_string()).collect::<BTreeSet<_>>(),
    }
}

fn doc(peer: PeerID) -> LoroDoc {
    let doc = LoroDoc::new();
    doc.set_peer_id(peer).unwrap();
    doc
}

fn change(live: &LoroDoc, who: PeerID, edit: impl FnOnce(&LoroDoc)) -> Vec<u8> {
    let mine = live.fork();
    mine.set_peer_id(who).unwrap();
    let since = mine.oplog_vv();
    edit(&mine);
    mine.commit();
    mine.export(ExportMode::updates(&since)).unwrap()
}

/// The verdict on `who`'s `edit` of `live` under `doc_name`'s rules.
fn judge(doc_name: &str, live: &LoroDoc, who: &Actor, edit: impl FnOnce(&LoroDoc)) -> Result<(), String> {
    let m = board();
    let (decl, _) = m.resolve(doc_name).unwrap();
    let update = change(live, who.peer, edit);
    let read = read(live, &update).unwrap();
    check(&m, decl, who, live, &read).map_err(|e| match e {
        CourierError::Forbidden(why) => why,
        other => panic!("{other:?}"),
    })
}

fn alices_board() -> LoroDoc {
    let live = doc(ALICE);
    let c1 = live.get_map("cards").insert_container("c1", LoroMap::new()).unwrap();
    c1.insert("title", "alice's").unwrap();
    c1.insert("state", "todo").unwrap();
    live.commit();
    live
}

fn card(d: &LoroDoc, id: &str, title: &str) {
    let c = d.get_map("cards").insert_container(id, LoroMap::new()).unwrap();
    c.insert("title", title).unwrap();
    c.insert("state", "todo").unwrap();
}

#[test]
fn only_the_creator_or_an_admin_changes_or_deletes_a_card() {
    let live = alices_board();
    let bob = actor(BOB, &["member"]);
    let alice = actor(ALICE, &["member"]);
    let admin = actor(BOB, &["admin"]);

    let delete = |d: &LoroDoc| d.get_map("cards").delete("c1").unwrap();
    assert_eq!(judge("board/1", &live, &bob, delete).unwrap_err(), "manifest line 8: may not delete cards/c1");
    judge("board/1", &live, &alice, delete).unwrap();
    judge("board/1", &live, &admin, delete).unwrap();

    let retitle = |d: &LoroDoc| {
        let c1 = d.get_by_str_path("cards/c1").unwrap().into_container().unwrap().into_map().unwrap();
        c1.insert("title", "bob's").unwrap();
    };
    assert!(judge("board/1", &live, &bob, retitle).unwrap_err().contains("may not change cards/c1/title"));
    judge("board/1", &live, &alice, retitle).unwrap();

    // Anyone may add a card; it is theirs from then on.
    judge("board/1", &live, &bob, |d| card(d, "c2", "bob's")).unwrap();
}

#[test]
fn a_state_moves_only_along_the_declared_transitions() {
    let live = alices_board();
    let alice = actor(ALICE, &["member"]);
    let set_state = |to: &'static str| {
        move |d: &LoroDoc| {
            let c1 = d.get_by_str_path("cards/c1").unwrap().into_container().unwrap().into_map().unwrap();
            c1.insert("state", to).unwrap();
        }
    };
    judge("board/1", &live, &alice, set_state("doing")).unwrap();
    assert_eq!(
        judge("board/1", &live, &alice, set_state("done")).unwrap_err(),
        "manifest line 9: may not change cards/c1/state"
    );
    // Not a transition at all: the state stays, so the rule is not reached.
    judge("board/1", &live, &alice, set_state("todo")).unwrap();
}

#[test]
fn field_checks_count_unicode_scalars_and_an_error_refuses() {
    let live = alices_board();
    let bob = actor(BOB, &["member"]);
    assert_eq!(
        judge("board/1", &live, &bob, |d| {
            d.get_map("cards").insert_container("c2", LoroMap::new()).unwrap();
        })
        .unwrap_err(),
        "manifest line 11: may not insert cards/c2"
    );
    judge("board/1", &live, &bob, |d| card(d, "c2", &"é".repeat(20))).unwrap();
    assert!(judge("board/1", &live, &bob, |d| card(d, "c2", &"é".repeat(21))).is_err());
    // A title that is not text: `len` cannot be taken, and a rule that errors refuses.
    assert!(
        judge("board/1", &live, &bob, |d| {
            d.get_map("cards").insert_container("c2", LoroMap::new()).unwrap().insert("title", 7).unwrap();
        })
        .is_err()
    );
}

#[test]
fn only_the_listed_fields_may_change() {
    let live = alices_board();
    let alice = actor(ALICE, &["member"]);
    let paint = |d: &LoroDoc| {
        let c1 = d.get_by_str_path("cards/c1").unwrap().into_container().unwrap().into_map().unwrap();
        c1.insert("color", "red").unwrap();
    };
    assert_eq!(judge("board/1", &live, &alice, paint).unwrap_err(), "manifest line 12: may not change cards/c1/color");
}

#[test]
fn a_doc_without_rules_admits_whatever_write_allows() {
    let live = alices_board();
    let bob = actor(BOB, &["member"]);
    judge("sheet/1", &live, &bob, |d| d.get_map("cards").delete("c1").unwrap()).unwrap();
}

#[test]
fn owned_tree_nodes_at_any_depth_with_an_override() {
    let live = doc(ALICE);
    let tree = live.get_tree("blocks");
    let top = tree.create(TreeParentId::Root).unwrap();
    let deep = tree.create(TreeParentId::Node(top)).unwrap();
    live.commit();
    let bob = actor(BOB, &["member"]);
    let editor = actor(BOB, &["editor"]);
    let mov = |d: &LoroDoc| {
        let tree = d.get_tree("blocks");
        let mine = tree.create(TreeParentId::Root).unwrap();
        tree.mov(deep, TreeParentId::Node(mine)).unwrap();
    };
    assert!(judge("board/1", &live, &bob, mov).unwrap_err().contains("may not move blocks/"));
    judge("board/1", &live, &editor, mov).unwrap();
    judge("board/1", &live, &bob, |d| {
        d.get_tree("blocks").create(TreeParentId::Root).unwrap();
    })
    .unwrap();
}

/// The chat's shape: messages in a list, each a map; `own messages/*` reaches the element
/// and the fields inside it.
#[test]
fn owned_list_elements_and_their_fields() {
    let live = doc(ALICE);
    let m = live.get_movable_list("messages").insert_container(0, LoroMap::new()).unwrap();
    m.insert("text", "hello").unwrap();
    live.commit();
    let bob = actor(BOB, &["member"]);
    let alice = actor(ALICE, &["member"]);
    let admin = actor(BOB, &["admin"]);

    let delete = |d: &LoroDoc| d.get_movable_list("messages").delete(0, 1).unwrap();
    assert_eq!(judge("chat/1", &live, &bob, delete).unwrap_err(), "manifest line 16: may not delete messages/0");
    judge("chat/1", &live, &alice, delete).unwrap();
    judge("chat/1", &live, &admin, delete).unwrap();

    let edit = |d: &LoroDoc| {
        let m = d.get_by_str_path("messages/0").unwrap().into_container().unwrap().into_map().unwrap();
        m.insert("text", "edited").unwrap();
    };
    assert_eq!(judge("chat/1", &live, &bob, edit).unwrap_err(), "manifest line 16: may not change messages/0/text");
    judge("chat/1", &live, &alice, edit).unwrap();

    judge("chat/1", &live, &bob, |d| {
        d.get_movable_list("messages").insert_container(1, LoroMap::new()).unwrap().insert("text", "mine").unwrap();
    })
    .unwrap();
}
