//! The manifest's read and write rules on the node — step 5 of
//! `docs/design/group-chat-sync.md`: T8, T10, T15 and T20's node halves.

use courier::CourierError;
use courier::subscribe::desktop_start_subscribe;
use courier::sync::{SyncAck, SyncLayer, desktop_start_sync};
use courier::token::{Scope, Token, issue_root};
use identity::Identity;
use loro::LoroDoc;

use super::roles_tests::{Fixture, NOW, fixture_with, minted, refused};
use super::*;
use crate::push::MockPusher;

const CHAT: &str = r#"app "chat" {
  roles admin, moderator, member
  role admin { grant moderator, member }
  doc chat              { read member  write admin }
  doc group/{gid}/meta  { read members(group/{gid}/meta)  write member }
  doc group/{gid}/{day} { shard by day  read members(group/{gid}/meta)  write members(group/{gid}/meta) }
  doc dm/{a}/{b}/{day}  { shard by day  read a, b  write a, b }
  doc user/{did}        { read did  write did }
}"#;

const DAY: &str = "2026-10-04";

fn text(s: &str) -> LoroDoc {
    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, s).unwrap();
    doc
}

fn group(members: &[&str]) -> LoroDoc {
    let doc = LoroDoc::new();
    let map = doc.get_map("members");
    for did in members {
        map.insert(did, true).unwrap();
    }
    doc
}

impl Fixture {
    fn sync_on(
        &self,
        item: &str,
        who: &Identity,
        token: &Token,
        name: &str,
        doc: &LoroDoc,
        pusher: &MockPusher,
    ) -> Result<SyncAck, NodeError> {
        let layer = SyncLayer::Doc(name.to_string());
        let hello =
            desktop_start_sync(who.did(), token.clone(), &self.ws, item, layer, doc, None).unwrap();
        self.admin.accept_sync(hello, NOW, pusher)
    }

    fn sync(
        &self,
        who: &Identity,
        token: &Token,
        name: &str,
        doc: &LoroDoc,
    ) -> Result<SyncAck, NodeError> {
        self.sync_on(&self.item, who, token, name, doc, &MockPusher::new())
    }

    fn subscribe(&self, who: &Identity, token: &Token, name: &str) {
        let layer = SyncLayer::Doc(name.to_string());
        let hello = desktop_start_subscribe(who.did(), token.clone(), &self.ws, &self.item, layer);
        self.admin.subscribe(&hello, NOW).unwrap();
    }

    fn stored(&self, name: &str) -> Option<LoroDoc> {
        let bytes = self.vault.get_doc(&self.ws, &self.item, name).unwrap()?;
        let doc = LoroDoc::new();
        doc.import(&bytes).unwrap();
        Some(doc)
    }

    fn stored_text(&self, name: &str) -> String {
        self.stored(name).unwrap().get_text("t").to_string()
    }
}

#[test]
fn a_member_may_read_an_admin_doc_but_not_change_it() {
    let f = fixture_with(CHAT);
    let (bob, bob_token) = f.member();
    f.sync(&f.alice, &f.alice_token, "chat", &text("general"))
        .unwrap();

    let ack = f.sync(&bob, &bob_token, "chat", &LoroDoc::new()).unwrap();
    let mine = LoroDoc::new();
    mine.import(&ack.update).unwrap();
    assert_eq!(mine.get_text("t").to_string(), "general");

    mine.get_text("t").insert(0, "bobs-room ").unwrap();
    refused(f.sync(&bob, &bob_token, "chat", &mine), CourierError::NoWrite);
    assert_eq!(f.stored_text("chat"), "general");
}

#[test]
fn nobody_writes_under_another_users_did() {
    let f = fixture_with(CHAT);
    let (bob, bob_token) = f.member();
    let alices = format!("user/{}", f.alice.did());
    f.sync(&f.alice, &f.alice_token, &alices, &text("alice"))
        .unwrap();
    refused(
        f.sync(&bob, &bob_token, &alices, &text("mallory")),
        CourierError::NoRead,
    );
    assert_eq!(f.stored_text(&alices), "alice");

    // Nor may bob create one for someone who hasn't yet.
    let (carol, _) = f.member();
    let carols = format!("user/{}", carol.did());
    refused(
        f.sync(&bob, &bob_token, &carols, &text("mallory")),
        CourierError::NoWrite,
    );
    assert!(f.stored(&carols).is_none());
}

