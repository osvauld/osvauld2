use identity::Identity;

use super::*;
use serde::de::DeserializeOwned;

fn ids() -> (Identity, Identity) {
    let (node, _) = identity::generate();
    let (desktop, _) = identity::generate();
    (node, desktop)
}

/// Bytes round-trip, standing in for a transport hop.
fn wire<T: Serialize + DeserializeOwned>(msg: T) -> T {
    let bytes = bincode::serialize(&msg).unwrap();
    bincode::deserialize(&bytes).unwrap()
}

/// Nothing revoked. Storage lives in `kunki`, so courier is always handed this set.
fn none() -> HashSet<[u8; 32]> {
    HashSet::new()
}

#[test]
fn bootstrap_claim_authenticates_and_reconnects() {
    let (node, desktop) = ids();
    let ticket = wire(issue_connection_ticket(&node, 1, "kunki").unwrap());
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();

    let welcome = node_accept_claim(wire(hello), &node, &mut admins, 3).unwrap();
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0].did, desktop.did());

    let record = desktop_finish_claim(&ticket, wire(welcome), &desktop, 4).unwrap();
    assert_eq!(record.node_did, node.did());
    assert_eq!(record.node_id, ticket.node_id);

    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, wire(challenge)).unwrap();
    assert!(
        node_accept_reconnect(wire(reconnect), &node, &admins, &mut challenges, 5, &none()).is_ok()
    );
}

#[test]
fn tampered_ticket_node_id_is_rejected() {
    let (node, desktop) = ids();
    let mut ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    ticket.node_id.push('x');

    assert_eq!(
        desktop_start_claim(ticket, &desktop, 2).unwrap_err(),
        CourierError::BadAttestation
    );
}

#[test]
fn the_attestation_binds_the_keys_presented_beside_it() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let mut hello = desktop_start_claim(ticket, &desktop, 2).unwrap();
    hello.desktop_device_key.push('x');
    let mut admins = Vec::new();

    assert_eq!(
        node_accept_claim(hello, &node, &mut admins, 3).unwrap_err(),
        CourierError::BadAttestation
    );
}

#[test]
fn an_attestation_signed_by_someone_else_is_refused() {
    let (node, desktop) = ids();
    let (impostor, _) = identity::generate();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let mut hello = desktop_start_claim(ticket, &desktop, 2).unwrap();

    // The keys and DID say desktop; the signature is the impostor's. Before, the attestation
    // was a flat blob checked field-by-field; now it is a chain that must root at its subject.
    hello.attestation = token::attest(
        &impostor,
        node.did(),
        token::KeyBinding {
            encryption: hello.desktop_encryption_key.clone(),
            device: hello.desktop_device_key.clone(),
        },
        2,
        1_000,
    )
    .unwrap();

    assert_eq!(
        node_accept_claim(hello, &node, &mut Vec::new(), 3).unwrap_err(),
        CourierError::NodeMismatch,
    );
}

#[test]
fn an_attestation_grants_nothing_the_policy_table_reads() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket, &desktop, 2).unwrap();
    let claims = hello.attestation.claims().unwrap();

    // It describes keys rather than granting anything, so its role must not appear in the
    // capability table — otherwise "I am me" would read as authority over the node.
    assert!(
        policy::platform_capabilities(&claims.role, &claims.scope).is_empty(),
        "role {:?} carries capabilities",
        claims.role
    );
    assert!(!claims.delegable, "nothing chains off a statement of keys");
    assert_eq!(claims.sub, desktop.did(), "the claimant is its own root");
}

#[test]
fn a_binding_survives_the_wire() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = wire(desktop_start_claim(ticket, &desktop, 2).unwrap());

    let binds = hello.attestation.claims().unwrap().binds.expect("bound");
    assert_eq!(binds.encryption, hello.desktop_encryption_key);
    assert_eq!(binds.device, hello.desktop_device_key);
    assert!(node_accept_claim(hello, &node, &mut Vec::new(), 3).is_ok());
}

#[test]
fn second_bootstrap_claim_is_rejected() {
    let (node, desktop) = ids();
    let (other, _) = identity::generate();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();
    node_accept_claim(hello, &node, &mut admins, 3).unwrap();

    let second = desktop_start_claim(ticket, &other, 4).unwrap();
    assert_eq!(
        node_accept_claim(second, &node, &mut admins, 5).unwrap_err(),
        CourierError::AlreadyAdmined
    );
}

#[test]
fn reconnect_unknown_admin_is_rejected() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();
    let welcome = node_accept_claim(hello, &node, &mut admins, 3).unwrap();
    let record = desktop_finish_claim(&ticket, welcome, &desktop, 4).unwrap();
    admins.clear();

    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();
    assert_eq!(
        node_accept_reconnect(reconnect, &node, &admins, &mut challenges, 5, &none()).unwrap_err(),
        CourierError::UnknownAdmin
    );
}

#[test]
fn reconnect_requires_desktop_signature() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();
    let welcome = node_accept_claim(hello, &node, &mut admins, 3).unwrap();
    let record = desktop_finish_claim(&ticket, welcome, &desktop, 4).unwrap();
    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let mut reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();
    reconnect.signature = enc([0u8; 64]);

    assert_eq!(
        node_accept_reconnect(reconnect, &node, &admins, &mut challenges, 5, &none()).unwrap_err(),
        CourierError::BadSignature
    );
}

#[test]
fn reconnect_replay_is_rejected() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();
    let welcome = node_accept_claim(hello, &node, &mut admins, 3).unwrap();
    let record = desktop_finish_claim(&ticket, welcome, &desktop, 4).unwrap();
    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();

    node_accept_reconnect(
        reconnect.clone(),
        &node,
        &admins,
        &mut challenges,
        5,
        &none(),
    )
    .unwrap();
    assert_eq!(
        node_accept_reconnect(reconnect, &node, &admins, &mut challenges, 5, &none()).unwrap_err(),
        CourierError::StaleChallenge
    );
}

