//! The frames of a space session (`laptop-host.md` §2 Sessions), over one bidirectional
//! stream a member opens to a host. Vocabulary only — `net` carries the bytes, the host's
//! `kunki::admin` and the member's shell act on them.
//!
//! ```text
//! anyone → host   Fetch { code }                          first frame; no identity yet
//! host   → anyone Fetched { invite, hosts, ws, items } | Refused { reason }
//!
//! member → host   Hello { attestation, ws_id, token | claim, docs }   first frame
//! host   → member Welcome { token?, docs } | Refused { reason }
//! host   → member Hosts { hosts }                          after Welcome, again when the list changes
//! both            Update { item_id, layer, vv, update }   what the other lacks; empty = a pull
//! host   → member Ack { item_id, layer, vv, missing }
//! host   → member Rejected { .. }                          a refused write; the member rolls back
//!
//! host   → owner  HostHere { addr }                        an appointed host, after its Welcome
//! owner  → host   Ledger { entries }                       the whole ledger, then on every change
//!
//! member → host   NetHello { attestation, ws_id, token }   first frame of a second stream
//! host   → member Welcome { .. } | Refused { reason }      then the stream carries `NetFrame`s
//! ```
//!
//! The second stream is app channel traffic (`jetpack-mp.md` §6 step 3): its own QUIC stream so
//! game inputs never queue behind a large doc update. Admitted like `Hello`, with a token held
//! (a claim is the first session's business), and judged message by message ([`NetFrame`]).
//!
//! `Update` reuses [`SyncHello`](crate::sync::SyncHello)'s meaning for `vv` and `update`, and
//! `Ack` is [`SyncAck`](crate::sync::SyncAck) without the request id — a session is ordered, so
//! a reply is answered by the frame that carried it. A host relays an accepted update to its
//! other sessions on the same space as an `Update` with the host's own `vv`.
//!
//! An appointed host (`laptop-host.md` §2 "Appointed hosts") is a member whose session to the
//! owner is also its uplink: the last two frames ride on that ordinary session, and `Hosts` is
//! how every member learns where else the space can be dialled.

use serde::{Deserialize, Serialize};

use crate::invite::{InviteClaimHello, InviteTicket};
use crate::ledger;
use crate::link::HostAddr;
use crate::net::Sealed;
use crate::publish::{PublishedItem, PublishedWorkspace};
use crate::sync::{Rejected, SyncLayer};
use crate::token::Token;

/// A host of a space: who it is and where it is dialled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostEntry {
    pub did: String,
    pub addr: HostAddr,
}

/// One doc's position, as a side states it in `Hello`/`Welcome`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocState {
    pub item_id: String,
    pub layer: SyncLayer,
    pub vv: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frame {
    Fetch {
        code: [u8; crate::link::CODE_LEN],
    },
    Fetched {
        invite: InviteTicket,
        hosts: Vec<HostEntry>,
        ws: PublishedWorkspace,
        items: Vec<PublishedItem>,
    },
    Hello {
        /// `courier::attest`'s text: the DID's word for the device key the transport proved.
        attestation: String,
        ws_id: String,
        /// Held from an earlier session; absent on the first, when `claim` redeems the invite.
        token: Option<Token>,
        claim: Option<InviteClaimHello>,
        docs: Vec<DocState>,
    },
    Welcome {
        /// Minted for a `claim`; the member keeps it for every later `Hello`.
        token: Option<Token>,
        docs: Vec<DocState>,
    },
    Refused {
        reason: String,
    },
    Update {
        item_id: String,
        layer: SyncLayer,
        vv: Vec<u8>,
        update: Vec<u8>,
    },
    Ack {
        item_id: String,
        layer: SyncLayer,
        vv: Vec<u8>,
        missing: bool,
    },
    Rejected {
        item_id: String,
        layer: SyncLayer,
        rejected: Rejected,
    },
    /// Give (or with `revoke`, take back) an app role on one of the space's items; the host
    /// decides from the member's grants on record (`courier::role`).
    Role {
        item_id: String,
        to: String,
        role: String,
        revoke: bool,
    },
    RoleDone {
        ok: bool,
        reason: String,
    },
    /// The owner's whole ledger for this space, every send: replay is idempotent, so a resend
    /// needs no cursor.
    Ledger {
        entries: Vec<ledger::Signed>,
    },
    /// Every host of the space, owner first — the order a member fails over in.
    Hosts {
        hosts: Vec<HostEntry>,
    },
    /// Where this appointed host can be dialled; the owner lists it while the DID holds
    /// [`role::HOST`](crate::role::HOST).
    HostHere {
        addr: HostAddr,
    },
    NetHello {
        attestation: String,
        ws_id: String,
        token: Token,
    },
}

/// The channel stream after its `Welcome`. Presence is per item: a device is "here" on an item
/// while it has the app open, and only those devices hear its messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetFrame {
    /// member → host: this device opened the item's app; the host checks it may.
    Open { item_id: String },
    Close { item_id: String },
    /// Both ways: one message, relayed unread to every other device here on its item.
    Msg(Sealed),
    /// host → member: a device here on the item, its attestation so the member checks the
    /// device's DID itself. Sent for each one already here on `Open`, then as each arrives.
    Here { item_id: String, attestation: String },
    Gone { item_id: String, device: [u8; 32] },
}
