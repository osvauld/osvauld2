use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

pub fn public_key(secret: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(secret).verifying_key().to_bytes()
}

pub fn sign(secret: &[u8; 32], message: &[u8]) -> [u8; 64] {
    SigningKey::from_bytes(secret).sign(message).to_bytes()
}

pub fn verify(public: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let Ok(verifying_key) = VerifyingKey::from_bytes(public) else {
        return false;
    };
    // Strict: refuses the malleable and small-order forms a plain `verify` lets through, so
    // one signature has one byte form — what a spent-nonce set or a hash over a signed
    // statement relies on.
    verifying_key
        .verify_strict(message, &Signature::from_bytes(signature))
        .is_ok()
}

#[cfg(test)]
mod tests;
