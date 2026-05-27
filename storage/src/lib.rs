//! Lite embedded key-value store: a single ordered keyspace over redb, raw bytes in
//! and out. Keys are sortable path strings (e.g. `"space/page/layer/shard"`); the
//! "namespace" is just a prefix convention inside the key. No serialization, no crypto,
//! no dirty-tracking — those all live in the layers above this one.

mod error;
mod store;

pub use error::StorageError;
pub use store::Store;
