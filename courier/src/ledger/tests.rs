use identity::Identity;

use super::*;
use crate::token::{Scope, issue_root};

fn person() -> Identity {
    identity::generate().0
}

fn issued(owner: &Identity) -> Entry {
    let member = person();
    let token = issue_root(
        owner,
        member.did(),
        "member",
        Scope::Workspace("ws1".into()),
        false,
        1,
        1000,
    )
    .unwrap();
    Entry::Issued(token)
}

fn revoked() -> Entry {
    Entry::Revoked {
        id: [9; 32],
        at: 42,
    }
}

#[test]
fn both_entries_round_trip_under_the_owner() {
    let owner = person();
    for entry in [issued(&owner), revoked()] {
        let signed = sign(&owner, &entry).unwrap();
        assert_eq!(verify(&signed, owner.did()).unwrap(), entry);
    }
}

#[test]
fn another_owner_is_refused() {
    let owner = person();
    let other = person();
    let signed = sign(&owner, &revoked()).unwrap();
    assert_eq!(
        verify(&signed, other.did()),
        Err(CourierError::BadSignature)
    );
    assert_eq!(
        verify(&signed, "did:key:nonsense"),
        Err(CourierError::Decode)
    );
}

#[test]
fn a_touched_body_is_refused() {
    let owner = person();
    let mut signed = sign(&owner, &revoked()).unwrap();
    // Another id under the original signature.
    signed.body = bincode::serialize(&Entry::Revoked {
        id: [1; 32],
        at: 42,
    })
    .unwrap();
    assert_eq!(
        verify(&signed, owner.did()),
        Err(CourierError::BadSignature)
    );
}

#[test]
fn a_touched_signature_is_refused() {
    let owner = person();
    let mut signed = sign(&owner, &revoked()).unwrap();
    signed.sig[0] ^= 1;
    assert_eq!(
        verify(&signed, owner.did()),
        Err(CourierError::BadSignature)
    );
    signed.sig.truncate(10);
    assert_eq!(verify(&signed, owner.did()), Err(CourierError::Decode));
}

/// A valid signature over bytes that are not an entry is a decode error, not a verified one.
#[test]
fn a_signed_non_entry_does_not_decode() {
    let owner = person();
    let body = b"not an entry".to_vec();
    let signed = Signed {
        sig: owner.sign(&[DOMAIN, &body].concat()).to_vec(),
        body,
    };
    assert_eq!(verify(&signed, owner.did()), Err(CourierError::Decode));
}
