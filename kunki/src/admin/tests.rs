use std::collections::HashSet;

use courier::DesktopNodeRecord;
use courier::invite::{InviteRequest, desktop_start_invite_claim};
use courier::publish::{PublishedItem, PublishedWorkspace, desktop_publish};
use courier::subscribe::desktop_start_subscribe;
use courier::sync::{SyncLayer, desktop_start_sync};
use courier::token::{Scope, Token, verify_chain};
use identity::Identity;
use loro::LoroDoc;
use tempfile::TempDir;
use vault::Vault;

use super::*;
use crate::node;
use crate::push::MockPusher;

const NOW: u64 = 10;
const EXP: u64 = 1_000;

fn node_vault() -> (TempDir, Vault) {
    let tmp = TempDir::new().unwrap();
    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    (tmp, vault)
}

fn holder() -> Identity {
    identity::generate().0
}

fn root_for(vault: &Vault, aud: &str) -> Token {
    vault
        .with_signer(|node| {
            courier::token::issue_root(node, aud, "owner", Scope::Node, true, NOW, EXP)
        })
        .unwrap()
        .unwrap()
}

#[test]
fn a_token_is_found_by_id_and_under_its_holder() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let bob = holder();

    let id = admin
        .record(&root_for(&vault, alice.did()), Cause::Node, NOW)
        .unwrap();
    admin
        .record(&root_for(&vault, bob.did()), Cause::Node, NOW)
        .unwrap();

    let issue = admin.issue(&id).unwrap().expect("recorded");
    assert_eq!(issue.holder, alice.did());
    assert_eq!(issue.at, NOW);

    let alices = admin.issued_to(alice.did()).unwrap();
    assert_eq!(alices.len(), 1, "bob's token is not in alice's index");
    assert_eq!(alices[0], issue, "the index leads back to the whole record");
    assert_eq!(admin.issued_to("did:key:znobody").unwrap(), vec![]);
}

/// Drive a full claim against `admin`, returning what the desktop keeps.
fn claim(admin: &Admin, vault: &Vault, desktop: &Identity) -> Result<DesktopNodeRecord, NodeError> {
    let ticket = vault
        .with_signer(|node| courier::issue_connection_ticket(node, NOW, "kunki"))
        .unwrap()?;
    let hello = courier::desktop_start_claim(ticket.clone(), desktop, NOW)?;
    let welcome = admin.accept_claim(hello, NOW)?;
    Ok(courier::desktop_finish_claim(
        &ticket, welcome, desktop, NOW,
    )?)
}

fn minted(byte: char) -> String {
    std::iter::repeat(byte).take(32).collect()
}

#[test]
fn a_published_workspace_and_its_items_are_adopted() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();

    let ws = PublishedWorkspace {
        id: minted('a'),
        name: "notes".to_string(),
        created: 1,
    };
    let item = PublishedItem {
        id: minted('b'),
        name: "board".to_string(),
        kind: "app".to_string(),
        created: 2,
    };
    let hello = desktop_publish(
        alice.did(),
        record.token.clone(),
        ws.clone(),
        vec![item.clone()],
    );

    let ack = admin.accept_publish(hello, NOW).unwrap();
    assert!(
        ack.known_items.is_empty(),
        "nothing held before this publish"
    );

    assert_eq!(vault.workspaces().unwrap()[0].id, ws.id);
    let items = vault.items(&ws.id).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, item.id);
    assert_eq!(items[0].kind, vault::ItemKind::App);
}

#[test]
fn a_republish_reports_what_was_already_held() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let ws = PublishedWorkspace {
        id: minted('a'),
        name: "notes".to_string(),
        created: 1,
    };
    let first = PublishedItem {
        id: minted('b'),
        name: "board".to_string(),
        kind: "app".to_string(),
        created: 2,
    };
    admin
        .accept_publish(
            desktop_publish(
                alice.did(),
                record.token.clone(),
                ws.clone(),
                vec![first.clone()],
            ),
            NOW,
        )
        .unwrap();

    let second = PublishedItem {
        id: minted('c'),
        ..first.clone()
    };
    let hello = desktop_publish(alice.did(), record.token, ws, vec![second]);
    let ack = admin.accept_publish(hello, NOW).unwrap();
    assert_eq!(ack.known_items, vec![first.id]);
}

