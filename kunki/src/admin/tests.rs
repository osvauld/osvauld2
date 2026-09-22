use std::collections::HashSet;

use courier::DesktopNodeRecord;
use courier::token::{Scope, Token, verify_chain};
use identity::Identity;
use tempfile::TempDir;
use vault::Vault;

use super::*;
use crate::node;

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
    Ok(courier::desktop_finish_claim(&ticket, welcome, desktop)?)
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
    assert!(admin.accept_reconnect(hello, &mut challenges).is_ok());

    // A stranger is refused by the same list that admitted alice.
    let mut challenges = Vec::new();
    let challenge = vault
        .with_signer(|node| courier::node_issue_reconnect_challenge(node, &mut challenges))
        .unwrap();
    let stranger = holder();
    let hello = courier::desktop_start_reconnect(&record, &stranger, challenge).unwrap();
    assert!(matches!(
        admin.accept_reconnect(hello, &mut challenges),
        Err(NodeError::Courier(courier::CourierError::UnknownAdmin))
    ));
}

#[test]
fn a_relationship_is_not_read_as_a_token() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = holder();
    claim(&admin, &vault, &alice).unwrap();

    // Both live under `users/<did>/`; only one of them is a grant.
    assert_eq!(admin.issued_to(alice.did()).unwrap(), vec![]);
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
