//! Sync: a desktop pushing local Loro changes for one item's layer to its home node, and
//! learning back what it doesn't have. One request, one round trip — the desktop states its
//! own current version vector rather than the node trying to remember a peer's progress
//! (`docs/design/osvauld1-prior-art.md` §8: the old implementation advanced a subscriber's
//! recorded vector the instant a message was *enqueued*, not delivered, so a dropped
//! connection left it believing a peer was caught up when it never applied anything).
//!
//! Merge only, never replace. The node's copy is authoritative by construction — it is the
//! workspace's home node — so unlike the old implementation there is no "divergence" that
//! needs a destructive fallback: every import is an ordinary CRDT merge, in both directions.
//!
//! `courier` holds the Loro dependency, not `vault` (which stays Loro-free by design) or
//! `kunki` — this stays the pure-message-transition layer publish/invite already are, just
//! extended to bytes that happen to be Loro's rather than a snapshot the caller never opens.
//! The caller supplies the node's on-disk snapshot and persists what comes back; sync itself
//! touches no storage.

use loro::{ExportMode, LoroDoc, VersionVector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::access::{Access, doc_address};
use crate::policy;
use crate::token::{Scope, Token};
use crate::{CourierError, Result, nonce};

/// Which of an item's two content keys this sync is for (`vault::item::src_key`/`doc_key`) —
/// typed here so a malformed name can't silently address the wrong one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncLayer {
    Src,
    Doc(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHello {
    pub request_id: String,
    pub desktop_did: String,
    pub token: Token,
    pub ws_id: String,
    pub item_id: String,
    pub layer: SyncLayer,
    /// The desktop's own encoded version vector, taken *after* the edits `update` carries —
    /// what the node diffs its own copy against to find what this push still doesn't cover.
    pub vv: Vec<u8>,
    /// Loro update bytes; empty when the desktop has nothing new to push (a pure pull).
    pub update: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAck {
    pub request_id: String,
    /// Everything the node's copy has beyond `hello.vv` — importing this converges the
    /// desktop with the node, including whatever a third party had already pushed.
    pub update: Vec<u8>,
    /// The node's version vector after the merge — what the desktop's next push can be a diff
    /// against, since the node provably holds everything up to it.
    pub vv: Vec<u8>,
    /// The node still lacks part of `hello.vv` after the merge: the desktop's `since` was ahead
    /// of what the node really holds (a restored node, a causally incomplete diff). The desktop
    /// answers by pushing full history.
    pub missing: bool,
}

/// Build the desktop's half: commit pending edits, then export only what changed since `since`
/// (`None` — this layer has never synced with this node before — pushes full history). `token`/
/// `desktop_did` are the desktop's own claim/reconnect grant, presented as-is — building this
/// message signs nothing, same as `desktop_publish`.
pub fn desktop_start_sync(
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &str,
    layer: SyncLayer,
    doc: &LoroDoc,
    since: Option<&[u8]>,
) -> Result<SyncHello> {
    doc.commit();
    let since_vv = match since {
        Some(bytes) => VersionVector::decode(bytes).map_err(|_| CourierError::Decode)?,
        None => VersionVector::default(),
    };
    let update = doc
        .export(ExportMode::updates(&since_vv))
        .map_err(|_| CourierError::Decode)?;
    Ok(SyncHello {
        request_id: nonce(),
        desktop_did: desktop_did.to_string(),
        token,
        ws_id: ws_id.to_string(),
        item_id: item_id.to_string(),
        layer,
        vv: doc.oplog_vv().encode(),
        update,
    })
}

/// What a sync or subscription acts on: the item as an app instance. Node and workspace
/// grants contain it; an app grant contains only its own item.
pub fn item_scope(ws_id: &str, item_id: &str) -> Scope {
    Scope::App {
        ws: ws_id.to_string(),
        app: item_id.to_string(),
    }
}

/// Whether a grant held at `scope` reaches this layer: a doc through its item or its own
/// address; the source through the item, or any address inside it (a guest on one doc still
/// needs the app to open it).
pub fn reaches(scope: &Scope, ws_id: &str, item_id: &str, layer: &SyncLayer) -> bool {
    let item = item_scope(ws_id, item_id);
    match layer {
        SyncLayer::Doc(name) => doc_scope(ws_id, item_id, name).is_some_and(|d| scope.contains(&d)),
        SyncLayer::Src => {
            scope.contains(&item) || (matches!(scope, Scope::Resource(_)) && item.contains(scope))
        }
    }
}

/// A doc's own address as a scope. `None` for a name that isn't a doc name: such a doc reaches
/// no one, rather than falling back to the item.
fn doc_scope(ws_id: &str, item_id: &str, name: &str) -> Option<Scope> {
    doc_address(ws_id, item_id, name).map(Scope::Resource)
}

/// The scope the presented token must reach for this layer — see [`reaches`].
pub fn layer_target(token: &Token, ws_id: &str, item_id: &str, layer: &SyncLayer) -> Result<Scope> {
    Ok(match layer {
        SyncLayer::Doc(name) => doc_scope(ws_id, item_id, name).ok_or(CourierError::BadScope)?,
        SyncLayer::Src => {
            let held = token.claims()?.scope;
            if matches!(held, Scope::Resource(_)) && reaches(&held, ws_id, item_id, layer) {
                held
            } else {
                item_scope(ws_id, item_id)
            }
        }
    })
}

/// Authorize, merge, and diff back. `current_snapshot` is the node's own stored snapshot for
/// this item/layer (`None` the first time this layer is ever synced), read by the caller
/// before this call; on success the second element is the new snapshot for the caller to
/// persist in its place.
///
/// The presented token proves membership over the layer ([`layer_target`]). `access` is the
/// manifest's verdict for this caller on this doc (`crate::access`), decided by the node from
/// its records before the call; the source layer passes [`Access::OPEN`]. Reading is needed
/// for any sync of a doc the node holds with content; one it doesn't (or holds empty)
/// discloses nothing, so its creator needs only write. Writing is needed for anything that
/// changes the node's copy. A doc name with no address is refused (`BadScope`).
pub fn node_accept_sync(
    hello: &SyncHello,
    node_did: &str,
    current_snapshot: Option<&[u8]>,
    access: Access,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<(SyncAck, Vec<u8>)> {
    policy::membership(
        &hello.token,
        node_did,
        &hello.desktop_did,
        &layer_target(&hello.token, &hello.ws_id, &hello.item_id, &hello.layer)?,
        now,
        revoked,
    )?;
    let doc = LoroDoc::new();
    if let Some(bytes) = current_snapshot {
        doc.import(bytes).map_err(|_| CourierError::Decode)?;
    }
    let before = doc.oplog_vv();
    // Opening a doc syncs it before anything is written, so the node may hold an empty copy.
    if !before.is_empty() && !access.read {
        return Err(CourierError::NoRead);
    }
    if !hello.update.is_empty() {
        doc.import(&hello.update)
            .map_err(|_| CourierError::Decode)?;
    }
    // An app's source carries its manifest — its roles and rules — so changing it is
    // installing the app, not using it. Pulling it stays open to every member.
    let changed = doc.oplog_vv() != before;
    if changed && !access.write {
        return Err(CourierError::NoWrite);
    }
    if hello.layer == SyncLayer::Src && changed {
        policy::authorize(
            &hello.token,
            node_did,
            &hello.desktop_did,
            policy::Capability::AppInstall,
            &item_scope(&hello.ws_id, &hello.item_id),
            now,
            revoked,
        )?;
    }

    let their_vv = VersionVector::decode(&hello.vv).map_err(|_| CourierError::Decode)?;
    let ours = doc.oplog_vv();
    let diff = doc
        .export(ExportMode::updates(&their_vv))
        .map_err(|_| CourierError::Decode)?;
    let snapshot = doc
        .export(ExportMode::Snapshot)
        .map_err(|_| CourierError::Decode)?;

    Ok((
        SyncAck {
            request_id: hello.request_id.clone(),
            update: diff,
            vv: ours.encode(),
            missing: !ours.includes_vv(&their_vv),
        },
        snapshot,
    ))
}

#[cfg(test)]
mod tests;
