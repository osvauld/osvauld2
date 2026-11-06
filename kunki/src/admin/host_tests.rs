//! Appointed hosts — laptop-host step 7b: a hosted admin over the owner's ledger, appointment,
//! the host's open judgement, the uplink's trusted merges, and relay-only-what-was-new.

use std::collections::HashSet;

use courier::ledger::{self, Entry};
use courier::role::HOST;
use courier::subscribe::desktop_start_subscribe;
use courier::sync::{Author, SyncLayer, SyncReply, desktop_start_sync};
use courier::token::{Scope, Token, issue_root};
use identity::{Identity, Signer};
use loro::LoroDoc;
use tempfile::TempDir;
use vault::Vault;

use super::roles_tests::{NOW, acked, device, doc_of, fixture_with, minted, rejected};
use super::*;
use crate::node;
use crate::push::MockPusher;

const EXP: u64 = 1_000;

/// `chat` is an admin's to write; anything else is undeclared.
const CHAT: &str = r#"app "chat" {
  roles admin, member
  doc chat { read member write admin }
}"#;

/// Declares no docs, so membership alone decides.
const BARE: &str = r#"app "notes" {
  roles member
}"#;

fn node_vault() -> (TempDir, Vault) {
    let tmp = TempDir::new().unwrap();
    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    (tmp, vault)
}

fn who() -> Identity {
    identity::generate().0
}

/// A token `signer` issues `aud` directly — an owner minting without a vault.
fn minted_by(signer: &impl Signer, aud: &str, role: &str, scope: Scope) -> Token {
    issue_root(signer, aud, role, scope, false, NOW, EXP).unwrap()
}

fn minted_in(vault: &Vault, aud: &str, role: &str, scope: Scope) -> Token {
    vault
        .with_signer(|me| issue_root(me, aud, role, scope, false, NOW, EXP))
        .unwrap()
        .unwrap()
}

/// `who`'s own doc with `s` written in it.
fn text(who: &Identity, s: &str) -> LoroDoc {
    let doc = doc_of(who);
    doc.get_text("t").insert(0, s).unwrap();
    doc
}

fn stored_text(vault: &Vault, ws: &str, item: &str, name: &str) -> String {
    let bytes = vault.get_doc(ws, item, name).unwrap().unwrap();
    let doc = LoroDoc::new();
    doc.import(&bytes).unwrap();
    doc.get_text("t").to_string()
}

fn sync_hello(
    who: &Identity,
    token: &Token,
    ws: &str,
    item: &str,
    name: &str,
    doc: &LoroDoc,
) -> SyncHello {
    let layer = SyncLayer::Doc(name.to_string());
    desktop_start_sync(who.did(), token.clone(), ws, item, layer, doc, None).unwrap()
}

fn ids(grants: &[Grant]) -> HashSet<[u8; 32]> {
    grants.iter().map(|g| g.id).collect()
}

#[test]
fn a_hosted_admins_records_and_the_accounts_own_never_meet() {
    let (_tmp, vault) = node_vault();
    let owner = who();
    let own = Admin::new(vault.clone());
    let hosted = Admin::hosted(vault.clone(), owner.did());
    let alice = who();

    let mine = own
        .record(
            &minted_in(&vault, alice.did(), "owner", Scope::Node),
            Cause::Node,
            NOW,
        )
        .unwrap();
    let theirs = hosted
        .record(
            &minted_by(&owner, alice.did(), "owner", Scope::Node),
            Cause::Node,
            NOW,
        )
        .unwrap();

    assert_eq!(own.root().unwrap(), node::did(&vault).unwrap());
    assert_eq!(hosted.root().unwrap(), owner.did());
    assert!(
        own.issue(&theirs).unwrap().is_none(),
        "the owner's issue is not the account's"
    );
    assert!(
        hosted.issue(&mine).unwrap().is_none(),
        "nor the account's the owner's"
    );
    assert_eq!(own.issued_to(alice.did()).unwrap().len(), 1);
    assert_eq!(hosted.issued_to(alice.did()).unwrap().len(), 1);

    hosted.revoke(&theirs, NOW).unwrap();
    assert!(
        own.revoked().unwrap().is_empty(),
        "one root's revocation never shadows another's"
    );
    assert_eq!(hosted.revoked().unwrap(), HashSet::from([theirs]));
    assert!(own.is_live(&mine, NOW).unwrap());
    assert!(!hosted.is_live(&theirs, NOW).unwrap());
}

