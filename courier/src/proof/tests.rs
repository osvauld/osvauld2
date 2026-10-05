use identity::Identity;

use super::*;
use crate::CourierError;

const NOW: u64 = 1_000_000_000;

fn id() -> Identity {
    identity::generate().0
}

#[test]
fn a_request_signed_by_its_caller_verifies() {
    let (node, alice) = (id(), id());
    let body = br#"{"op":"Sync"}"#;
    let proof = prove(&alice, node.did(), body, NOW);
    verify(&proof, alice.did(), node.did(), body, NOW).unwrap();
}

#[test]
fn a_request_naming_alice_but_signed_by_bob_is_refused() {
    let (node, alice, bob) = (id(), id(), id());
    let body = br#"{"op":"Sync"}"#;
    let proof = prove(&bob, node.did(), body, NOW);
    assert!(matches!(
        verify(&proof, alice.did(), node.did(), body, NOW),
        Err(CourierError::BadSignature)
    ));
}

#[test]
fn a_proof_does_not_carry_over_to_a_different_body() {
    let (node, alice) = (id(), id());
    let proof = prove(&alice, node.did(), br#"{"op":"Sync","a":1}"#, NOW);
    let other = br#"{"op":"Sync","a":2}"#;
    assert!(matches!(
        verify(&proof, alice.did(), node.did(), other, NOW),
        Err(CourierError::BadSignature)
    ));
}

#[test]
fn a_proof_for_one_node_is_refused_by_another() {
    let (node, other, alice) = (id(), id(), id());
    let body = b"{}";
    let proof = prove(&alice, node.did(), body, NOW);
    assert!(matches!(
        verify(&proof, alice.did(), other.did(), body, NOW),
        Err(CourierError::BadSignature)
    ));
}

#[test]
fn a_proof_too_old_or_from_the_future_is_refused() {
    let (node, alice) = (id(), id());
    let body = b"{}";
    let old = prove(&alice, node.did(), body, NOW - WINDOW_MS - 1);
    let future = prove(&alice, node.did(), body, NOW + 1);
    for proof in [old, future] {
        assert!(matches!(
            verify(&proof, alice.did(), node.did(), body, NOW),
            Err(CourierError::StaleRequest)
        ));
    }
    let edge = prove(&alice, node.did(), body, NOW - WINDOW_MS);
    verify(&edge, alice.did(), node.did(), body, NOW).unwrap();
}

#[test]
fn a_request_id_is_admitted_once() {
    let (node, alice) = (id(), id());
    let proof = prove(&alice, node.did(), b"{}", NOW);
    let mut seen = Replay::new(NOW - 10);
    seen.admit(&proof, NOW).unwrap();
    assert!(matches!(
        seen.admit(&proof, NOW + 1),
        Err(CourierError::Replayed)
    ));
}

#[test]
fn a_request_stamped_at_or_before_the_node_started_is_refused() {
    // The replay set lives in memory: refusing anything stamped before it started is what
    // stops a request admitted before a restart from being admitted again after it.
    let (node, alice) = (id(), id());
    let mut seen = Replay::new(NOW);
    for ts in [NOW - 1, NOW] {
        let proof = prove(&alice, node.did(), b"{}", ts);
        assert!(matches!(
            seen.admit(&proof, NOW + 5),
            Err(CourierError::Replayed)
        ));
    }
}

#[test]
fn a_request_id_not_shaped_like_a_nonce_is_refused() {
    let (node, alice) = (id(), id());
    let mut proof = prove(&alice, node.did(), b"{}", NOW);
    proof.request_id = "x".repeat(4096);
    let mut seen = Replay::new(NOW - 10);
    assert!(matches!(seen.admit(&proof, NOW), Err(CourierError::Decode)));
}

#[test]
fn ids_older_than_the_window_are_forgotten() {
    let (node, alice) = (id(), id());
    let mut seen = Replay::new(NOW - 1);
    for i in 0..5 {
        let proof = prove(&alice, node.did(), b"{}", NOW + i);
        seen.admit(&proof, NOW + i).unwrap();
    }
    let later = NOW + 2 * WINDOW_MS;
    let proof = prove(&alice, node.did(), b"{}", later);
    seen.admit(&proof, later + 1).unwrap();
    assert_eq!(seen.len(), 1);
}