#[test]
fn a_kind_this_build_does_not_know_is_refused() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let ws = PublishedWorkspace {
        id: minted('a'),
        name: "notes".to_string(),
        created: 1,
    };
    let item = PublishedItem {
        id: minted('b'),
        name: "board".to_string(),
        kind: "spreadsheet".to_string(),
        created: 2,
    };
    let hello = desktop_publish(alice.did(), record.token, ws, vec![item]);

    assert!(matches!(
        admin.accept_publish(hello, NOW),
        Err(NodeError::BadItemKind(k)) if k == "spreadsheet"
    ));
}

#[test]
fn revoking_a_claimant_stops_it_publishing() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();

    let ws = PublishedWorkspace {
        id: minted('a'),
        name: "notes".to_string(),
        created: 1,
    };
    let hello = desktop_publish(alice.did(), record.token.clone(), ws, vec![]);

    admin.revoke(&record.token.id(), NOW).unwrap();
    assert!(matches!(
        admin.accept_publish(hello, NOW),
        Err(NodeError::Courier(courier::CourierError::Revoked))
    ));
}

#[test]
fn an_invite_is_minted_and_redeemed_for_the_role_it_names() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let bob = holder();

    let request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token: record.token,
        role: "member".to_string(),
        scope: Scope::Workspace(minted('a')),
    };
    let ticket = admin.issue_invite(&request, "kunki", NOW).unwrap();

    let hello = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    let welcome = admin.accept_invite(hello, NOW).unwrap();

    let claims = verify_chain(
        &welcome.token,
        &node::did(&vault).unwrap(),
        bob.did(),
        NOW,
        &HashSet::new(),
    )
    .unwrap();
    assert_eq!(claims.role, "member");
    assert_eq!(claims.scope, Scope::Workspace(minted('a')));
}

#[test]
fn a_redeemed_invite_survives_a_restart_and_cannot_be_redeemed_twice() {
    let tmp = TempDir::new().unwrap();
    let alice = holder();
    let bob = holder();

    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    let record = claim(&Admin::new(vault.clone()), &vault, &alice).unwrap();
    let request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token: record.token,
        role: "member".to_string(),
        scope: Scope::Workspace(minted('a')),
    };
    let ticket = Admin::new(vault.clone())
        .issue_invite(&request, "kunki", NOW)
        .unwrap();
    let hello = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    Admin::new(vault.clone())
        .accept_invite(hello.clone(), NOW)
        .unwrap();
    drop(vault);

    // The whole point: before this, `redeemed` would have been an in-memory set, so a reboot
    // handed the same ticket a second life.
    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    let admin = Admin::new(vault.clone());
    assert!(matches!(
        admin.accept_invite(hello, NOW),
        Err(NodeError::Courier(
            courier::CourierError::InviteAlreadyRedeemed
        ))
    ));
}

#[test]
fn an_invite_for_a_role_with_real_capability_is_refused() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();

    let request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token: record.token,
        role: "maintainer".to_string(),
        scope: Scope::Workspace(minted('a')),
    };
    assert!(matches!(
        admin.issue_invite(&request, "kunki", NOW),
        Err(NodeError::Courier(courier::CourierError::RoleNotInvitable))
    ));
}

#[test]
fn an_invite_at_node_scope_cannot_grant_a_role_that_gains_capability_once_narrowed_to_a_workspace()
{
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();

    let request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token: record.token,
        role: "maintainer".to_string(),
        scope: Scope::Node,
    };
    assert!(matches!(
        admin.issue_invite(&request, "kunki", NOW),
        Err(NodeError::Courier(courier::CourierError::RoleNotInvitable))
    ));
}

#[test]
fn a_revoked_inviter_cannot_mint_an_invite() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    admin.revoke(&record.token.id(), NOW).unwrap();

    let request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token: record.token,
        role: "member".to_string(),
        scope: Scope::Workspace(minted('a')),
    };
    assert!(matches!(
        admin.issue_invite(&request, "kunki", NOW),
        Err(NodeError::Courier(courier::CourierError::Revoked))
    ));
}

#[test]
fn a_claimed_node_is_still_claimed_after_a_restart() {
    let tmp = TempDir::new().unwrap();
    let alice = holder();

    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    claim(&Admin::new(vault.clone()), &vault, &alice).unwrap();
    drop(vault);

    // The whole point: before this, the list was an in-memory Vec, so a reboot handed the
    // node to whoever claimed it next.
    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    let admin = Admin::new(vault.clone());
    assert_eq!(admin.admins().unwrap().len(), 1);
    assert_eq!(admin.admins().unwrap()[0].did, alice.did());
    assert!(matches!(
        claim(&admin, &vault, &holder()),
        Err(NodeError::Courier(courier::CourierError::AlreadyAdmined))
    ));
}