#[test]
fn a_hosted_admin_judges_a_chain_rooted_at_the_owner_not_at_the_vault() {
    let (_tmp, vault) = node_vault();
    let owner = who();
    let hosted = Admin::hosted(vault.clone(), owner.did());
    let alice = who();
    let (ws, item) = (minted('a'), minted('b'));
    let token = minted_by(&owner, alice.did(), "member", Scope::Workspace(ws.clone()));
    hosted.record(&token, Cause::Node, NOW).unwrap();

    let hello = sync_hello(&alice, &token, &ws, &item, "board", &text(&alice, "hi"));
    acked(hosted.accept_sync(hello.clone(), Some(device(&alice)), NOW, &MockPusher::new()));
    assert_eq!(stored_text(&vault, &ws, &item, "board"), "hi");

    // The same vault as its own authority knows nothing of the owner: the chain roots
    // elsewhere.
    assert!(matches!(
        Admin::new(vault.clone()).accept_sync(
            hello,
            Some(device(&alice)),
            NOW,
            &MockPusher::new()
        ),
        Err(NodeError::Courier(CourierError::NodeMismatch))
    ));
}

#[test]
fn the_owners_ledger_replayed_elsewhere_judges_the_same() {
    let (_tmp, owner_vault) = node_vault();
    let owner = Admin::new(owner_vault.clone());
    let owner_did = node::did(&owner_vault).unwrap();
    let alice = who();
    let (ws, other) = (minted('a'), minted('e'));
    let in_ws = |role: &str| {
        minted_in(
            &owner_vault,
            alice.did(),
            role,
            Scope::Workspace(ws.clone()),
        )
    };
    let first = owner.record(&in_ws("member"), Cause::Node, NOW).unwrap();
    let second = owner
        .record(&in_ws("maintainer"), Cause::Node, NOW)
        .unwrap();
    let node_wide = owner
        .record(
            &minted_in(&owner_vault, alice.did(), "owner", Scope::Node),
            Cause::Node,
            NOW,
        )
        .unwrap();
    let elsewhere = owner
        .record(
            &minted_in(
                &owner_vault,
                alice.did(),
                "member",
                Scope::Workspace(other.clone()),
            ),
            Cause::Node,
            NOW,
        )
        .unwrap();
    owner.revoke(&first, NOW + 1).unwrap();

    let entries = owner.ledger(&ws).unwrap();
    assert_eq!(
        entries.len(),
        4,
        "two in the space, one node-wide, one revocation"
    );
    assert!(
        !entries
            .iter()
            .any(|e| matches!(e, Entry::Issued(t) if t.id() == elsewhere)),
        "another space's issue is not this space's business"
    );
    assert!(entries.contains(&Entry::Revoked {
        id: first,
        at: NOW + 1
    }));

    let (_tmp2, host_vault) = node_vault();
    let host = Admin::hosted(host_vault.clone(), &owner_did);
    let signed = owner.signed_ledger(&ws).unwrap();
    for _ in 0..2 {
        for entry in &signed {
            host.replay(entry).unwrap();
        }
        for id in [first, second, node_wide] {
            assert_eq!(
                host.is_live(&id, NOW).unwrap(),
                owner.is_live(&id, NOW).unwrap()
            );
        }
        assert!(host.issue(&elsewhere).unwrap().is_none());
        let theirs = owner.grants(alice.did(), NOW).unwrap();
        let expected: HashSet<_> = ids(&theirs)
            .into_iter()
            .filter(|id| *id != elsewhere)
            .collect();
        assert_eq!(ids(&host.grants(alice.did(), NOW).unwrap()), expected);
        assert_eq!(
            host.issued_to(alice.did()).unwrap().len(),
            3,
            "a replay is a no-op"
        );
    }
    assert!(host.is_live(&second, NOW).unwrap());
    assert!(!host.is_live(&first, NOW).unwrap());

    // The ledger flows down, never up: a host has no key to sign the owner's facts with.
    assert!(matches!(host.signed_ledger(&ws), Err(NodeError::NotOwner(d)) if d == owner_did));
}

