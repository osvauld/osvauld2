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
    #[error("not an item kind this build understands: {0}")]
    BadItemKind(String),
    // Roles are read from the app's manifest; without its source the node cannot know them.
    #[error("no app source on this node for {0}")]
    NoManifest(String),
    #[error("app source for {0} is unreadable")]
    BadSource(String),
    #[error(transparent)]
    Manifest(#[from] manifest::Error),
    #[error("this invite has no recorded inviter; ask for a new one")]
    UntrackedInvite,
    #[error(transparent)]
    Vault(#[from] vault::VaultError),
    #[error(transparent)]
    Courier(#[from] courier::CourierError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
