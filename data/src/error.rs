use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataError {
    #[error(transparent)]
    Storage(#[from] storage::StorageError),
    /// Serializing a layer's CRDT snapshot for storage failed.
    #[error(transparent)]
    Encode(#[from] loro::LoroEncodeError),
    /// Loading a stored snapshot back into a `LoroDoc` failed (corrupt or
    /// incompatible bytes).
    #[error(transparent)]
    Import(#[from] loro::LoroError),
}