#[test]
fn a_ledger_signed_by_anyone_but_the_owner_is_refused_whole() {
    let (_tmp, vault) = node_vault();
    let (owner, stranger, alice) = (who(), who(), who());
    let host = Admin::hosted(vault.clone(), owner.did());
    let token = minted_by(&owner, alice.did(), "member", Scope::Workspace(minted('a')));

    let forged = ledger::sign(&stranger, &Entry::Issued(token.clone())).unwrap();
    assert!(matches!(
        host.replay(&forged),
        Err(NodeError::Courier(CourierError::BadSignature))
    ));
    assert!(
        host.issue(&token.id()).unwrap().is_none(),
        "nothing recorded on a refusal"
    );

    let forged = ledger::sign(
        &stranger,
        &Entry::Revoked {
            id: token.id(),
            at: NOW,
        },
    )
    .unwrap();
    assert!(host.replay(&forged).is_err());
    assert!(host.revoked().unwrap().is_empty());

    // A real one, so the refusals above were about the signer and not the shape.
    host.replay(&ledger::sign(&owner, &Entry::Issued(token.clone())).unwrap())
        .unwrap();
    assert!(host.is_live(&token.id(), NOW).unwrap());
}

#[test]
fn a_ledger_with_one_forged_entry_among_valid_ones_records_nothing() {
    let (_tmp, vault) = node_vault();
    let (owner, stranger, alice) = (who(), who(), who());
    let host = Admin::hosted(vault.clone(), owner.did());
    let ws = minted('a');
    let first = minted_by(&owner, alice.did(), "member", Scope::Workspace(ws.clone()));
    let second = minted_by(&owner, alice.did(), "maintainer", Scope::Workspace(ws.clone()));
    let entries = vec![
        ledger::sign(&owner, &Entry::Issued(first.clone())).unwrap(),
        ledger::sign(
            &stranger,
            &Entry::Revoked {
                id: first.id(),
                at: NOW,
            },
        )
        .unwrap(),
        ledger::sign(&owner, &Entry::Issued(second.clone())).unwrap(),
    ];

    assert!(matches!(
        host.replay_all(&entries),
        Err(NodeError::Courier(CourierError::BadSignature))
    ));
    assert!(
        host.issue(&first.id()).unwrap().is_none(),
        "the valid entry before the forgery was not applied either"
    );
    assert!(host.issue(&second.id()).unwrap().is_none());
    assert!(host.revoked().unwrap().is_empty());

    // Without the forgery the same entries go in whole, and again idempotently.
    let honest = [entries[0].clone(), entries[2].clone()];
    for _ in 0..2 {
        host.replay_all(&honest).unwrap();
        assert!(host.is_live(&first.id(), NOW).unwrap());
        assert!(host.is_live(&second.id(), NOW).unwrap());
        assert_eq!(host.issued_to(alice.did()).unwrap().len(), 2);
    }
}

#[test]
fn replaying_the_owners_own_ledger_into_itself_keeps_its_lineage() {
    let (_tmp, vault) = node_vault();
    let owner = Admin::new(vault.clone());
    let alice = who();
    let ws = minted('a');
    let parent = owner
        .record(
            &minted_in(&vault, alice.did(), "owner", Scope::Node),
            Cause::Node,
            NOW,
        )
        .unwrap();
    let child = owner
        .record(
            &minted_in(&vault, alice.did(), "member", Scope::Workspace(ws.clone())),
            Cause::Under(parent),
            NOW,
        )
        .unwrap();

    for entry in owner.signed_ledger(&ws).unwrap() {
        owner.replay(&entry).unwrap();
    }
    assert_eq!(
        owner.issue(&child).unwrap().unwrap().cause,
        Cause::Under(parent)
    );
}

