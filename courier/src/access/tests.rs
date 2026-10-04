use std::collections::BTreeSet;

use loro::{ExportMode, LoroDoc};
use manifest::Manifest;

use super::*;
use crate::CourierError;

const WS: &str = "ws1";
const ITEM: &str = "item1";
const ALICE: &str = "did:key:z6MkAlice";
const BOB: &str = "did:key:z6MkBob";

const CHAT: &str = r#"app "chat" {
  roles admin, moderator, member
  role admin { grant moderator, member }
  doc chat               { read member  write admin }
  doc group/{gid}/meta   { read members(group/{gid}/meta)  write member }
  doc dm/{a}/{b}         { read a, b  write a, b }
  doc user/{did}         { read did  write did }
}"#;

fn chat() -> Manifest {
    Manifest::parse(CHAT).unwrap()
}

fn grant(role: &str, scope: Scope) -> Grant {
    Grant {
        id: [0; 32],
        role: role.to_string(),
        scope,
    }
}

fn app() -> Scope {
    Scope::App {
        ws: WS.to_string(),
        app: ITEM.to_string(),
    }
}

fn resource(path: &str) -> Scope {
    Scope::Resource(format!("ws/{WS}/{ITEM}/{path}"))
}

/// `who`'s access to `doc` holding `grants`, where `members` is what the node's copies list.
fn access(
    m: &Manifest,
    doc: &str,
    who: &str,
    grants: &[Grant],
    members: &[(&str, &[&str])],
) -> std::result::Result<Access, CourierError> {
    let roles = roles_at(grants, WS, ITEM, doc);
    doc_access(m, doc, who, &roles, &mut |name: &str| {
        Ok::<_, CourierError>(
            members
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, l)| l.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default(),
        )
    })
}

const NONE: Access = Access {
    read: false,
    write: false,
};
const READ: Access = Access {
    read: true,
    write: false,
};

