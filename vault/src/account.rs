use std::path::Path;

use crate::error::VaultError;

pub struct AccountInfo {
    pub did: String,
    pub label: String,
}

const DID_PREFIX: &str = "did:key:";
const DB_EXT: &str = "redb";

// An account's db is named after its DID with the `did:key:` prefix stripped: the
// remaining multibase (`z…`, base58btc) is filesystem-safe on every OS.
pub(crate) fn did_to_filename(did: &str) -> Result<String, VaultError> {
    let multibase = did
        .strip_prefix(DID_PREFIX)
        .ok_or_else(|| VaultError::BadDid(did.to_string()))?;
    Ok(format!("{multibase}.{DB_EXT}"))
}

// Reverse of `did_to_filename`, returning None for any path that isn't a well-formed
// account db (wrong extension, or a stem that doesn't decode to an ed25519 did:key).
fn filename_to_did(path: &Path) -> Option<String> {
    if path.extension()?.to_str()? != DB_EXT {
        return None;
    }
    let did = format!("{DID_PREFIX}{}", path.file_stem()?.to_str()?);
    identity::public_key_from_did(&did).map(|_| did)
}

// List the DIDs of every account in `dir`. A missing directory means "no accounts yet".
pub(crate) fn scan_dids(dir: &Path) -> Result<Vec<String>, VaultError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut dids = Vec::new();
    for entry in entries {
        if let Some(did) = filename_to_did(&entry?.path()) {
            dids.push(did);
        }
    }
    Ok(dids)
}
