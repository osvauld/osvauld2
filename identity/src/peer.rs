//! A device's Loro peer ID, derived from its device key (`owned-entries.md` §2.1): every
//! change a device writes is stamped with it, so a doc records which device made each change
//! and which device created each entry. The key is per install (`vault.device_public_key()`,
//! the iroh endpoint id) — never [`Identity::device_public_key`], which the phrase derives and
//! every install of one account shares; two writers under one peer corrupt a doc.

use cryptography::kdf::hkdf_sha256;

/// Loro's `PeerID` is a `u64`; `u64::MAX` is the one value it refuses.
pub fn peer_of(device_public_key: &[u8; 32]) -> u64 {
    let okm = hkdf_sha256(device_public_key, None, b"osvauld-peer");
    let peer = u64::from_le_bytes(okm[..8].try_into().expect("8 bytes"));
    if peer == u64::MAX { peer - 1 } else { peer }
}

#[cfg(test)]
mod tests {
    use super::peer_of;

    #[test]
    fn a_device_key_maps_to_one_stable_peer_that_loro_accepts() {
        let a = [7u8; 32];
        let b = [8u8; 32];
        assert_eq!(peer_of(&a), peer_of(&a));
        assert_ne!(peer_of(&a), peer_of(&b));
        assert_ne!(peer_of(&a), u64::MAX);
    }
}
