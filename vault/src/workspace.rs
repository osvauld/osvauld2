//! Workspaces: the top-level containers a user creates inside their account — a "space" or
//! virtual desktop in the shell. A workspace is just a namespace in the account store
//! (`ws/<id>/…`) plus a small sealed metadata record at `ws/<id>/meta`. There is no registry
//! document: the set of workspaces *is* the set of `meta` keys, found by a prefix scan
//! (`Store::list_prefixed`). CRDT layers only enter later, for the
//! collaborative *content* a workspace holds (`.doc` files, members), never for this header.

use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngCore;
use serde::{Deserialize, Serialize};

/// A workspace's header: an opaque random `id`, a human `name`, and a creation time
/// (unix seconds). Sealed and stored at `ws/<id>/meta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    pub id: String,
    pub name: String,
    pub created: u64,
}

/// Prefix every workspace key shares, for the listing scan.
pub(crate) const WS_PREFIX: &str = "ws/";

/// The key a workspace's sealed header lives at.
pub(crate) fn meta_key(id: &str) -> String {
    format!("ws/{id}/meta")
}

/// The workspace id encoded by a `ws/<id>/meta` key, or `None` if `key` is some other key
/// under `ws/` (e.g. a nested `ws/<id>/file/<fid>` content key). A real id has no slash, so
/// requiring exactly `ws/<id>/meta` rejects those.
pub(crate) fn id_from_meta_key(key: &str) -> Option<&str> {
    let id = key.strip_prefix(WS_PREFIX)?.strip_suffix("/meta")?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

/// A fresh opaque workspace id: 16 random bytes, hex-encoded.
pub(crate) fn new_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().fold(String::with_capacity(32), |mut acc, byte| {
        use std::fmt::Write;
        let _ = write!(acc, "{byte:02x}");
        acc
    })
}

/// Unix seconds now, or 0 if the clock is before the epoch (it isn't).
pub(crate) fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
