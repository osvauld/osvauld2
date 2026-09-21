//! Opaque sealed records an account keeps for itself, outside the workspace/item tree.

use crate::VaultError;

const PREFIX: &str = "entry/";

/// Caller keys live under a reserved prefix, so an entry can never address the keystore, a
/// workspace, or an item however its name is spelled.
pub(crate) fn key(name: &str) -> Result<String, VaultError> {
    if name.is_empty() {
        return Err(VaultError::InvalidName(name.to_string()));
    }
    Ok(format!("{PREFIX}{name}"))
}

/// The scan prefix for `name`; an empty name enumerates every entry.
pub(crate) fn scan(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// Back to the caller's spelling — `None` for a key outside the namespace.
pub(crate) fn name_of(key: &str) -> Option<&str> {
    key.strip_prefix(PREFIX)
}
