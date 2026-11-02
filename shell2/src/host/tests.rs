//! Admission without a connection: what the transport proved (`remote`) against what the
//! `Hello` claims. T16's half that lives on the host.

use courier::invite::{InviteRequest, desktop_start_invite_claim};
use courier::token::Scope;
use kunki::admin::Admin;
use tempfile::TempDir;
use vault::Vault;

use super::admit;
use crate::node;

const NOW: u64 = 1_700_000_000;

fn host() -> (Vault, TempDir, String) {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("alice", "pw").unwrap();
    let record = node::self_claim(&vault, NOW).unwrap();
    let ws = vault.create_workspace("team").unwrap();
    let request = InviteRequest {
        desktop_did: record.node_did.clone(),
        token: record.token,
        role: "member".into(),
        scope: Scope::Workspace(ws.id.clone()),
        public: true,
    };
    let code = Admin::new(vault.clone())
        .issue_code(&request, "alice", NOW)
        .unwrap();
    let (ticket, _) = Admin::new(vault.clone())
        .fetch_code(&code, NOW)
        .unwrap()
        .unwrap();
    (vault, tmp, ticket.to_text().unwrap())
}

fn member() -> (identity::Identity, [u8; 32], String) {
    let me = identity::generate().0;
    let device = {
        let mut d = [0u8; 32];
        rand_bytes(&mut d);
        cryptography::signature::public_key(&d)
    };
    let attestation = courier::attest::attest(&me, device, NOW).unwrap();
    (me, device, attestation)
}

fn rand_bytes(out: &mut [u8; 32]) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    for (i, b) in out.iter_mut().enumerate() {
        *b = (t >> (i % 16 * 8)) as u8 ^ (i as u8).wrapping_mul(31);
    }
}

#[test]
fn a_claim_by_the_attested_account_on_the_proven_device_is_admitted() {
    let (vault, _tmp, ticket) = host();
    let admin = Admin::new(vault);
    let (bob, device, attestation) = member();
    let ticket = courier::invite::InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    let admitted = admit(&admin, device, &attestation, None, Some(claim), NOW).unwrap();
    assert_eq!(admitted.did, bob.did());
    assert!(admitted.minted.is_some());
    // The token it minted admits the next session on its own.
    let again = admit(&admin, device, &attestation, Some(admitted.token), None, NOW).unwrap();
    assert!(again.minted.is_none());
}

#[test]
fn an_attestation_for_another_device_is_refused() {
    let (vault, _tmp, ticket) = host();
    let admin = Admin::new(vault);
    let (bob, _device, attestation) = member();
    let ticket = courier::invite::InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    let other = [7u8; 32];
    let err = admit(&admin, other, &attestation, None, Some(claim), NOW).unwrap_err();
    assert!(err.contains("another device"), "{err}");
}

#[test]
fn a_claim_by_someone_else_than_the_attested_account_is_refused() {
    let (vault, _tmp, ticket) = host();
    let admin = Admin::new(vault);
    let (_bob, device, attestation) = member();
    let carol = identity::generate().0;
    let ticket = courier::invite::InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &carol, NOW).unwrap();
    let err = admit(&admin, device, &attestation, None, Some(claim), NOW).unwrap_err();
    assert!(err.contains("another account"), "{err}");
}

#[test]
fn garbage_and_an_empty_hello_are_refused() {
    let (vault, _tmp, _ticket) = host();
    let admin = Admin::new(vault);
    let (_bob, device, attestation) = member();
    assert!(admit(&admin, device, "nonsense", None, None, NOW).is_err());
    let err = admit(&admin, device, &attestation, None, None, NOW).unwrap_err();
    assert!(err.contains("no token and no claim"), "{err}");
}