#[test]
fn a_reconnect_after_a_restart_is_answered_from_the_store() {
    let tmp = TempDir::new().unwrap();
    let alice = holder();

    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    let record = claim(&Admin::new(vault.clone()), &vault, &alice).unwrap();
    drop(vault);

    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    let admin = Admin::new(vault.clone());
    let mut challenges = Vec::new();
    let challenge = vault
        .with_signer(|node| courier::node_issue_reconnect_challenge(node, &mut challenges))
        .unwrap();
    let hello = courier::desktop_start_reconnect(&record, &alice, challenge).unwrap();
    assert!(admin.accept_reconnect(hello, &mut challenges, NOW).is_ok());

    // A stranger is refused by the same list that admitted alice.
    let mut challenges = Vec::new();
    let challenge = vault
        .with_signer(|node| courier::node_issue_reconnect_challenge(node, &mut challenges))
        .unwrap();
    let stranger = holder();
    let hello = courier::desktop_start_reconnect(&record, &stranger, challenge).unwrap();
    assert!(matches!(
        admin.accept_reconnect(hello, &mut challenges, NOW),
        Err(NodeError::Courier(courier::CourierError::UnknownAdmin))
    ));
}

#[test]
fn revoking_a_claimant_stops_it_reconnecting() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();

    let reconnect = |record: &DesktopNodeRecord| {
        let mut challenges = Vec::new();
        let challenge = vault
            .with_signer(|node| courier::node_issue_reconnect_challenge(node, &mut challenges))
            .unwrap();
        let hello = courier::desktop_start_reconnect(record, &alice, challenge).unwrap();
        admin.accept_reconnect(hello, &mut challenges, NOW)
    };

    assert!(reconnect(&record).is_ok(), "an admin in good standing");

    // The node takes the grant back. Nothing about this was possible while the credential was
    // a permit: it had no id to name and no check that would have consulted one.
    admin.revoke(&record.token.id(), NOW).unwrap();
    assert!(matches!(
        reconnect(&record),
        Err(NodeError::Courier(courier::CourierError::Revoked))
    ));

    // Still an admin by relationship — revoking a grant is not forgetting the person, and the
    // fresh token from the successful reconnect above is a separate grant that still stands.
    assert_eq!(admin.admins().unwrap().len(), 1);
}

#[test]
fn a_relationship_is_not_read_as_a_token() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();

    // Both live under `users/<did>/`, and a claim now writes both: the grant it issued and
    // the relationship that admitted them. Exactly one of them is a token.
    let issued = admin.issued_to(alice.did()).unwrap();
    assert_eq!(issued.len(), 1, "the relationship is not a second grant");
    assert_eq!(issued[0].token, record.token, "the token the claimant kept");
    assert_eq!(issued[0].cause, Cause::Node, "the node's own decision");
    assert_eq!(admin.admins().unwrap().len(), 1);
}

#[test]
fn a_profile_beside_the_tokens_is_not_read_as_one() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    admin
        .record(&root_for(&vault, alice.did()), Cause::Node, NOW)
        .unwrap();

    // What `users/<did>/meta` will hold once profiles exist. The tokens sit one level
    // further down precisely so this cannot be mistaken for a grant.
    vault
        .put_entry(&format!("users/{}/meta", alice.did()), b"display name")
        .unwrap();

    assert_eq!(admin.issued_to(alice.did()).unwrap().len(), 1);
}

#[test]
fn the_cause_records_lineage_a_flat_token_no_longer_carries() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let (alice, bob) = (holder(), holder());

    let assigner = admin
        .record(&root_for(&vault, alice.did()), Cause::Node, NOW)
        .unwrap();
    // What `role.assign` will do: alice's authority causes a token the node signs itself, so
    // bob's token has no `prf` reaching alice. The cause is the only surviving link.
    let assigned = root_for(&vault, bob.did());
    assert!(assigned.claims().unwrap().prf.is_none());
    let assigned = admin
        .record(&assigned, Cause::Under(assigner), NOW)
        .unwrap();

    assert_eq!(admin.issue(&assigner).unwrap().unwrap().cause, Cause::Node);
    assert_eq!(
        admin.issue(&assigned).unwrap().unwrap().cause,
        Cause::Under(assigner)
    );
}