#[test]
fn an_app_that_declares_no_docs_leaves_every_doc_to_membership() {
    let bare = Manifest::parse(r#"app "x" { roles admin }"#).unwrap();
    assert_eq!(access(&bare, "anything/at/all", BOB, &[], &[]), Ok(Access::OPEN));
}

#[test]
fn an_undeclared_doc_is_refused() {
    let member = [grant("member", Scope::Node)];
    assert_eq!(
        access(&chat(), "smuggled/x", BOB, &member, &[]),
        Err(CourierError::Undeclared)
    );
}

#[test]
fn a_role_rule_admits_that_role_and_every_role_above_it() {
    let m = chat();
    let node_member = [grant("member", Scope::Node)];
    assert_eq!(access(&m, "chat", BOB, &node_member, &[]), Ok(READ));
    let ws_member = [grant("member", Scope::Workspace(WS.to_string()))];
    assert_eq!(access(&m, "chat", BOB, &ws_member, &[]), Ok(READ));
    let app_admin = [grant("admin", app())];
    assert_eq!(access(&m, "chat", BOB, &app_admin, &[]), Ok(Access::OPEN));
    // Platform owners and maintainers are the manifest's implicit owner.
    let ws_maintainer = [grant("maintainer", Scope::Workspace(WS.to_string()))];
    assert_eq!(access(&m, "chat", BOB, &ws_maintainer, &[]), Ok(Access::OPEN));
    let node_owner = [grant("owner", Scope::Node)];
    assert_eq!(access(&m, "chat", ALICE, &node_owner, &[]), Ok(Access::OPEN));
}

#[test]
fn a_grant_elsewhere_or_a_platform_guest_holds_no_app_role() {
    let m = chat();
    let other_ws = [grant("owner", Scope::Workspace("ws2".to_string()))];
    assert_eq!(access(&m, "chat", BOB, &other_ws, &[]), Ok(NONE));
    let other_app = [grant(
        "admin",
        Scope::App {
            ws: WS.to_string(),
            app: "item2".to_string(),
        },
    )];
    assert_eq!(access(&m, "chat", BOB, &other_app, &[]), Ok(NONE));
    let guest = [grant("guest", Scope::Node)];
    assert_eq!(access(&m, "chat", BOB, &guest, &[]), Ok(NONE));
}

#[test]
fn a_did_variable_admits_that_did_and_no_role_overrides_it() {
    let m = chat();
    let owner = [grant("owner", Scope::Node)];
    let mine = format!("user/{ALICE}");
    assert_eq!(access(&m, &mine, ALICE, &[], &[]), Ok(Access::OPEN));
    assert_eq!(access(&m, &mine, BOB, &owner, &[]), Ok(NONE));
    let dm = format!("dm/{ALICE}/{BOB}");
    assert_eq!(access(&m, &dm, BOB, &[], &[]), Ok(Access::OPEN));
    assert_eq!(access(&m, &dm, "did:key:z6MkCarol", &owner, &[]), Ok(NONE));
}

#[test]
fn members_of_a_doc_admits_whoever_the_nodes_copy_lists() {
    let m = chat();
    let member = [grant("member", Scope::Node)];
    let listed: &[(&str, &[&str])] = &[("group/g1/meta", &[ALICE])];
    let alice = access(&m, "group/g1/meta", ALICE, &member, listed).unwrap();
    assert_eq!(alice, Access::OPEN);
    let bob = access(&m, "group/g1/meta", BOB, &member, listed).unwrap();
    assert_eq!(bob, Access { read: false, write: true });
    // Nothing on the node lists anyone.
    let nobody = access(&m, "group/g2/meta", ALICE, &member, listed).unwrap();
    assert!(!nobody.read);
}

#[test]
fn a_resource_grant_carries_its_role_to_the_docs_it_covers_only() {
    let m = chat();
    let one = [grant("admin", resource("chat"))];
    assert_eq!(access(&m, "chat", BOB, &one, &[]), Ok(Access::OPEN));
    let mine = format!("user/{ALICE}");
    assert_eq!(access(&m, &mine, BOB, &one, &[]), Ok(NONE));
    let groups = [grant("member", resource("group/*"))];
    assert!(access(&m, "group/g1/meta", BOB, &groups, &[]).unwrap().write);
    assert_eq!(access(&m, "chat", BOB, &groups, &[]), Ok(NONE));
}

fn snapshot(build: impl FnOnce(&LoroDoc)) -> Vec<u8> {
    let doc = LoroDoc::new();
    build(&doc);
    doc.export(ExportMode::Snapshot).unwrap()
}

#[test]
fn members_in_reads_the_dids_a_members_map_sets_true() {
    let doc = snapshot(|d| {
        let m = d.get_map("members");
        m.insert(ALICE, true).unwrap();
        m.insert(BOB, false).unwrap();
        m.insert("did:key:z6MkCarol", "yes").unwrap();
    });
    assert_eq!(
        members_in(Some(&doc)).unwrap(),
        BTreeSet::from([ALICE.to_string()])
    );
    assert!(members_in(None).unwrap().is_empty());
    let list = snapshot(|d| d.get_list("members").push(ALICE).unwrap());
    assert!(members_in(Some(&list)).unwrap().is_empty());
    assert_eq!(members_in(Some(b"junk")), Err(CourierError::Decode));
}

/// A storable doc name always has an address, so a resource grant can reach any doc.
#[test]
fn every_valid_doc_name_has_an_address_and_no_other_does() {
    for name in [
        "chat",
        "group/g1/meta",
        "dm/did:key:z6MkA/did:key:z6MkB/2026-10-04",
        "a_b-c",
        "a.b",
        "a/../b",
        "a/./b",
        "a//b",
        "/a",
        "a/",
        "a b",
        "é",
        "",
    ] {
        assert_eq!(
            workspace::valid_doc_name(name),
            doc_address(WS, ITEM, name).is_some(),
            "{name:?}"
        );
    }
}
