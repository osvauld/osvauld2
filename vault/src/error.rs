use identity::{CryptoError, IdentityError};
use storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("wrong passphrase")]
    WrongPassphrase,
    #[error("no account for {0}")]
    NoSuchAccount(String),
    #[error("account has no keystore")]
    NoKeystore,
    #[error("not a valid did: {0}")]
    BadDid(String),
    #[error("vault is locked")]
    Locked,
    #[error("not a valid name: {0}")]
    InvalidName(String),
    #[error(transparent)]
    Identity(IdentityError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// Lift a wrong-passphrase unlock to the top-level variant so a driver matches one case;
// every other identity failure passes through transparently.
impl From<IdentityError> for VaultError {
    fn from(error: IdentityError) -> Self {
        match error {
            IdentityError::WrongPassphrase => VaultError::WrongPassphrase,
            other => VaultError::Identity(other),
        }
    }
}
