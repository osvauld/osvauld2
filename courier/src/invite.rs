//! Invite: a node minting a role-scoped ticket for someone an existing member authorizes, so
//! a second person can join without becoming an owner. Same mechanism as bootstrap's
//! `ConnectionTicket` — a node-signed claim later redeemed by a claimant — but the claim bakes
//! in *which* role and scope the redeemer receives, rather than always meaning "become the
//! first owner". Kept as its own type rather than folded into `ConnectionTicket`: the bootstrap
//! ticket's fields (fixed cap, no role/scope) would be dead weight on every invite, and vice
//! versa.
//!
//! Redemption (`desktop_start_invite_claim`/`node_accept_invite`) mints the same shape of
//! token bootstrap does, but replay protection can't be the in-memory `challenges: &mut
//! Vec<String>` reconnect uses — an invite ticket has to still be good after a node restart
//! between minting and redemption. So `redeemed` here is read-only input, exactly like
//! `revoked` elsewhere: courier never touches storage, the caller loads what's spent before
//! calling and persists the newly spent nonce (`InviteWelcome::redeemed_nonce`) after.

use std::collections::HashSet;

use identity::Signer;
use serde::{Deserialize, Serialize};

use crate::policy::{self, Capability};
use crate::token::{self, Scope, Token};
use crate::{CourierError, Result, nonce, sign_blob, verify_blob};

const INVITE_TICKET_DOMAIN: &[u8] = b"osvauld/courier/invite/v1\0";
const INVITE_TICKET_VERSION: u8 = 1;
const INVITE_TICKET_PREFIX: &str = "osvi1.";

/// What an existing member asks the node for: a ticket that grants `role` at `scope` to
/// whoever redeems it. `token`/`desktop_did` are the asker's own claim/reconnect grant,
/// presented as-is — this message signs nothing itself, `issue_invite_ticket` does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteRequest {
    pub desktop_did: String,
    pub token: Token,
    pub role: String,
    pub scope: Scope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteTicket {
    pub version: u8,
    pub node_did: String,
    pub node_encryption_key: String,
    pub device_public_key: String,
    pub node_id: String,
    pub name: String,
    pub relay: Option<String>,
    pub invite_token: String,
}

impl InviteTicket {
    /// See `ConnectionTicket::to_text`: same reasoning, a distinct prefix so the two ticket
    /// kinds are unambiguous on sight rather than by decoding one to find out.
    pub fn to_text(&self) -> Result<String> {
        let json = serde_json::to_vec(self).map_err(|_| CourierError::Decode)?;
        Ok(format!("{INVITE_TICKET_PREFIX}{}", crate::enc(json)))
    }

    pub fn from_text(text: &str) -> Result<Self> {
        let body = text
            .trim()
            .strip_prefix(INVITE_TICKET_PREFIX)
            .ok_or(CourierError::UnknownTicketVersion)?;
        let ticket: Self =
            serde_json::from_slice(&crate::dec(body)?).map_err(|_| CourierError::Decode)?;
        if ticket.version != INVITE_TICKET_VERSION {
            return Err(CourierError::UnknownTicketVersion);
        }
        Ok(ticket)
    }
}

/// The signed half. Role and scope live only here, not on the public `InviteTicket` — the
/// human copying the ticket text does not need to see them, and the node re-derives them from
/// this verified blob at redemption rather than trusting a copy sitting in the clear.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct InviteClaim {
    version: u8,
    iss: String,
    role: String,
    scope: Scope,
    nonce: String,
    iat: u64,
    node_encryption_key: String,
    device_public_key: String,
    node_id: String,
    name: String,
    relay: Option<String>,
}

/// Whether `role` could ever carry platform capability at `scope`, or at anything a holder
/// could narrow `scope` into by ordinary client-side delegation (`token::delegate` performs no
/// node-side check — narrowing is between holders, never approved by the node). The table only
/// ever assigns capability at `Scope::Node` or `Scope::Workspace(_)` — `Scope::App`/
/// `Scope::Resource` are always empty — and a workspace can only narrow further into app/resource
/// scope, which the table never populates. So `Scope::Node` is the only level that can narrow
/// into a level the table treats differently, and the one lookahead this needs beyond the literal
/// cell is: a node-scope request must also be checked at workspace scope. `"maintainer"` is
/// exactly the role this catches — empty at `Scope::Node`, fully powered at `Scope::Workspace(_)`
/// — which a first cut of this check missed; a fresh review caught it before this landed.
fn role_could_gain_capability(role: &str, scope: &Scope) -> bool {
    if !policy::platform_capabilities(role, scope).is_empty() {
        return true;
    }
    // Never stored or sent anywhere — just a workspace-shaped probe so the table's
    // `Scope::Workspace(_)` arms match regardless of which real workspace this is about.
    matches!(scope, Scope::Node)
        && !policy::platform_capabilities(role, &Scope::Workspace(String::new())).is_empty()
}

