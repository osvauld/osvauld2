//! shell2's side of the node protocol — the desktop half of `kunki::bridge`. A [`Link`] says
//! where a request goes: straight into this account's own `kunki::admin` when the account
//! hosts for itself (laptop-host step 4), or over a kunki node's unix socket when it claimed
//! one. One request, one reply either way; nothing here is long-lived except what [`listen`]
//! hands back.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use courier::invite::{InviteRequest, InviteTicket, InviteWelcome, desktop_start_invite_claim};
use courier::publish::{PublishAck, PublishedItem, PublishedWorkspace, desktop_publish};
use courier::role::RoleRequest;
use courier::subscribe::desktop_start_subscribe;
use courier::sync::{SyncAck, SyncHello, SyncLayer, SyncReply, desktop_start_sync, item_scope};
use courier::token::{Scope, Token};
use courier::{ClaimWelcome, ConnectionTicket, DesktopNodeRecord};
use kunki::admin::{Admin, Cause};
use kunki::bridge::{Envelope, Request};
use kunki::push::{LiveRegistry, Push};
use loro::{ExportMode, LoroDoc};
use osvauld_rpc::Response;
use vault::{Vault, WorkspaceItem, WorkspaceMeta};

/// Where this account's node requests go.
#[derive(Clone)]
pub enum Link {
    /// This account hosts for itself: `kunki::admin` over its own vault. No socket and no
    /// envelope proof — the caller is this process.
    Here(Vault),
    /// A kunki node reached over its unix socket: the pre-pivot path, which
    /// `e2e_chat_sync.py`'s node mode still drives.
    Socket(PathBuf),
}

impl Link {
    /// From what the account remembers: no relationship, or one whose node is this very
    /// account, means it hosts for itself; anything else is a claimed node's socket.
    pub fn current(vault: &Vault) -> Link {
        match load_relationship(vault).ok().flatten() {
            Some(record) if !is_me(vault, &record) => Link::socket(),
            _ => Link::Here(vault.clone()),
        }
    }

    /// The kunki socket, whatever the account remembers: a claim names a *new* node, so it
    /// never goes to the one this account already is.
    pub fn socket() -> Link {
        Link::Socket(kunki::bridge::socket_path())
    }
}

/// Whether `record` names this account as its own host.
pub fn is_me(vault: &Vault, record: &DesktopNodeRecord) -> bool {
    vault.with_signer(|d| d.did() == record.node_did) == Some(true)
}

/// The in-process host's push registry — one per process, as the host is.
pub fn registry() -> &'static LiveRegistry {
    static REGISTRY: OnceLock<LiveRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LiveRegistry::new)
}

/// Hosting for itself: the owner's root grant, issued by this account to this account and
/// recorded in its own `kunki::admin` so every later check finds it. The relationship it saves
/// names this account as the node, which is what `Link::current` reads as "here".
pub fn self_claim(vault: &Vault, now: u64) -> Result<DesktopNodeRecord, String> {
    let (me, encryption, token) = vault
        .with_signer(|me| {
            courier::self_claim(me, now).map(|token| {
                let encryption = URL_SAFE_NO_PAD.encode(me.encryption_public_key());
                (me.did().to_string(), encryption, token)
            })
        })
        .ok_or("account is locked")?
        .map_err(|e| e.to_string())?;
    Admin::new(vault.clone())
        .record(&token, Cause::Node, now)
        .map_err(|e| e.to_string())?;
    let device = vault.device_public_key().map_err(|e| e.to_string())?;
    let record = DesktopNodeRecord {
        node_did: me.clone(),
        node_id: URL_SAFE_NO_PAD.encode(device),
        node_encryption_key: encryption,
        token,
    };
    save_as(vault, &me, &record)?;
    Ok(record)
}