#[test]
fn the_revoked_set_is_what_the_chain_check_consumes() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let node_did = node::did(&vault).unwrap();
    let alice = holder();
    let token = root_for(&vault, alice.did());
    let id = admin.record(&token, Cause::Node, NOW).unwrap();

    assert!(admin.revoked().unwrap().is_empty());
    assert!(
        verify_chain(
            &token,
            &node_did,
            alice.did(),
            NOW,
            &admin.revoked().unwrap()
        )
        .is_ok()
    );

    admin.revoke(&id, NOW + 1).unwrap();
    assert_eq!(admin.revoked().unwrap(), HashSet::from([id]));
    assert_eq!(
        verify_chain(
            &token,
            &node_did,
            alice.did(),
            NOW,
            &admin.revoked().unwrap()
        ),
        Err(courier::CourierError::Revoked)
    );
    // Revocation kills the grant; it does not erase that the node made it.
    assert_eq!(admin.issued_to(alice.did()).unwrap().len(), 1);
}

#[test]
fn a_delegation_the_node_never_issued_can_still_be_revoked() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let (alice, bob) = (holder(), holder());
    let parent = root_for(&vault, alice.did());

    // Minted between holders: the node sees it for the first time when it is presented.
    let child = courier::token::delegate(
        &parent,
        &alice,
        bob.did(),
        Scope::Workspace("ws".into()),
        false,
        NOW,
        EXP,
    )
    .unwrap();

    admin.revoke(&child.id(), NOW + 1).unwrap();
    assert!(admin.revoked().unwrap().contains(&child.id()));
    assert!(
        admin.issue(&child.id()).unwrap().is_none(),
        "revoking does not invent a record the node never made"
    );
}

#[test]
fn a_locked_node_neither_reads_nor_writes_its_records() {
    let (_tmp, mut vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let token = root_for(&vault, alice.did());
    let id = admin.record(&token, Cause::Node, NOW).unwrap();

    vault.lock();

    // Sealed to the account, so the records are gone with the key, not merely hidden.
    assert!(matches!(
        admin.issue(&id),
        Err(NodeError::Vault(vault::VaultError::Locked))
    ));
    assert!(matches!(admin.revoked(), Err(NodeError::Vault(_))));
    assert!(matches!(
        admin.record(&token, Cause::Node, NOW),
        Err(NodeError::Vault(_))
    ));
    assert!(matches!(admin.revoke(&id, NOW), Err(NodeError::Vault(_))));
}

#[test]
fn a_locked_node_neither_mints_nor_redeems_invites() {
    let (_tmp, mut vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let bob = holder();

    let request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token: record.token,
        role: "member".to_string(),
        scope: Scope::Workspace(minted('a')),
    };
    let ticket = admin.issue_invite(&request, "kunki", NOW).unwrap();
    let hello = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();

    vault.lock();

    assert!(matches!(
        admin.issue_invite(&request, "kunki", NOW),
        Err(NodeError::Vault(_))
    ));
    assert!(matches!(
        admin.accept_invite(hello, NOW),
        Err(NodeError::Vault(_))
    ));
}

#[test]
fn a_sync_push_lands_in_the_named_doc_layer_and_a_later_push_finds_it_there() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let ws_id = minted('a');
    let item_id = minted('b');

    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hello").unwrap();
    let hello = desktop_start_sync(
        alice.did(),
        record.token.clone(),
        &ws_id,
        &item_id,
        SyncLayer::Doc("board".to_string()),
        &doc,
        None,
    )
    .unwrap();
    admin.accept_sync(hello, NOW, &MockPusher::new()).unwrap();

    let stored = vault.get_doc(&ws_id, &item_id, "board").unwrap().unwrap();
    let landed = LoroDoc::new();
    landed.import(&stored).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "hello");

    // A second push, from what `admin` already has on disk — the node-side load must go
    // through `get_doc`, not just trust whatever the first call happened to leave in memory.
    doc.get_text("t").insert(5, " world").unwrap();
    let hello = desktop_start_sync(
        alice.did(),
        record.token,
        &ws_id,
        &item_id,
        SyncLayer::Doc("board".to_string()),
        &doc,
        None,
    )
    .unwrap();
    admin.accept_sync(hello, NOW, &MockPusher::new()).unwrap();
    let stored = vault.get_doc(&ws_id, &item_id, "board").unwrap().unwrap();
    let landed = LoroDoc::new();
    landed.import(&stored).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "hello world");
}

