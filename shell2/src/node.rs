//! shell2's dial-out to a kunki node's bridge (`kunki::bridge`) — the desktop half of that
//! protocol. One connection per call, mirroring the node's own "one connection, one request,
//! one reply" shape; nothing here is long-lived like `bridge.rs`'s own listener.

use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use courier::invite::{InviteRequest, InviteTicket, InviteWelcome, desktop_start_invite_claim};
use courier::publish::{PublishAck, PublishedItem, PublishedWorkspace, desktop_publish};
use courier::role::RoleRequest;
use courier::subscribe::desktop_start_subscribe;
use courier::sync::{SyncAck, SyncHello, SyncLayer, desktop_start_sync, item_scope};
use courier::token::{Scope, Token};
use courier::{ClaimWelcome, ConnectionTicket, DesktopNodeRecord};
use kunki::bridge::{Envelope, Request};
use kunki::push::Push;
use loro::{ExportMode, LoroDoc};
use osvauld_rpc::Response;
use vault::{Vault, WorkspaceItem, WorkspaceMeta};

/// Same convention as `kunki::bridge`'s own `now_secs`, independently, because this side has
/// no `Admin` to hang it on.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs()
}

/// One entry, one node: today's build claims a single node per account, so there is exactly
/// one relationship to remember rather than a table keyed by node did.
const RELATIONSHIP_KEY: &str = "node/relationship";

pub fn save_relationship(vault: &Vault, record: &DesktopNodeRecord) -> Result<(), String> {
    let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    vault
        .put_entry(RELATIONSHIP_KEY, &bytes)
        .map_err(|e| e.to_string())
}

/// Saves only if `did` is still the active account: a claim runs off the UI thread, and the
/// account may have switched while it was in flight.
fn save_as(vault: &Vault, did: &str, record: &DesktopNodeRecord) -> Result<(), String> {
    if vault.with_signer(|d| d.did() == did) != Some(true) {
        return Err("account changed during the claim".into());
    }
    save_relationship(vault, record)
}