/// The spaces this account serves from its own endpoint: every workspace it holds that it did
/// not join from another host (`joined/<ws>` marks those, step 6) — when it hosts for itself.
pub fn hosting(vault: &Vault) -> Result<Vec<String>, String> {
    if !matches!(Link::current(vault), Link::Here(_)) {
        return Ok(Vec::new());
    }
    let joined = vault.list_entries("joined/").map_err(|e| e.to_string())?;
    Ok(vault
        .workspaces()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|w| w.id)
        .filter(|id| !joined.iter().any(|j| j == &format!("joined/{id}")))
        .collect())
}

/// Who serves `ws_id`, for a space this account hosts: this account first — `online` is
/// whether this device's endpoint is actually bound, a host nobody can dial is not one to
/// advertise — then every appointed host that has said where it is, `online` by `live`
/// (whether it holds a host session right now).
pub fn hosts(
    vault: &Vault,
    ws_id: &str,
    online: bool,
    now: u64,
    live: &dyn Fn(&str) -> bool,
) -> Result<Vec<serde_json::Value>, String> {
    if !hosting(vault)?.iter().any(|w| w == ws_id) {
        return Ok(Vec::new());
    }
    let did = vault
        .with_signer(|d| d.did().to_string())
        .ok_or("account is locked")?;
    let device = URL_SAFE_NO_PAD.encode(vault.device_public_key().map_err(|e| e.to_string())?);
    let mut out = vec![serde_json::json!({ "did": did, "device": device, "online": online })];
    let appointed = kunki::host::appointed_hosts(&Admin::new(vault.clone()), ws_id, now)
        .map_err(|e| e.to_string())?;
    out.extend(appointed.iter().map(|h| {
        serde_json::json!({
            "did": h.did,
            "device": URL_SAFE_NO_PAD.encode(h.addr.device),
            "online": live(&h.did),
        })
    }));
    Ok(out)
}

fn connect(socket: &Path) -> Result<UnixStream, String> {
    // The Android debug process has no desktop environment variables; ADB supplies this endpoint.
    #[cfg(all(target_os = "android", debug_assertions, feature = "adb-debug"))]
    let android_socket = crate::android_debug_dir().join("kunki.sock");
    #[cfg(all(target_os = "android", debug_assertions, feature = "adb-debug"))]
    let socket = android_socket.as_path();
    UnixStream::connect(socket).map_err(|e| e.to_string())
}

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
fn call(link: &Link, req: &Request) -> Result<Response, String> {
    send(link, &Envelope::plain(req)?)
}

/// Signed as the vault's account, for the node this account has claimed (`courier::proof`).
fn call_as(link: &Link, vault: &Vault, req: &Request) -> Result<Response, String> {
    send(link, &seal(vault, req)?)
}

fn seal(vault: &Vault, req: &Request) -> Result<Envelope, String> {
    let node_did = load_relationship(vault)?
        .ok_or("this account has not joined a node")?
        .node_did;
    vault
        .with_signer(|me| Envelope::signed(req, me, &node_did, kunki::bridge::now_ms()))
        .ok_or("account is locked")?
}

fn send(link: &Link, env: &Envelope) -> Result<Response, String> {
    match link {
        Link::Here(host) => {
            let req: Request = serde_json::from_str(env.body.get()).map_err(|e| e.to_string())?;
            Ok(kunki::bridge::dispatch(req, host, registry()))
        }
        Link::Socket(socket) => {
            let mut conn = connect(socket)?;
            conn.set_read_timeout(Some(READ_TIMEOUT))
                .map_err(|e| e.to_string())?;
            let payload = serde_json::to_vec(env).map_err(|e| e.to_string())?;
            osvauld_rpc::write_msg(&mut conn, &payload).map_err(|e| e.to_string())?;
            let bytes = osvauld_rpc::read_msg(&mut conn).map_err(|e| e.to_string())?;
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())
        }
    }
}

fn unpack<T: serde::de::DeserializeOwned>(resp: Response) -> Result<T, String> {
    match resp {
        Response::Ok { result } => serde_json::from_value(result).map_err(|e| e.to_string()),
        Response::Err { message } => Err(message),
    }
}

/// Liveness probe — also how a caller waits for a just-spawned node's socket to exist.
pub fn ping(link: &Link) -> bool {
    matches!(call(link, &Request::Ping), Ok(Response::Ok { .. }))
}

