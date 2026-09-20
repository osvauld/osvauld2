use std::collections::HashSet;

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