#[test]
fn a_sync_push_to_the_src_layer_does_not_touch_the_doc_layer() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let ws_id = minted('a');
    let item_id = minted('b');

    let doc = LoroDoc::new();
    doc.get_text("main.lua").insert(0, "return {}").unwrap();
    let hello = desktop_start_sync(
        alice.did(),
        record.token,
        &ws_id,
        &item_id,
        SyncLayer::Src,
        &doc,
        None,
    )
    .unwrap();
    admin.accept_sync(hello, NOW, &MockPusher::new()).unwrap();

    assert!(vault.get_src(&ws_id, &item_id).unwrap().is_some());
    assert!(vault.get_doc(&ws_id, &item_id, "board").unwrap().is_none());
}

#[test]
fn revoking_a_claimant_stops_it_syncing() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    admin.revoke(&record.token.id(), NOW).unwrap();

    let hello = desktop_start_sync(
        alice.did(),
        record.token,
        &minted('a'),
        &minted('b'),
        SyncLayer::Doc("board".to_string()),
        &LoroDoc::new(),
        None,
    )
    .unwrap();
    assert!(matches!(
        admin.accept_sync(hello, NOW, &MockPusher::new()),
        Err(NodeError::Courier(courier::CourierError::Revoked))
    ));
}

#[test]
fn a_locked_node_does_not_sync() {
    let (_tmp, mut vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let hello = desktop_start_sync(
        alice.did(),
        record.token,
        &minted('a'),
        &minted('b'),
        SyncLayer::Doc("board".to_string()),
        &LoroDoc::new(),
        None,
    )
    .unwrap();

    vault.lock();

    // `node::did` is the first thing `accept_sync` reads, so a locked account fails there
    // with `NodeError::Locked`, not the wrapped `NodeError::Vault(VaultError::Locked)` a
    // later store read would give.
    assert!(matches!(
        admin.accept_sync(hello, NOW, &MockPusher::new()),
        Err(NodeError::Locked)
    ));
}

#[test]
fn a_subscriber_is_recorded_and_found_by_layer() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let ws_id = minted('a');
    let item_id = minted('b');
    let layer = SyncLayer::Doc("board".to_string());

    let hello = desktop_start_subscribe(alice.did(), record.token, &ws_id, &item_id, layer.clone());
    admin.subscribe(&hello, NOW).unwrap();

    assert_eq!(
        admin.subscribers_for(&ws_id, &item_id, &layer).unwrap(),
        vec![alice.did().to_string()]
    );
    // A different layer on the same item has its own, empty list — subscriptions don't leak
    // across layers.
    assert!(
        admin
            .subscribers_for(&ws_id, &item_id, &SyncLayer::Src)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_unsubscribe_removes_exactly_that_subscriber() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let (alice, bob) = (holder(), holder());
    let alice_record = claim(&admin, &vault, &alice).unwrap();
    let ws_id = minted('a');
    let item_id = minted('b');
    let layer = SyncLayer::Doc("board".to_string());

    // Only one desktop ever raw-claims a node; bob joins the workspace through an invite,
    // same as `an_invite_is_minted_and_redeemed_for_the_role_it_names`.
    let invite_request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token: alice_record.token.clone(),
        role: "member".to_string(),
        scope: Scope::Workspace(ws_id.clone()),
    };
    let ticket = admin.issue_invite(&invite_request, "kunki", NOW).unwrap();
    let bob_welcome = admin
        .accept_invite(desktop_start_invite_claim(ticket, &bob, NOW).unwrap(), NOW)
        .unwrap();

    admin
        .subscribe(
            &desktop_start_subscribe(
                alice.did(),
                alice_record.token,
                &ws_id,
                &item_id,
                layer.clone(),
            ),
            NOW,
        )
        .unwrap();
    let bob_hello = desktop_start_subscribe(
        bob.did(),
        bob_welcome.token,
        &ws_id,
        &item_id,
        layer.clone(),
    );
    admin.subscribe(&bob_hello, NOW).unwrap();

    admin.unsubscribe(&bob_hello, NOW).unwrap();
    assert_eq!(
        admin.subscribers_for(&ws_id, &item_id, &layer).unwrap(),
        vec![alice.did().to_string()]
    );
}