/// Redeem a node's boot ticket as its first admin. `vault` signs as the local account without
/// this module ever holding its key — the same discipline `kunki::main`'s own boot ticket
/// keeps on the other side of this exchange.
pub fn claim(
    link: &Link,
    vault: &Vault,
    ticket: &ConnectionTicket,
    now: u64,
) -> Result<DesktopNodeRecord, String> {
    let hello = vault
        .with_signer(|desktop| courier::desktop_start_claim(ticket.clone(), desktop, now))
        .ok_or("account is locked")?
        .map_err(|e| e.to_string())?;
    let me = hello.desktop_did.clone();
    let welcome: ClaimWelcome = unpack(call(link, &Request::Claim(hello))?)?;
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
    link: &Link,
    vault: &Vault,
    token: Token,
    workspace: PublishedWorkspace,
    items: Vec<PublishedItem>,
) -> Result<PublishAck, String> {
    let did = vault
        .with_signer(|desktop| desktop.did().to_string())
        .ok_or("account is locked")?;
    let hello = desktop_publish(&did, token, workspace, items);
    unpack(call_as(link, vault, &Request::Publish(hello))?)
}

/// Ask the claimed node to mint an invite for someone else to redeem at `role`/`scope`. Only
/// the first desktop ever claims a node (`Admin::accept_claim` refuses a second admin); every
/// later member arrives through this and [`claim_invite`] instead.
pub fn invite(
    link: &Link,
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
    unpack(call_as(link, vault, &Request::Invite(request))?)
}

/// Redeem an invite minted by [`invite`]. No `desktop_finish_invite_claim` exists — unlike
/// `ClaimWelcome`, `InviteWelcome` carries no separate verification step — so the record is
/// built directly from what the ticket and the welcome each already vouch for.
pub fn claim_invite(
    link: &Link,
    vault: &Vault,
    ticket: InviteTicket,
    now: u64,
) -> Result<DesktopNodeRecord, String> {
    let hello = vault
        .with_signer(|desktop| desktop_start_invite_claim(ticket.clone(), desktop, now))
        .ok_or("account is locked")?
        .map_err(|e| e.to_string())?;
    let me = hello.desktop_did.clone();
    let welcome: InviteWelcome = unpack(call(link, &Request::ClaimInvite(hello))?)?;
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
    link: &Link,
    vault: &Vault,
    text: &str,
    now: u64,
) -> Result<DesktopNodeRecord, String> {
    if let Ok(ticket) = ConnectionTicket::from_text(text) {
        return claim(link, vault, &ticket, now);
    }
    let invite = match InviteTicket::from_text(text) {
        Ok(ticket) => ticket,
        Err(_) if crate::names::is_name(text) => {
            InviteTicket::from_text(&crate::names::resolve(text)?)
                .map_err(|_| format!("'{text}' does not point at an invite"))?
        }
        Err(_) => return Err("not a node ticket, invite or name".to_string()),
    };
    claim_invite(link, vault, invite, now)
}

/// The socket round trip only — `hello` is built against a live `LoroDoc` before this call
/// (`courier::desktop_start_sync` needs the doc itself, not bytes), and the ack's `update` is
/// imported back into that same doc after. Both of those touch a doc that is not `Send`; this
/// function is the part of the exchange that is, so a caller offloads only this to a thread.
pub fn sync(link: &Link, vault: &Vault, hello: SyncHello) -> Result<SyncReply, String> {
    unpack(call_as(link, vault, &Request::Sync(hello))?)
}

/// `sync` where a rejection is only a failure: a pull, or a push with no doc to roll back.
fn sync_acked(link: &Link, vault: &Vault, hello: SyncHello) -> Result<SyncAck, String> {
    match sync(link, vault, hello)? {
        SyncReply::Ack(ack) => Ok(ack),
        SyncReply::Rejected(r) => Err(format!("rejected: {}", r.reason)),
    }
}

