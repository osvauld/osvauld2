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

#[test]
fn bootstrap_claim_authenticates_and_reconnects() {
    let (node, desktop) = ids();
    let ticket = wire(issue_connection_ticket(&node, 1, "kunki").unwrap());
    let hello = desktop_start_claim(ticket.clone(), &desktop, 2).unwrap();
    let mut admins = Vec::new();

    let welcome = node_accept_claim(wire(hello), &node, &mut admins, 3).unwrap();
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0].did, desktop.did());

    let record = desktop_finish_claim(&ticket, wire(welcome), &desktop).unwrap();
    assert_eq!(record.node_did, node.did());
    assert_eq!(record.node_id, ticket.node_id);

    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, wire(challenge)).unwrap();
    assert!(node_accept_reconnect(wire(reconnect), &node, &admins, &mut challenges).is_ok());
}

#[test]
fn tampered_ticket_node_id_is_rejected() {
    let (node, desktop) = ids();
    let mut ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    ticket.node_id.push('x');

    assert_eq!(
        desktop_start_claim(ticket, &desktop, 2).unwrap_err(),
        CourierError::BadPermit
    );
}

#[test]
fn relationship_permit_binds_desktop_public_material() {
    let (node, desktop) = ids();
    let ticket = issue_connection_ticket(&node, 1, "kunki").unwrap();
    let mut hello = desktop_start_claim(ticket, &desktop, 2).unwrap();
    hello.desktop_device_key.push('x');
    let mut admins = Vec::new();

    assert_eq!(
        node_accept_claim(hello, &node, &mut admins, 3).unwrap_err(),
        CourierError::BadPermit
    );
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
    let record = desktop_finish_claim(&ticket, welcome, &desktop).unwrap();
    admins.clear();

    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();
    assert_eq!(
        node_accept_reconnect(reconnect, &node, &admins, &mut challenges).unwrap_err(),
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
    let record = desktop_finish_claim(&ticket, welcome, &desktop).unwrap();
    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let mut reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();
    reconnect.signature = enc([0u8; 64]);

    assert_eq!(
        node_accept_reconnect(reconnect, &node, &admins, &mut challenges).unwrap_err(),
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
    let record = desktop_finish_claim(&ticket, welcome, &desktop).unwrap();
    let mut challenges = Vec::new();
    let challenge = node_issue_reconnect_challenge(&node, &mut challenges);
    let reconnect = desktop_start_reconnect(&record, &desktop, challenge).unwrap();

    node_accept_reconnect(reconnect.clone(), &node, &admins, &mut challenges).unwrap();
    assert_eq!(
        node_accept_reconnect(reconnect, &node, &admins, &mut challenges).unwrap_err(),
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
        permit_for_desktop: issue_permit(&other_node, desktop.did(), "node.admin", 2, None, None)
            .unwrap(),
    };

    assert_eq!(
        desktop_finish_claim(&ticket, welcome, &desktop).unwrap_err(),
        CourierError::NodeMismatch
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