#[test]
fn a_revoked_token_cannot_subscribe() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    admin.revoke(&record.token.id(), NOW).unwrap();

    let hello = desktop_start_subscribe(
        alice.did(),
        record.token,
        &minted('a'),
        &minted('b'),
        SyncLayer::Doc("board".to_string()),
    );
    assert!(matches!(
        admin.subscribe(&hello, NOW),
        Err(NodeError::Courier(courier::CourierError::Revoked))
    ));
}

#[test]
fn a_crafted_item_id_cannot_forge_a_different_subscriptions_key() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let ws_id = minted('a');

    // Without encoding, `item_id = "X/doc"` with `Src` would land at the same key as
    // `item_id = "X"` with `Doc("src")` — the same aliasing class flagged in `accept_sync`'s
    // review, guarded against here from the start.
    let real_item = "X".to_string();
    let crafted_item = "X/doc".to_string();

    admin
        .subscribe(
            &desktop_start_subscribe(
                alice.did(),
                record.token.clone(),
                &ws_id,
                &crafted_item,
                SyncLayer::Src,
            ),
            NOW,
        )
        .unwrap();

    assert!(
        admin
            .subscribers_for(&ws_id, &real_item, &SyncLayer::Doc("src".to_string()))
            .unwrap()
            .is_empty(),
        "a crafted item_id must not alias a different item's layer"
    );
    assert_eq!(
        admin
            .subscribers_for(&ws_id, &crafted_item, &SyncLayer::Src)
            .unwrap(),
        vec![alice.did().to_string()]
    );
}

/// The "two desktops, one node" scenario end to end: alice edits and syncs, bob never calls
/// sync himself, and bob's copy still converges — because he subscribed, and `accept_sync`
/// pushed to him once alice's edit landed. `MockPusher` stands in for the transport that
/// doesn't exist yet; everything else here (claim, invite, sync, subscribe, fan-out) is real.
#[test]
fn two_desktops_converge_through_a_push_neither_one_pulled_for() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let (alice, bob) = (holder(), holder());
    let alice_record = claim(&admin, &vault, &alice).unwrap();
    let ws_id = minted('a');
    let item_id = minted('b');
    let layer = SyncLayer::Doc("board".to_string());

    let invite_request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token: alice_record.token.clone(),
        role: "member".to_string(),
        scope: Scope::Workspace(ws_id.clone()),
    };
    let ticket = admin.issue_invite(&invite_request, "kunki", NOW).unwrap();
    let bob_welcome = admin
        .accept_invite(desktop_start_invite_claim(ticket, &bob, NOW).unwrap(), NOW)
        .unwrap();

    admin
        .subscribe(
            &desktop_start_subscribe(
                bob.did(),
                bob_welcome.token,
                &ws_id,
                &item_id,
                layer.clone(),
            ),
            NOW,
        )
        .unwrap();

    let alice_doc = LoroDoc::new();
    alice_doc
        .get_text("t")
        .insert(0, "hello from alice")
        .unwrap();
    let hello = desktop_start_sync(
        alice.did(),
        alice_record.token,
        &ws_id,
        &item_id,
        layer,
        &alice_doc,
        None,
    )
    .unwrap();

    let pusher = MockPusher::new();
    admin.accept_sync(hello, NOW, &pusher).unwrap();

    let pushed = pusher.received_by(bob.did());
    assert_eq!(
        pushed.len(),
        1,
        "bob is the only subscriber, and never pushed himself"
    );

    let bob_doc = LoroDoc::new();
    bob_doc.import(&pushed[0].snapshot).unwrap();
    assert_eq!(bob_doc.get_text("t").to_string(), "hello from alice");
}

#[test]
fn a_push_never_targets_the_desktop_that_authored_the_sync() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    let record = claim(&admin, &vault, &alice).unwrap();
    let ws_id = minted('a');
    let item_id = minted('b');
    let layer = SyncLayer::Doc("board".to_string());

    admin
        .subscribe(
            &desktop_start_subscribe(
                alice.did(),
                record.token.clone(),
                &ws_id,
                &item_id,
                layer.clone(),
            ),
            NOW,
        )
        .unwrap();

    let hello = desktop_start_sync(
        alice.did(),
        record.token,
        &ws_id,
        &item_id,
        layer,
        &LoroDoc::new(),
        None,
    )
    .unwrap();

    let pusher = MockPusher::new();
    admin.accept_sync(hello, NOW, &pusher).unwrap();

    assert!(
        pusher.received_by(alice.did()).is_empty(),
        "a push back to the desktop that just pushed is a wasted round trip, not a bug caught elsewhere"
    );
}