#[test]
fn an_appointment_is_a_host_grant_on_exactly_that_space_until_revoked() {
    let f = fixture_with(BARE);
    let (bob, carol) = (who(), who());
    let other = minted('e');

    let token = f.admin.appoint_host(&f.ws, bob.did(), NOW).unwrap();
    let claims = token.claims().unwrap();
    assert_eq!(claims.role, HOST);
    assert_eq!(claims.scope, Scope::Workspace(f.ws.clone()));
    assert_eq!(claims.aud, bob.did());
    assert_eq!(claims.iss, node::did(&f.vault).unwrap());
    assert!(!claims.delegable, "a host serves, it does not appoint");
    assert_eq!(claims.exp, NOW + HOST_TTL);
    assert_eq!(
        f.admin.issue(&token.id()).unwrap().unwrap().cause,
        Cause::Node
    );

    assert!(f.admin.is_host(bob.did(), &f.ws, NOW).unwrap());
    assert!(!f.admin.is_host(bob.did(), &other, NOW).unwrap());
    assert!(!f.admin.is_host(carol.did(), &f.ws, NOW).unwrap());
    assert!(!f.admin.is_host(bob.did(), &f.ws, NOW + HOST_TTL).unwrap());

    f.admin.appoint_host(&f.ws, carol.did(), NOW).unwrap();
    let mut both = vec![bob.did().to_string(), carol.did().to_string()];
    both.sort();
    assert_eq!(f.admin.hosts_of(&f.ws, NOW).unwrap(), both);
    assert!(f.admin.hosts_of(&other, NOW).unwrap().is_empty());

    f.admin.revoke(&token.id(), NOW + 1).unwrap();
    assert!(!f.admin.is_host(bob.did(), &f.ws, NOW + 1).unwrap());
    assert_eq!(
        f.admin.hosts_of(&f.ws, NOW + 1).unwrap(),
        vec![carol.did().to_string()]
    );
    // An app's own role named `host` is an app role, not an appointment.
    f.admin
        .record(&minted_in(&f.vault, bob.did(), HOST, f.app()), Cause::Node, NOW)
        .unwrap();
    assert!(!f.admin.is_host(bob.did(), &f.ws, NOW + 1).unwrap());

    assert!(matches!(
        f.admin.appoint_host(&f.ws, "not-a-did", NOW),
        Err(NodeError::Courier(CourierError::Decode))
    ));
    // A hosted admin holds the owner's records without the owner's key.
    assert!(matches!(
        Admin::hosted(f.vault.clone(), carol.did()).appoint_host(&f.ws, bob.did(), NOW),
        Err(NodeError::NotOwner(d)) if d == carol.did()
    ));
}

#[test]
fn a_host_is_judged_open_on_every_doc_of_its_space() {
    let f = fixture_with(CHAT);
    let (bob, bob_token) = f.member();
    let carol = who();
    let carol_token = f.admin.appoint_host(&f.ws, carol.did(), NOW).unwrap();

    // Declared and admin-only to write: a member is refused, the host is not.
    rejected(
        f.admin.accept_sync(
            sync_hello(&bob, &bob_token, &f.ws, &f.item, "chat", &text(&bob, "bob")),
            Some(device(&bob)),
            NOW,
            &MockPusher::new(),
        ),
        "may not write",
    );
    acked(f.admin.accept_sync(
        sync_hello(
            &carol,
            &carol_token,
            &f.ws,
            &f.item,
            "chat",
            &text(&carol, "relayed"),
        ),
        Some(device(&carol)),
        NOW,
        &MockPusher::new(),
    ));
    assert_eq!(stored_text(&f.vault, &f.ws, &f.item, "chat"), "relayed");

    // Undeclared: nobody's, except a host's.
    let hello = sync_hello(&bob, &bob_token, &f.ws, &f.item, "secret", &LoroDoc::new());
    let judged = f.admin.judge(&hello, Some(device(&bob)), NOW).unwrap();
    assert!(judged.undeclared && !judged.access.read);
    assert!(
        matches!(judged.author, Author::Device(_)),
        "a member's push is bound to its device's peer"
    );
    let hello = sync_hello(
        &carol,
        &carol_token,
        &f.ws,
        &f.item,
        "secret",
        &LoroDoc::new(),
    );
    let judged = f.admin.judge(&hello, Some(device(&carol)), NOW).unwrap();
    assert_eq!(judged.access, Access::OPEN);
    assert!(!judged.undeclared);
    assert_eq!(judged.author, Author::Host, "a host relays others' changes by design");
    assert!(
        judged.rules.is_some(),
        "the rules still travel, for the fan-out's re-check"
    );

    // Only on its own space.
    let other = minted('e');
    let hello = sync_hello(&carol, &carol_token, &other, &f.item, "chat", &text(&carol, "x"));
    assert!(matches!(
        f.admin.accept_sync(hello, Some(device(&carol)), NOW, &MockPusher::new()),
        Err(NodeError::Courier(CourierError::OutOfScope))
    ));
}