pub fn load_relationship(vault: &Vault) -> Result<Option<DesktopNodeRecord>, String> {
    let Some(bytes) = vault
        .get_entry(RELATIONSHIP_KEY)
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// See `kunki::bridge::READ_TIMEOUT`: a stalled node must not wedge the shell's UI thread.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// For the requests that name no caller: `Ping` and the two claims.
fn call(socket: &Path, req: &Request) -> Result<Response, String> {
    send(socket, &Envelope::plain(req)?)
}

/// Signed as the vault's account, for the node this account has claimed (`courier::proof`).
fn call_as(socket: &Path, vault: &Vault, req: &Request) -> Result<Response, String> {
    send(socket, &seal(vault, req)?)
}

fn seal(vault: &Vault, req: &Request) -> Result<Envelope, String> {
    let node_did = load_relationship(vault)?
        .ok_or("this account has not joined a node")?
        .node_did;
    vault
        .with_signer(|me| Envelope::signed(req, me, &node_did, kunki::bridge::now_ms()))
        .ok_or("account is locked")?
}

fn send(socket: &Path, env: &Envelope) -> Result<Response, String> {
    let mut conn = UnixStream::connect(socket).map_err(|e| e.to_string())?;
    conn.set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let payload = serde_json::to_vec(env).map_err(|e| e.to_string())?;
    osvauld_rpc::write_msg(&mut conn, &payload).map_err(|e| e.to_string())?;
    let bytes = osvauld_rpc::read_msg(&mut conn).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn unpack<T: serde::de::DeserializeOwned>(resp: Response) -> Result<T, String> {
    match resp {
        Response::Ok { result } => serde_json::from_value(result).map_err(|e| e.to_string()),
        Response::Err { message } => Err(message),
    }
}

/// Liveness probe — also how a caller waits for a just-spawned node's socket to exist.
pub fn ping(socket: &Path) -> bool {
    matches!(call(socket, &Request::Ping), Ok(Response::Ok { .. }))
}

/// Redeem a node's boot ticket as its first admin. `vault` signs as the local account without
/// this module ever holding its key — the same discipline `kunki::main`'s own boot ticket
/// keeps on the other side of this exchange.
pub fn claim(
    socket: &Path,
    vault: &Vault,
    ticket: &ConnectionTicket,
    now: u64,
) -> Result<DesktopNodeRecord, String> {
    let hello = vault
        .with_signer(|desktop| courier::desktop_start_claim(ticket.clone(), desktop, now))
        .ok_or("account is locked")?
        .map_err(|e| e.to_string())?;
    let me = hello.desktop_did.clone();
    let welcome: ClaimWelcome = unpack(call(socket, &Request::Claim(hello))?)?;
    let record = vault
        .with_signer(|desktop| courier::desktop_finish_claim(ticket, welcome, desktop, now))
        .ok_or("account is locked")?
        .map_err(|e| e.to_string())?;
    // Saved here, not by the caller: every later request is signed for this node's DID.
    save_as(vault, &me, &record)?;
    Ok(record)
}

/// Announce a workspace and its item headers to the claimed node. Idempotent: `Admin::accept_publish`
/// adopts by id, so calling this again with the same workspace is how a later item gets added
/// rather than something only the first call may do.
pub fn publish(
    socket: &Path,
    vault: &Vault,
    token: Token,
    workspace: PublishedWorkspace,
    items: Vec<PublishedItem>,
) -> Result<PublishAck, String> {
    let did = vault
        .with_signer(|desktop| desktop.did().to_string())
        .ok_or("account is locked")?;
    let hello = desktop_publish(&did, token, workspace, items);
    unpack(call_as(socket, vault, &Request::Publish(hello))?)
}

/// Ask the claimed node to mint an invite for someone else to redeem at `role`/`scope`. Only
/// the first desktop ever claims a node (`Admin::accept_claim` refuses a second admin); every
/// later member arrives through this and [`claim_invite`] instead.
pub fn invite(
    socket: &Path,
    vault: &Vault,
    token: Token,
    role: &str,
    scope: Scope,
) -> Result<InviteTicket, String> {
    let did = vault
        .with_signer(|desktop| desktop.did().to_string())
        .ok_or("account is locked")?;
    let request = InviteRequest {
        desktop_did: did,
        token,
        role: role.to_string(),
        scope,
        public: false,
    };
    unpack(call_as(socket, vault, &Request::Invite(request))?)
}

/// Redeem an invite minted by [`invite`]. No `desktop_finish_invite_claim` exists — unlike
/// `ClaimWelcome`, `InviteWelcome` carries no separate verification step — so the record is
/// built directly from what the ticket and the welcome each already vouch for.
pub fn claim_invite(
    socket: &Path,
    vault: &Vault,
    ticket: InviteTicket,
    now: u64,
) -> Result<DesktopNodeRecord, String> {
    let hello = vault
        .with_signer(|desktop| desktop_start_invite_claim(ticket.clone(), desktop, now))
        .ok_or("account is locked")?
        .map_err(|e| e.to_string())?;
    let me = hello.desktop_did.clone();
    let welcome: InviteWelcome = unpack(call(socket, &Request::ClaimInvite(hello))?)?;
    let record = DesktopNodeRecord {
        node_did: welcome.node_did,
        node_id: ticket.node_id,
        node_encryption_key: ticket.node_encryption_key,
        token: welcome.token,
    };
    save_as(vault, &me, &record)?;
    Ok(record)
}

/// What one text box accepts: a `ConnectionTicket` (`osv1.`) claims an unowned node, an
/// `InviteTicket` (`osvi1.`) joins one, and anything else shaped like a name is looked up
/// on-chain for the public invite it points at. The prefixes tell them apart on sight.
pub fn join(
    socket: &Path,
    vault: &Vault,
    text: &str,
    now: u64,
) -> Result<DesktopNodeRecord, String> {
    if let Ok(ticket) = ConnectionTicket::from_text(text) {
        return claim(socket, vault, &ticket, now);
    }
    let invite = match InviteTicket::from_text(text) {
        Ok(ticket) => ticket,
        Err(_) if crate::names::is_name(text) => {
            InviteTicket::from_text(&crate::names::resolve(text)?)
                .map_err(|_| format!("'{text}' does not point at an invite"))?
        }
        Err(_) => return Err("not a node ticket, invite or name".to_string()),
    };
    claim_invite(socket, vault, invite, now)
}

/// The socket round trip only — `hello` is built against a live `LoroDoc` before this call
/// (`courier::desktop_start_sync` needs the doc itself, not bytes), and the ack's `update` is
/// imported back into that same doc after. Both of those touch a doc that is not `Send`; this
/// function is the part of the exchange that is, so a caller offloads only this to a thread.
pub fn sync(socket: &Path, vault: &Vault, hello: SyncHello) -> Result<SyncAck, String> {
    unpack(call_as(socket, vault, &Request::Sync(hello))?)
}

/// Pull whatever the node already has for one doc layer, best-effort: `None` on any failure —
/// network, authorization, or the node genuinely having nothing for it — collapsed into one
/// outcome because the caller's fallback is the same either way. This is what stops a second
/// desktop's app from mistaking "nobody has synced this doc to *me* yet" for "this doc has
/// never been written," and re-running its own first-run seeding logic against content someone
/// else already created — see `resolver_with_node` in `shell2/src/main.rs`, the caller.
pub fn pull_doc(
    socket: &Path,
    vault: &Vault,
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &str,
    name: &str,
) -> Option<Vec<u8>> {
    let doc = LoroDoc::new();
    let hello = desktop_start_sync(
        desktop_did,
        token,
        ws_id,
        item_id,
        SyncLayer::Doc(name.to_string()),
        &doc,
        None,
    )
    .ok()?;
    let ack = sync(socket, vault, hello).ok()?;
    doc.import(&ack.update).ok()?;
    // Checked on the doc's own oplog, not `ack.update`'s byte length: whether an empty diff
    // serializes to zero bytes is a Loro encoding detail, not something to depend on. A doc
    // nobody has ever written to has an empty oplog regardless of that encoding.
    if doc.oplog_vv().is_empty() {
        return None;
    }
    let bytes = doc.export(ExportMode::Snapshot).ok()?;
    // Best-effort cache: the caller already has `bytes` even if this fails.
    let _ = vault.put_doc(ws_id, item_id, &bytes, name);
    Some(bytes)
}

/// Adopt a workspace/item another desktop already published — its exact ids, learned from
/// that desktop's own `CreateWorkspace`/`CreateItem` replies rather than a node-side "what do
/// you hold" query, which does not exist yet — and pull the item's current source from the
/// node. `open_tab` needs both: `get_src` returning content, and a local `WorkspaceItem` to
/// find in the first place. Sync alone provides neither for a doc that has never been open.
pub fn join_item(
    socket: &Path,
    vault: &Vault,
    token: Token,
    ws: WorkspaceMeta,
    item: WorkspaceItem,
) -> Result<(), String> {
    vault.adopt_workspace(&ws).map_err(|e| e.to_string())?;
    vault.adopt_item(&item).map_err(|e| e.to_string())?;

    let did = vault
        .with_signer(|d| d.did().to_string())
        .ok_or("account is locked")?;
    let doc = LoroDoc::new();
    let hello = desktop_start_sync(&did, token, &ws.id, &item.id, SyncLayer::Src, &doc, None)
        .map_err(|e| e.to_string())?;
    let ack = sync(socket, vault, hello)?;
    doc.import(&ack.update).map_err(|e| e.to_string())?;
    let bytes = doc
        .export(ExportMode::Snapshot)
        .map_err(|e| e.to_string())?;
    vault
        .put_src(&ws.id, &item.id, &bytes)
        .map_err(|e| e.to_string())
}

/// Push this item's current source snapshot to the claimed node — a one-shot `Sync`, not the
/// continuous kind [`join_item`] relies on nothing else providing yet.
pub fn push_src(
    socket: &Path,
    vault: &Vault,
    token: Token,
    ws_id: &str,
    item_id: &str,
) -> Result<(), String> {
    let bytes = vault
        .get_src(ws_id, item_id)
        .map_err(|e| e.to_string())?
        .ok_or("no local source for this item")?;
    let doc = LoroDoc::new();
    doc.import(&bytes).map_err(|e| e.to_string())?;
    let did = vault
        .with_signer(|d| d.did().to_string())
        .ok_or("account is locked")?;
    let hello = desktop_start_sync(&did, token, ws_id, item_id, SyncLayer::Src, &doc, None)
        .map_err(|e| e.to_string())?;
    sync(socket, vault, hello).map(|_ack| ())
}

/// Declare interest in one item's layer — what makes a future push for it reach this desktop,
/// once a [`listen`] connection is open to carry it. Idempotent on the node side
/// (`Admin::subscribe` is a plain key write), so calling this more than once for the same
/// layer costs nothing beyond the round trip.
pub fn subscribe(
    socket: &Path,
    vault: &Vault,
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &str,
    layer: SyncLayer,
) -> Result<(), String> {
    let hello = desktop_start_subscribe(desktop_did, token, ws_id, item_id, layer);
    unpack(call_as(socket, vault, &Request::Subscribe(hello))?)
}

pub fn unsubscribe(
    socket: &Path,
    vault: &Vault,
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &str,
    layer: SyncLayer,
) -> Result<(), String> {
    let hello = desktop_start_subscribe(desktop_did, token, ws_id, item_id, layer);
    unpack(call_as(socket, vault, &Request::Unsubscribe(hello))?)
}

/// Give `to` an app role on one item, or with `revoke` take it back. The node decides from
/// this account's grants on record; `token` only proves membership.
#[allow(clippy::too_many_arguments)]
pub fn change_role(
    socket: &Path,
    vault: &Vault,
    token: Token,
    ws_id: &str,
    item_id: &str,
    to: &str,
    role: &str,
    revoke: bool,
) -> Result<(), String> {
    let desktop_did = vault
        .with_signer(|d| d.did().to_string())
        .ok_or("account is locked")?;
    let req = RoleRequest {
        desktop_did,
        token,
        to: to.to_string(),
        role: role.to_string(),
        scope: item_scope(ws_id, item_id),
    };
    let req = if revoke {
        Request::RevokeRole(req)
    } else {
        Request::AssignRole(req)
    };
    match call_as(socket, vault, &req)? {
        Response::Ok { .. } => Ok(()),
        Response::Err { message } => Err(message),
    }
}

/// Open the channel pushes travel on and wait for the node's ack. Deliberately not a loop
/// itself — a caller reads pushes off the returned connection with [`next_push`], normally in
/// a loop on its own thread, so establishing the connection (with its own retry-on-drop
/// policy) and consuming it stay two separate concerns.
pub fn listen(
    socket: &Path,
    vault: &Vault,
    desktop_did: &str,
    token: Token,
) -> Result<UnixStream, String> {
    let req = Request::Listen {
        desktop_did: desktop_did.to_string(),
        token,
    };
    let env = seal(vault, &req)?;
    let mut conn = UnixStream::connect(socket).map_err(|e| e.to_string())?;
    let payload = serde_json::to_vec(&env).map_err(|e| e.to_string())?;
    osvauld_rpc::write_msg(&mut conn, &payload).map_err(|e| e.to_string())?;
    let bytes = osvauld_rpc::read_msg(&mut conn).map_err(|e| e.to_string())?;
    match serde_json::from_slice(&bytes).map_err(|e| e.to_string())? {
        Response::Ok { .. } => Ok(conn),
        Response::Err { message } => Err(message),
    }
}

/// Blocks for the next `Push` on an already-open [`listen`] connection.
pub fn next_push(conn: &mut UnixStream) -> Result<Push, String> {
    let bytes = osvauld_rpc::read_msg(conn).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
