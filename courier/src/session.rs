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
//! both            Update { item_id, layer, vv, update }   what the other lacks; empty = a pull
//! host   → member Ack { item_id, layer, vv, missing }
//! host   → member Rejected { .. }                          a refused write; the member rolls back
//! ```
//!
//! `Update` reuses [`SyncHello`](crate::sync::SyncHello)'s meaning for `vv` and `update`, and
//! `Ack` is [`SyncAck`](crate::sync::SyncAck) without the request id — a session is ordered, so
//! a reply is answered by the frame that carried it. A host relays an accepted update to its
//! other sessions on the same space as an `Update` with the host's own `vv`.

use serde::{Deserialize, Serialize};

use crate::invite::{InviteClaimHello, InviteTicket};
use crate::link::HostAddr;
use crate::publish::{PublishedItem, PublishedWorkspace};
use crate::sync::{Rejected, SyncLayer};
use crate::token::Token;

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
        hosts: Vec<HostAddr>,
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
}