/// Pull whatever the node already has for one doc layer, best-effort: `None` on any failure —
/// network, authorization, or the node genuinely having nothing for it — collapsed into one
/// outcome because the caller's fallback is the same either way. This is what stops a second
/// desktop's app from mistaking "nobody has synced this doc to *me* yet" for "this doc has
/// never been written," and re-running its own first-run seeding logic against content someone
/// else already created — see `resolver_with_node` in `shell2/src/main.rs`, the caller.
pub fn pull_doc(
    link: &Link,
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
    let ack = sync_acked(link, vault, hello).ok()?;
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
    link: &Link,
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
    let ack = sync_acked(link, vault, hello)?;
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
    link: &Link,
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
    sync_acked(link, vault, hello).map(|_ack| ())
}

/// Declare interest in one item's layer — what makes a future push for it reach this desktop,
/// once a [`listen`] connection is open to carry it. Idempotent on the node side
/// (`Admin::subscribe` is a plain key write), so calling this more than once for the same
/// layer costs nothing beyond the round trip.
pub fn subscribe(
    link: &Link,
    vault: &Vault,
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &str,
    layer: SyncLayer,
) -> Result<(), String> {
    let hello = desktop_start_subscribe(desktop_did, token, ws_id, item_id, layer);
    unpack(call_as(link, vault, &Request::Subscribe(hello))?)
}

pub fn unsubscribe(
    link: &Link,
    vault: &Vault,
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &str,
    layer: SyncLayer,
) -> Result<(), String> {
    let hello = desktop_start_subscribe(desktop_did, token, ws_id, item_id, layer);
    unpack(call_as(link, vault, &Request::Unsubscribe(hello))?)
}

/// Give `to` an app role on one item, or with `revoke` take it back. The node decides from
/// this account's grants on record; `token` only proves membership.
#[allow(clippy::too_many_arguments)]
pub fn change_role(
    link: &Link,
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
    match call_as(link, vault, &req)? {
        Response::Ok { .. } => Ok(()),
        Response::Err { message } => Err(message),
    }
}

/// Open the channel pushes travel on and wait for the node's ack. Deliberately not a loop
/// itself — a caller reads pushes off the returned connection with [`next_push`], normally in
/// a loop on its own thread, so establishing the connection (with its own retry-on-drop
/// policy) and consuming it stay two separate concerns.
pub fn listen(
    link: &Link,
    vault: &Vault,
    desktop_did: &str,
    token: Token,
) -> Result<Pushes, String> {
    match link {
        Link::Here(host) => {
            Admin::new(host.clone())
                .authorize_listen(desktop_did, &token, now_secs())
                .map_err(|e| e.to_string())?;
            Ok(Pushes::Here(registry().register(desktop_did)))
        }
        Link::Socket(socket) => {
            let req = Request::Listen {
                desktop_did: desktop_did.to_string(),
                token,
            };
            let env = seal(vault, &req)?;
            let mut conn = connect(socket)?;
            let payload = serde_json::to_vec(&env).map_err(|e| e.to_string())?;
            osvauld_rpc::write_msg(&mut conn, &payload).map_err(|e| e.to_string())?;
            let bytes = osvauld_rpc::read_msg(&mut conn).map_err(|e| e.to_string())?;
            match serde_json::from_slice(&bytes).map_err(|e| e.to_string())? {
                Response::Ok { .. } => Ok(Pushes::Socket(conn)),
                Response::Err { message } => Err(message),
            }
        }
    }
}

/// What [`listen`] hands back: the held socket, or the in-process registry's own channel.
/// Either ends when the host replaces or forgets this desktop's registration.
pub enum Pushes {
    Socket(UnixStream),
    Here(Receiver<Push>),
}

/// Blocks for the next `Push`.
pub fn next_push(pushes: &mut Pushes) -> Result<Push, String> {
    match pushes {
        Pushes::Socket(conn) => {
            let bytes = osvauld_rpc::read_msg(conn).map_err(|e| e.to_string())?;
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())
        }
        Pushes::Here(rx) => rx.recv().map_err(|_| "host registry closed".to_string()),
    }
}

#[cfg(test)]
mod tests;
