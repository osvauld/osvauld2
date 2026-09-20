use thiserror::Error;

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("set OSVAULD_KUNKI_PASSPHRASE to create or unlock the node identity")]
    NoPassphrase,
    #[error("the node account is locked")]
    Locked,
    // Which of them is the node? Guessing would sign tickets as the wrong identity.
    #[error("{0} accounts in the node directory; expected exactly one")]
    ManyAccounts(usize),
    // The store says something impossible: an index with no record behind it, or a name that
    // is not an id. Reading past it would under-report the audit log or the revoked set.
    #[error("admin record {0} is damaged")]
    Damaged(String),
    #[error(transparent)]
    Vault(#[from] vault::VaultError),
    #[error(transparent)]
    Courier(#[from] courier::CourierError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