#[test]
fn a_dm_reaches_its_two_people_only_asked_for_or_pushed() {
    let f = fixture_with(CHAT);
    let (bob, bob_token) = f.member();
    let (carol, carol_token) = f.member();
    let dm = format!("dm/{}/{}/{DAY}", f.alice.did(), bob.did());
    f.subscribe(&bob, &bob_token, &dm);
    f.subscribe(&carol, &carol_token, &dm);

    let pusher = MockPusher::new();
    f.sync_on(&f.item, &f.alice, &f.alice_token, &dm, &text("hi bob"), &pusher)
        .unwrap();
    assert_eq!(pusher.received_by(bob.did()).len(), 1);
    assert!(pusher.received_by(carol.did()).is_empty());

    refused(
        f.sync(&carol, &carol_token, &dm, &LoroDoc::new()),
        CourierError::NoRead,
    );
    // Owning the node does not open someone else's DM either.
    let carols_dm = format!("dm/{}/{}/{DAY}", bob.did(), carol.did());
    f.sync(&bob, &bob_token, &carols_dm, &text("hi carol"))
        .unwrap();
    refused(
        f.sync(&f.alice, &f.alice_token, &carols_dm, &LoroDoc::new()),
        CourierError::NoRead,
    );
}

#[test]
fn a_group_reaches_whoever_its_meta_lists_now() {
    let f = fixture_with(CHAT);
    let (bob, bob_token) = f.member();
    let (carol, carol_token) = f.member();
    let (meta, day) = ("group/g1/meta", format!("group/g1/{DAY}"));
    let listed = group(&[f.alice.did(), bob.did()]);
    f.sync(&f.alice, &f.alice_token, meta, &listed).unwrap();
    f.subscribe(&bob, &bob_token, &day);
    f.subscribe(&carol, &carol_token, &day);

    let pusher = MockPusher::new();
    f.sync_on(&f.item, &f.alice, &f.alice_token, &day, &text("for bob"), &pusher)
        .unwrap();
    assert_eq!(pusher.received_by(bob.did()).len(), 1);
    assert!(pusher.received_by(carol.did()).is_empty());

    refused(
        f.sync(&carol, &carol_token, meta, &LoroDoc::new()),
        CourierError::NoRead,
    );
    refused(
        f.sync(&carol, &carol_token, &day, &LoroDoc::new()),
        CourierError::NoRead,
    );
    let tomorrow = "group/g1/2026-10-05";
    refused(
        f.sync(&carol, &carol_token, tomorrow, &text("let me in")),
        CourierError::NoWrite,
    );

    // The read rule is re-read on every push: listing carol reaches her from then on.
    listed.get_map("members").insert(carol.did(), true).unwrap();
    f.sync(&f.alice, &f.alice_token, meta, &listed).unwrap();
    let pusher = MockPusher::new();
    let next = text("for bob");
    next.get_text("t").insert(0, "and carol ").unwrap();
    f.sync_on(&f.item, &f.alice, &f.alice_token, &day, &next, &pusher)
        .unwrap();
    assert_eq!(pusher.received_by(carol.did()).len(), 1);
    f.sync(&carol, &carol_token, &day, &LoroDoc::new()).unwrap();
}

#[test]
fn an_undeclared_doc_is_refused_even_to_the_owner() {
    let f = fixture_with(CHAT);
    refused(
        f.sync(&f.alice, &f.alice_token, "smuggled/x", &text("x")),
        CourierError::Undeclared,
    );
    assert!(f.stored("smuggled/x").is_none());
}

#[test]
fn a_one_doc_grant_writes_that_doc_and_reaches_nothing_beside_it() {
    let f = fixture_with(CHAT);
    let dave = identity::generate().0;
    let scope = Scope::Resource(format!("ws/{}/{}/chat", f.ws, f.item));
    let token = f
        .vault
        .with_signer(|node| issue_root(node, dave.did(), "admin", scope, false, NOW, NOW + 100))
        .unwrap()
        .unwrap();
    f.admin.record(&token, Cause::Node, NOW).unwrap();

    f.sync(&dave, &token, "chat", &text("dave's channels"))
        .unwrap();
    refused(
        f.sync(&dave, &token, "group/g1/meta", &group(&[dave.did()])),
        CourierError::OutOfScope,
    );
}

#[test]
fn an_item_whose_source_the_node_lacks_is_membership_only() {
    let f = fixture_with(CHAT);
    let (bob, bob_token) = f.member();
    f.sync_on(
        &minted('c'),
        &bob,
        &bob_token,
        "anything",
        &text("x"),
        &MockPusher::new(),
    )
    .unwrap();
}