#[test]
fn welcome_from_wrong_node_is_rejected() {
    let (node, desktop) = ids();
    let (other_node, _) = identity::generate();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let welcome = ClaimWelcome {
        node_did: other_node.did().to_string(),
        token: issue_claim_token(&other_node, desktop.did(), 2).unwrap(),
    };

    assert_eq!(
        desktop_finish_claim(&ticket, welcome, &desktop, 4).unwrap_err(),
        CourierError::NodeMismatch
    );
}

/// Claim, then reconnect at `at`, returning what the node decided.
fn claim_then_reconnect(at: u64, revoked: &HashSet<[u8; 32]>) -> Result<Token> {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();
    let welcome = node_accept_claim(hello, &node, &mut admins, 3).unwrap();
    let record = desktop_finish_claim(&ticket, welcome, &desktop, 4).unwrap();

    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();
    node_accept_reconnect(reconnect, &node, &admins, &mut challenges, at, revoked)
}

#[test]
fn the_relationship_token_expires_where_the_permit_it_replaced_never_did() {
    // Issued at 3, so anywhere inside the window is fine.
    assert!(claim_then_reconnect(3 + CLAIM_TTL - 1, &none()).is_ok());

    // Past it, the credential is simply no longer one. `PermitClaim` had no `exp` field, so
    // this case could not be expressed at all before, let alone refused.
    assert_eq!(
        claim_then_reconnect(3 + CLAIM_TTL + 1, &none()).unwrap_err(),
        CourierError::Expired
    );
}

#[test]
fn a_revoked_relationship_token_cannot_reconnect() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();
    let welcome = node_accept_claim(hello, &node, &mut admins, 3).unwrap();
    let record = desktop_finish_claim(&ticket, welcome, &desktop, 4).unwrap();

    // Revocation names the grant by id — which a permit did not have, so taking one back was
    // impossible rather than merely unimplemented.
    let revoked = HashSet::from([record.token.id()]);

    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();
    assert_eq!(
        node_accept_reconnect(reconnect, &node, &admins, &mut challenges, 5, &revoked).unwrap_err(),
        CourierError::Revoked
    );
}

#[test]
fn reconnecting_replaces_the_token_rather_than_extending_it() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();
    let welcome = node_accept_claim(hello, &node, &mut admins, 3).unwrap();
    let record = desktop_finish_claim(&ticket, welcome, &desktop, 4).unwrap();

    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();
    let fresh =
        node_accept_reconnect(reconnect, &node, &admins, &mut challenges, 5, &none()).unwrap();

    // A distinct grant with its own id, so revoking the old one does not reach the new one.
    assert_ne!(fresh, record.token);
    assert_ne!(fresh.id(), record.token.id());

    let renewed = desktop_accept_reissue(&record, fresh, &desktop, 5).unwrap();
    assert_eq!(renewed.node_did, record.node_did, "same relationship");
    // And it outlives the one it replaced, which is the point of reissuing at all.
    assert!(renewed.token.claims().unwrap().exp > record.token.claims().unwrap().exp);
}

#[test]
fn a_reissue_meant_for_someone_else_is_not_kept() {
    let (node, desktop) = ids();
    let (stranger, _) = identity::generate();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let welcome = node_accept_claim(hello, &node, &mut Vec::new(), 3).unwrap();
    let record = desktop_finish_claim(&ticket, welcome, &desktop, 4).unwrap();

    let for_stranger = issue_claim_token(&node, stranger.did(), 5).unwrap();
    assert_eq!(
        desktop_accept_reissue(&record, for_stranger, &desktop, 5).unwrap_err(),
        CourierError::WrongHolder
    );
}

#[test]
fn a_printed_ticket_parses_back_and_still_claims() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();

    let text = ticket.to_text().unwrap();
    assert!(text.starts_with("osv1."));
    assert!(
        text.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
        "one word a shell needs no quoting for: {text}"
    );

    let parsed = ConnectionTicket::from_text(&text).unwrap();
    assert_eq!(parsed, ticket);
    // Copy-paste picks up whitespace. Nothing else is forgiven — the signature does not
    // survive a changed field anyway.
    assert_eq!(
        ConnectionTicket::from_text(&format!("  {text}\n")).unwrap(),
        ticket
    );

    // Still the ticket the node signed, after the round trip through text.
    let hello = desktop_start_claim(parsed, &desktop, 2).unwrap();
    assert!(node_accept_claim(hello, &node, &mut Vec::new(), 3).is_ok());
}

#[test]
fn a_ticket_from_a_version_we_do_not_know_is_refused_by_name() {
    let (node, _) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();

    // A newer node's ticket. Without the prefix this decoded as v1 and dropped whatever v2
    // added, because `verify_ticket` only checks the ticket and its claim against each other.
    let future = format!("osv2.{}", enc(serde_json::to_vec(&ticket).unwrap()));
    assert_eq!(
        ConnectionTicket::from_text(&future).unwrap_err(),
        CourierError::UnknownTicketVersion
    );

    // Right prefix, but the version inside disagrees with it.
    let mut lying = ticket.clone();
    lying.version = 2;
    let lying = format!("osv1.{}", enc(serde_json::to_vec(&lying).unwrap()));
    assert_eq!(
        ConnectionTicket::from_text(&lying).unwrap_err(),
        CourierError::UnknownTicketVersion
    );

    // Something that is not a ticket at all says so, rather than "decode failed".
    assert_eq!(
        ConnectionTicket::from_text("https://example.com/not-a-ticket").unwrap_err(),
        CourierError::UnknownTicketVersion
    );
}