#[test]
fn a_trusted_update_is_merged_as_the_owners_and_relayed_only_when_it_moved() {
    let (_tmp, vault) = node_vault();
    let owner = who();
    let host = Admin::hosted(vault.clone(), owner.did());
    let bob = who();
    let (ws, item) = (minted('a'), minted('b'));
    let layer = SyncLayer::Doc("board".to_string());
    let bob_token = minted_by(&owner, bob.did(), "member", Scope::Workspace(ws.clone()));
    host.record(&bob_token, Cause::Node, NOW).unwrap();
    host.subscribe(
        &desktop_start_subscribe(bob.did(), bob_token, &ws, &item, layer.clone()),
        NOW,
    )
    .unwrap();

    let update = text(&owner, "from the owner")
        .export(loro::ExportMode::Snapshot)
        .unwrap();
    let pusher = MockPusher::new();
    assert!(
        host.accept_trusted(&ws, &item, &layer, &update, "uplink/1", NOW, &pusher)
            .unwrap()
    );
    assert_eq!(stored_text(&vault, &ws, &item, "board"), "from the owner");
    let pushed = pusher.received_by(bob.did());
    assert_eq!(pushed.len(), 1);
    assert_eq!(
        pushed[0].author,
        owner.did(),
        "merged as the owner's own write"
    );
    assert_eq!(pushed[0].origin, "uplink/1");
    assert_eq!(pusher.broadcasts().len(), 1);

    // The same bytes again: the version vector does not move, nothing is relayed.
    assert!(
        !host
            .accept_trusted(&ws, &item, &layer, &update, "uplink/2", NOW, &pusher)
            .unwrap()
    );
    assert_eq!(pusher.received_by(bob.did()).len(), 1);
    assert_eq!(pusher.broadcasts().len(), 1);
    assert!(
        !host
            .accept_trusted(&ws, &item, &layer, &[], "uplink/3", NOW, &pusher)
            .unwrap()
    );

    // The source layer lands in its own key.
    let src = text(&owner, "return {}")
        .export(loro::ExportMode::Snapshot)
        .unwrap();
    assert!(
        host.accept_trusted(&ws, &item, &SyncLayer::Src, &src, "uplink/4", NOW, &pusher)
            .unwrap()
    );
    assert!(vault.get_src(&ws, &item).unwrap().is_some());
    assert_eq!(pusher.broadcasts().len(), 2);

    assert!(matches!(
        host.accept_trusted(&ws, &item, &layer, b"not loro", "uplink/5", NOW, &pusher),
        Err(NodeError::Courier(CourierError::Decode))
    ));
}

#[test]
fn an_update_the_host_already_held_is_acked_but_not_relayed() {
    let f = fixture_with(BARE);
    let (bob, bob_token) = f.member();
    let layer = SyncLayer::Doc("board".to_string());
    f.admin
        .subscribe(
            &desktop_start_subscribe(bob.did(), bob_token, &f.ws, &f.item, layer),
            NOW,
        )
        .unwrap();
    let hello = sync_hello(
        &f.alice,
        &f.alice_token,
        &f.ws,
        &f.item,
        "board",
        &text(&f.alice, "once"),
    );

    let pusher = MockPusher::new();
    let alice = Some(device(&f.alice));
    let first = acked(f.admin.accept_sync(hello.clone(), alice, NOW, &pusher));
    let again = acked(f.admin.accept_sync(hello.clone(), alice, NOW, &pusher));
    assert!(first.changed && !again.changed);
    assert_eq!(
        pusher.received_by(bob.did()).len(),
        1,
        "the second push carried nothing new"
    );
    assert_eq!(pusher.broadcasts().len(), 1);

    // The live path keeps the same rule.
    let live = LoroDoc::new();
    let judged = f.admin.judge(&hello, alice, NOW).unwrap();
    let pusher = MockPusher::new();
    for _ in 0..2 {
        match f
            .admin
            .accept_live(&live, &hello, &judged, NOW, &pusher)
            .unwrap()
        {
            SyncReply::Ack(_) => {}
            other => panic!("expected an ack, got {other:?}"),
        }
    }
    assert_eq!(pusher.received_by(bob.did()).len(), 1);
    assert_eq!(pusher.broadcasts().len(), 1);
}