/// Mint an invite. `request.token` must carry `Capability::MemberInvite` over `request.scope` —
/// checked exactly as `node_accept_publish` checks `WorkspaceCreate` — and `request.role` must
/// carry no platform capability at `request.scope` or at anything it could be delegated down
/// into (see `role_could_gain_capability`), so this cannot be used to hand out a role with real
/// power ahead of `role.assign`'s rank check.
pub fn issue_invite_ticket(
    node: &(impl Signer + ?Sized),
    request: &InviteRequest,
    name: &str,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<InviteTicket> {
    policy::authorize(
        &request.token,
        node.did(),
        &request.desktop_did,
        Capability::MemberInvite,
        &request.scope,
        now,
        revoked,
    )?;
    if role_could_gain_capability(&request.role, &request.scope) {
        return Err(CourierError::RoleNotInvitable);
    }

    let node_encryption_key = crate::enc(node.encryption_public_key());
    let device_public_key = crate::enc(node.device_public_key());
    let claim = InviteClaim {
        version: INVITE_TICKET_VERSION,
        iss: node.did().to_string(),
        role: request.role.clone(),
        scope: request.scope.clone(),
        nonce: nonce(),
        iat: now,
        node_encryption_key: node_encryption_key.clone(),
        device_public_key: device_public_key.clone(),
        node_id: device_public_key.clone(),
        name: name.to_string(),
        relay: None,
    };
    Ok(InviteTicket {
        version: INVITE_TICKET_VERSION,
        node_did: node.did().to_string(),
        node_encryption_key,
        device_public_key: device_public_key.clone(),
        node_id: device_public_key,
        name: name.to_string(),
        relay: None,
        invite_token: sign_blob(node, INVITE_TICKET_DOMAIN, &claim)?,
    })
}

fn verify_invite_ticket(ticket: &InviteTicket) -> Result<InviteClaim> {
    let claim: InviteClaim =
        verify_blob(&ticket.invite_token, &ticket.node_did, INVITE_TICKET_DOMAIN)?;
    let matches = claim.version == ticket.version
        && claim.iss == ticket.node_did
        && claim.node_encryption_key == ticket.node_encryption_key
        && claim.device_public_key == ticket.device_public_key
        && claim.node_id == ticket.node_id
        && claim.name == ticket.name
        && claim.relay == ticket.relay;
    matches.then_some(claim).ok_or(CourierError::BadAttestation)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteClaimHello {
    pub ticket: InviteTicket,
    pub desktop_did: String,
    pub desktop_encryption_key: String,
    pub desktop_device_key: String,
    /// The claimant's signature over its own key material — same role `ClaimHello::attestation`
    /// plays for bootstrap.
    pub attestation: Token,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteWelcome {
    pub node_did: String,
    pub token: Token,
    /// What the caller must persist as spent before this redemption counts as done — the same
    /// nonce `node_accept_invite` just checked was not already in `redeemed`.
    pub redeemed_nonce: String,
}

pub fn desktop_start_invite_claim(
    ticket: InviteTicket,
    desktop: &(impl Signer + ?Sized),
    now: u64,
) -> Result<InviteClaimHello> {
    verify_invite_ticket(&ticket)?;
    let desktop_encryption_key = crate::enc(desktop.encryption_public_key());
    let desktop_device_key = crate::enc(desktop.device_public_key());
    Ok(InviteClaimHello {
        attestation: token::attest(
            desktop,
            &ticket.node_did,
            token::KeyBinding {
                encryption: desktop_encryption_key.clone(),
                device: desktop_device_key.clone(),
            },
            now,
            now + crate::CLAIM_TTL,
        )?,
        ticket,
        desktop_did: desktop.did().to_string(),
        desktop_encryption_key,
        desktop_device_key,
    })
}

/// Redeem an invite once. `redeemed` is every nonce this node has already spent; the caller
/// loads it before calling and, on `Ok`, persists `InviteWelcome::redeemed_nonce` into it —
/// the same load-before/persist-after shape `Admin::accept_claim` already uses for `admins`.
pub fn node_accept_invite(
    hello: InviteClaimHello,
    node: &(impl Signer + ?Sized),
    now: u64,
    redeemed: &HashSet<String>,
) -> Result<InviteWelcome> {
    let claim = verify_invite_ticket(&hello.ticket)?;
    if claim.iss != node.did() {
        return Err(CourierError::NodeMismatch);
    }
    if redeemed.contains(&claim.nonce) {
        return Err(CourierError::InviteAlreadyRedeemed);
    }
    // Rooted at the claimant, not at us — same reasoning as `node_accept_claim`.
    let attested = token::verify_chain(
        &hello.attestation,
        &hello.desktop_did,
        node.did(),
        now,
        &HashSet::new(),
    )?;
    let binds = attested.binds.ok_or(CourierError::BadAttestation)?;
    if binds.encryption != hello.desktop_encryption_key || binds.device != hello.desktop_device_key
    {
        return Err(CourierError::BadAttestation);
    }
    let token = token::issue_root(
        node,
        &hello.desktop_did,
        &claim.role,
        claim.scope,
        true,
        now,
        now + crate::CLAIM_TTL,
    )?;
    Ok(InviteWelcome {
        node_did: node.did().to_string(),
        token,
        redeemed_nonce: claim.nonce,
    })
}

#[cfg(test)]
mod tests;
