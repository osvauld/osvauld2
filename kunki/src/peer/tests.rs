use courier::DesktopNodeRecord;
use identity::Identity;
use tempfile::TempDir;
use vault::Vault;

use super::*;
use crate::admin::Admin;
use crate::node;

const NOW: u64 = 10;

fn account() -> (TempDir, Vault) {
    let tmp = TempDir::new().unwrap();
    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    (tmp, vault)
}

fn holder() -> Identity {
    identity::generate().0
}

/// A full claim against a node, returning what the claimant keeps. The node half is
/// `admin`'s; everything this module stores starts life as the return value here.
fn try_claim(node_vault: &Vault, desktop: &Identity) -> Result<DesktopNodeRecord, NodeError> {
    let ticket = node_vault
        .with_signer(|node| courier::issue_connection_ticket(node, NOW, "kunki"))
        .unwrap()?;
    let hello = courier::desktop_start_claim(ticket.clone(), desktop, NOW)?;
    let welcome = Admin::new(node_vault.clone()).accept_claim(hello, NOW)?;
    Ok(courier::desktop_finish_claim(
        &ticket, welcome, desktop, NOW,
    )?)
}

fn claim(node_vault: &Vault, desktop: &Identity) -> DesktopNodeRecord {
    try_claim(node_vault, desktop).unwrap()
}

#[test]
fn a_claimant_still_knows_its_node_after_a_restart() {
    let (_node_tmp, node_vault) = account();
    let desktop_tmp = TempDir::new().unwrap();
    let alice = holder();

    let (desktop, _) = node::open(desktop_tmp.path().to_path_buf(), "pw").unwrap();
    let record = claim(&node_vault, &alice);
    Peer::new(desktop.clone()).record_node(&record).unwrap();
    drop(desktop);

    // The gap this closes: before it, `desktop_finish_claim`'s return value was dropped by
    // every caller, so a restart left the claimant unable to name the node it had claimed.
    let (desktop, _) = node::open(desktop_tmp.path().to_path_buf(), "pw").unwrap();
    let peer = Peer::new(desktop);
    let kept = peer.node(&record.node_did).unwrap().expect("remembered");
    assert_eq!(kept, record);
    assert_eq!(peer.nodes().unwrap(), vec![record.clone()]);

    // And it is enough to reconnect with, which is the only reason to keep it.
    let mut challenges = Vec::new();
    let challenge = node_vault
        .with_signer(|node| courier::node_issue_reconnect_challenge(node, &mut challenges))
        .unwrap();
    let hello = courier::desktop_start_reconnect(&kept, &alice, challenge).unwrap();
    assert!(
        Admin::new(node_vault.clone())
            .accept_reconnect(hello, &mut challenges, NOW)
            .is_ok()
    );
}

#[test]
fn one_relationship_per_node_however_often_its_token_is_reissued() {
    let (_tmp, desktop) = account();
    let peer = Peer::new(desktop);
    let (_a_tmp, node_a) = account();
    let (_b_tmp, node_b) = account();
    let alice = holder();

    let first = claim(&node_a, &alice);
    peer.record_node(&first).unwrap();
    peer.record_node(&claim(&node_b, &alice)).unwrap();
    assert_eq!(peer.nodes().unwrap().len(), 2, "two nodes, two records");

    // A node is claimed once, so a replacement token never arrives by claiming again.
    assert!(matches!(
        try_claim(&node_a, &alice),
        Err(NodeError::Courier(courier::CourierError::AlreadyAdmined))
    ));

    // It arrives by reconnecting, which reissues. Unlike an `admin` issue, which is appended,
    // this replaces: there is only ever one current token per node.
    let mut challenges = Vec::new();
    let challenge = node_a
        .with_signer(|node| courier::node_issue_reconnect_challenge(node, &mut challenges))
        .unwrap();
    let hello = courier::desktop_start_reconnect(&first, &alice, challenge).unwrap();
    let reissued = Admin::new(node_a.clone())
        .accept_reconnect(hello, &mut challenges, NOW)
        .unwrap();
    assert_ne!(
        reissued, first.token,
        "a fresh grant, not the one presented"
    );

    let again = courier::desktop_accept_reissue(&first, reissued, &alice, NOW).unwrap();
    peer.record_node(&again).unwrap();

    assert_eq!(peer.nodes().unwrap().len(), 2, "still two");
    assert_eq!(peer.node(&again.node_did).unwrap().unwrap(), again);
}

#[test]
fn a_sibling_under_the_same_node_is_not_read_as_a_relationship() {
    let (_tmp, desktop) = account();
    let peer = Peer::new(desktop.clone());
    let (_node_tmp, node_vault) = account();
    let record = claim(&node_vault, &holder());
    peer.record_node(&record).unwrap();

    // Tokens held from this node will live here. The scan must pass over them, or the first
    // one stored would be parsed as a relationship and fail the whole listing.
    desktop
        .put_entry(
            &format!("{NODES}{}/tokens/abc", record.node_did),
            b"not a relationship",
        )
        .unwrap();

    assert_eq!(peer.nodes().unwrap(), vec![record]);
}

#[test]
fn a_node_never_claimed_is_absent_rather_than_an_error() {
    let (_tmp, desktop) = account();
    assert_eq!(Peer::new(desktop).node("did:key:zstranger").unwrap(), None);
}

#[test]
fn a_locked_account_neither_reads_nor_writes_its_relationships() {
    let (_tmp, mut desktop) = account();
    let peer = Peer::new(desktop.clone());
    let (_node_tmp, node_vault) = account();
    let record = claim(&node_vault, &holder());
    peer.record_node(&record).unwrap();

    desktop.lock();

    // Sealed to the account: the permit is gone with the key, not merely hidden.
    assert!(matches!(
        peer.node(&record.node_did),
        Err(NodeError::Vault(vault::VaultError::Locked))
    ));
    assert!(matches!(peer.nodes(), Err(NodeError::Vault(_))));
    assert!(matches!(
        peer.record_node(&record),
        Err(NodeError::Vault(_))
    ));
}
