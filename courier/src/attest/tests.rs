use super::*;
use crate::enc;

fn person() -> identity::Identity {
    identity::generate().0
}

#[test]
fn attestation_opens_to_the_signer_and_device() {
    let me = person();
    let device = [7u8; 32];
    let token = attest(&me, device, 1700).unwrap();
    let claim = verify_attestation(&token).unwrap();
    assert_eq!(claim.did, me.did());
    assert_eq!(claim.device, device);
    assert_eq!(claim.iat, 1700);
}

#[test]
fn a_rewritten_did_is_refused() {
    let me = person();
    let other = person();
    let token = attest(&me, [1u8; 32], 1).unwrap();
    // Re-encode the payload naming someone else under the original signature.
    let signed: crate::SignedBlob = serde_json::from_slice(&dec(&token).unwrap()).unwrap();
    let mut claim: Attestation = bincode::deserialize(&dec(&signed.payload).unwrap()).unwrap();
    claim.did = other.did().to_string();
    let forged = crate::SignedBlob {
        signature: signed.signature,
        payload: enc(bincode::serialize(&claim).unwrap()),
    };
    let forged = enc(serde_json::to_vec(&forged).unwrap());
    assert_eq!(verify_attestation(&forged), Err(CourierError::BadSignature));
}

#[test]
fn a_changed_device_is_refused() {
    let me = person();
    let token = attest(&me, [1u8; 32], 1).unwrap();
    let signed: crate::SignedBlob = serde_json::from_slice(&dec(&token).unwrap()).unwrap();
    let mut claim: Attestation = bincode::deserialize(&dec(&signed.payload).unwrap()).unwrap();
    claim.device = [2u8; 32];
    let forged = crate::SignedBlob {
        signature: signed.signature,
        payload: enc(bincode::serialize(&claim).unwrap()),
    };
    let forged = enc(serde_json::to_vec(&forged).unwrap());
    assert_eq!(verify_attestation(&forged), Err(CourierError::BadSignature));
}

#[test]
fn garbage_is_a_decode_error() {
    assert_eq!(verify_attestation("not a token"), Err(CourierError::Decode));
}
