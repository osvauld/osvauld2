//! The single **home document**, persisted encrypted in the unlocked account's store.
//!
//! A `.doc` is one Loro [`Doc`]; we keep one for now, under a fixed key in the account's
//! `Store`. At rest it's a Loro snapshot **ECIES-sealed to the account's own identity**
//! (so only this identity can open it) — reusing the existing public crypto, no new key
//! material. The same `Doc` is the merge point that the network (courier) will feed later.

use doc_editor::Doc;
use vault::Vault;

/// Where the one home document lives in the account store. (A real file namespace —
/// `space/page/file` — comes with the explorer; one fixed key is enough for now.)
const HOME_DOC_KEY: &str = "file/home.doc";

/// Load the home document from the vault, or create a fresh one. Any read/decrypt/decode
/// failure falls back to a new document (and logs) rather than losing the session.
pub fn load_or_create(vault: &Vault) -> Doc {
    if let (Some(store), Some(identity)) = (vault.store(), vault.identity()) {
        match store.get(HOME_DOC_KEY) {
            Ok(Some(sealed)) => match identity.decrypt_sealed(&sealed) {
                Ok(snapshot) => match Doc::from_snapshot(&snapshot) {
                    Ok(doc) => return doc,
                    Err(e) => eprintln!("home doc: snapshot decode failed: {e:?}"),
                },
                Err(e) => eprintln!("home doc: decrypt failed: {e:?}"),
            },
            Ok(None) => {} // nothing stored yet — first run for this account
            Err(e) => eprintln!("home doc: store read failed: {e:?}"),
        }
    }
    Doc::new()
}

/// Persist the home document: snapshot → ECIES-seal to our own identity → store. A no-op
/// when locked.
pub fn save(vault: &Vault, doc: &Doc) {
    let (Some(store), Some(identity)) = (vault.store(), vault.identity()) else {
        return;
    };
    let snapshot = doc.export_snapshot();
    match identity::encrypt_for(&identity.encryption_public_key(), &snapshot) {
        Ok(sealed) => {
            if let Err(e) = store.put(HOME_DOC_KEY, &sealed) {
                eprintln!("home doc: store write failed: {e:?}");
            }
        }
        Err(e) => eprintln!("home doc: encrypt failed: {e:?}"),
    }
}
