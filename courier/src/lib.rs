//! Courier protocol: authenticated peer sessions, then workspace publish/sync messages.
//!
//! Transport-free throughout: node bootstrap claim and reconnect are pure message
//! transitions, [`token`] is the delegation chain every credential here is made of, and
//! [`policy`] turns a verified chain into an allow/deny. QUIC/Iroh adapters will only carry
//! these bytes later.
//!
//! One credential mechanism, not two. Every grant and every attestation in this module is a
//! [`token::Token`], so expiry, revocation by id, and the chain walk apply to all of them
//! rather than to whichever ones remembered to implement it. Most chains root at the node,
//! but not all: a claimant's attestation roots at the claimant, and `verify_chain` takes the
//! expected root as a parameter for exactly that reason.

use std::collections::HashSet;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use identity::{Signer, public_key_from_did, verify};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod policy;
pub mod token;

use token::{Claims, Scope, Token};

const TICKET_DOMAIN: &[u8] = b"osvauld/courier/ticket/v1\0";
const RECONNECT_DOMAIN: &[u8] = b"osvauld/courier/reconnect/v1\0";

/// The role the first claimant holds over the node.
///
/// Not `admin`: [`policy::platform_capabilities`] keys on `(scope, role)`, and `admin` exists
/// only at node scope. Delegating an admin's authority down to one workspace would carry the
/// role unchanged into a `(Workspace, "admin")` lookup that is not in the table, leaving the
/// delegate with nothing — silently, since no step errs.
const CLAIM_ROLE: &str = "owner";

/// How long the node's grant to a claimant lives. Reconnect reissues, so this is also the
/// ceiling on how long a revocation takes to bite. The permit this replaced had no expiry
/// at all and no id to revoke, so it was valid forever by construction.
const CLAIM_TTL: u64 = 60 * 60 * 24 * 30;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CourierError {
    #[error("decode failed")]
    Decode,
    #[error("signature invalid")]
    BadSignature,
    // Also raised when a chain roots somewhere other than the authority the caller expected,
    // which since attestations exist is no longer always the node.
    #[error("identity did not match the one expected")]
    NodeMismatch,
    #[error("signed material does not match what was presented")]
    BadAttestation,
    #[error("token is not the grant this step requires")]
    WrongGrant,
    #[error("node already has admins")]
    AlreadyAdmined,
    #[error("unknown admin")]
    UnknownAdmin,
    #[error("stale reconnect challenge")]
    StaleChallenge,
    #[error("delegation chain too long")]
    ChainTooLong,
    #[error("token expired")]
    Expired,
    #[error("token revoked")]
    Revoked,
    #[error("parent token forbids delegation")]
    NotDelegable,
    #[error("delegation widens its parent")]
    Escalation,
    #[error("delegation chain does not reach the node")]
    BrokenChain,
    #[error("token was issued to someone else")]
    WrongHolder,
    #[error("scope is not a valid target")]
    BadScope,
    #[error("role does not cover the target")]
    OutOfScope,
    #[error("role does not carry that capability")]
    NotPermitted,
    #[error("connection ticket is not a version this build understands")]
    UnknownTicketVersion,
}

type Result<T> = std::result::Result<T, CourierError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionTicket {
    pub version: u8,
    pub node_did: String,
    pub node_encryption_key: String,
    pub device_public_key: String,
    pub node_id: String,
    pub name: String,
    pub relay: Option<String>,
    pub claim_token: String,
}

/// Current ticket version. The prefix carries it in the text form so a ticket from a newer
/// node is refused by name rather than decoded into something subtly older.
const TICKET_VERSION: u8 = 1;
const TICKET_PREFIX: &str = "osv1.";

impl ConnectionTicket {
    /// The form a human copies: one URL-safe word, no padding, nothing to quote in a shell.
    /// JSON inside rather than bincode — the two ends update separately, and a self-describing
    /// body turns version skew into a parse error instead of a misread field.
    pub fn to_text(&self) -> Result<String> {
        let json = serde_json::to_vec(self).map_err(|_| CourierError::Decode)?;
        Ok(format!("{TICKET_PREFIX}{}", enc(json)))
    }

    pub fn from_text(text: &str) -> Result<Self> {
        let body = text
            .trim()
            .strip_prefix(TICKET_PREFIX)
            .ok_or(CourierError::UnknownTicketVersion)?;
        let ticket: Self = serde_json::from_slice(&dec(body)?).map_err(|_| CourierError::Decode)?;
        // Belt and braces: the prefix said v1, so the body must agree. `verify_ticket` only
        // checks the ticket and its signed claim against each other, never against us.
        if ticket.version != TICKET_VERSION {
            return Err(CourierError::UnknownTicketVersion);
        }
        Ok(ticket)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TicketClaim {
    version: u8,
    iss: String,
    cap: String,
    nonce: String,
    iat: u64,
    node_encryption_key: String,
    device_public_key: String,
    node_id: String,
    name: String,
    relay: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedBlob {
    payload: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimHello {
    pub ticket: ConnectionTicket,
    pub desktop_did: String,
    pub desktop_encryption_key: String,
    pub desktop_device_key: String,
    /// The claimant's signature over its own key material. The plaintext keys above are what
    /// the node reads; this is what makes them worth reading.
    pub attestation: Token,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimWelcome {
    pub node_did: String,
    pub token: Token,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectChallenge {
    pub node_did: String,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectHello {
    pub node_did: String,
    pub desktop_did: String,
    pub token: Token,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReconnectProof {
    node_did: String,
    desktop_did: String,
    token: Token,
    nonce: String,
}

// Both sides keep their half of the relationship across restarts, so these carry serde:
// the node's list is what the first-admin check is decided against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminRecord {
    pub did: String,
    pub attestation: Token,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopNodeRecord {
    pub node_did: String,
    pub node_id: String,
    pub node_encryption_key: String,
    pub token: Token,
}

pub fn issue_connection_ticket(
    node: &(impl Signer + ?Sized),
    now: u64,
    name: &str,
) -> Result<ConnectionTicket> {
    let node_encryption_key = enc(node.encryption_public_key());
    let device_public_key = enc(node.device_public_key());
    let node_id = device_public_key.clone();
    let claim = TicketClaim {
        version: TICKET_VERSION,
        iss: node.did().to_string(),
        cap: "node.claim_admin.bootstrap".to_string(),
        nonce: nonce(),
        iat: now,
        node_encryption_key: node_encryption_key.clone(),
        device_public_key: device_public_key.clone(),
        node_id: node_id.clone(),
        name: name.to_string(),
        relay: None,
    };
    Ok(ConnectionTicket {
        version: TICKET_VERSION,
        node_did: node.did().to_string(),
        node_encryption_key,
        device_public_key,
        node_id,
        name: name.to_string(),
        relay: None,
        claim_token: sign_blob(node, TICKET_DOMAIN, &claim)?,
    })
}

pub fn desktop_start_claim(
    ticket: ConnectionTicket,
    desktop: &(impl Signer + ?Sized),
    now: u64,
) -> Result<ClaimHello> {
    verify_ticket(&ticket)?;
    let desktop_encryption_key = enc(desktop.encryption_public_key());
    let desktop_device_key = enc(desktop.device_public_key());
    Ok(ClaimHello {
        attestation: token::attest(
            desktop,
            &ticket.node_did,
            token::KeyBinding {
                encryption: desktop_encryption_key.clone(),
                device: desktop_device_key.clone(),
            },
            now,
            now + CLAIM_TTL,
        )?,
        ticket,
        desktop_did: desktop.did().to_string(),
        desktop_encryption_key,
        desktop_device_key,
    })
}

pub fn node_accept_claim(
    hello: ClaimHello,
    node: &(impl Signer + ?Sized),
    admins: &mut Vec<AdminRecord>,
    now: u64,
) -> Result<ClaimWelcome> {
    if !admins.is_empty() {
        return Err(CourierError::AlreadyAdmined);
    }
    let ticket = verify_ticket(&hello.ticket)?;
    if ticket.iss != node.did() {
        return Err(CourierError::NodeMismatch);
    }
    // Rooted at the claimant, not at us: this is the one token in the exchange whose authority
    // is the sender's own. `verify_chain` takes the expected root as a parameter, so it needs
    // no separate path.
    let attested = token::verify_chain(
        &hello.attestation,
        &hello.desktop_did,
        node.did(),
        now,
        &HashSet::new(),
    )?;
    // The keys in the clear must be the keys that were signed, or the signature is over
    // something other than what we are about to record.
    let binds = attested.binds.ok_or(CourierError::BadAttestation)?;
    if binds.encryption != hello.desktop_encryption_key || binds.device != hello.desktop_device_key
    {
        return Err(CourierError::BadAttestation);
    }
    admins.push(AdminRecord {
        did: hello.desktop_did.clone(),
        attestation: hello.attestation,
    });
    Ok(ClaimWelcome {
        node_did: node.did().to_string(),
        token: issue_claim_token(node, &hello.desktop_did, now)?,
    })
}

/// The node's grant to a claimant: its own root authority over itself, delegable so the
/// holder can hand a narrowed slice to their own node when publishing.
fn issue_claim_token(node: &(impl Signer + ?Sized), holder: &str, now: u64) -> Result<Token> {
    token::issue_root(
        node,
        holder,
        CLAIM_ROLE,
        Scope::Node,
        true,
        now,
        now + CLAIM_TTL,
    )
}

/// Check a token the node issued *to us* directly. The revoked set is empty on purpose: a
/// holder does not know what its node has revoked, and finding out is what connecting is
/// for. The node re-checks against its own set on every use.
fn verify_claim_token(token: &Token, node_did: &str, holder: &str, now: u64) -> Result<Claims> {
    let claims = token::verify_chain(token, node_did, holder, now, &HashSet::new())?;
    // A chain that merely reaches the node is not this credential. The relationship token is
    // the node's own root grant at node scope, and nothing narrower stands in for it.
    (claims.role == CLAIM_ROLE && claims.scope == Scope::Node)
        .then_some(claims)
        .ok_or(CourierError::WrongGrant)
}

pub fn desktop_finish_claim(
    ticket: &ConnectionTicket,
    welcome: ClaimWelcome,
    desktop: &(impl Signer + ?Sized),
    now: u64,
) -> Result<DesktopNodeRecord> {
    verify_ticket(ticket)?;
    if welcome.node_did != ticket.node_did {
        return Err(CourierError::NodeMismatch);
    }
    verify_claim_token(&welcome.token, &ticket.node_did, desktop.did(), now)?;
    Ok(DesktopNodeRecord {
        node_did: ticket.node_did.clone(),
        node_id: ticket.node_id.clone(),
        node_encryption_key: ticket.node_encryption_key.clone(),
        token: welcome.token,
    })
}

/// Take the token a reconnect handed back, replacing the one in `record`. Verified before it
/// is kept, so a node that answers with someone else's grant is refused rather than stored.
pub fn desktop_accept_reissue(
    record: &DesktopNodeRecord,
    token: Token,
    desktop: &(impl Signer + ?Sized),
    now: u64,
) -> Result<DesktopNodeRecord> {
    verify_claim_token(&token, &record.node_did, desktop.did(), now)?;
    Ok(DesktopNodeRecord {
        token,
        ..record.clone()
    })
}

pub fn node_issue_reconnect_challenge(
    node: &(impl Signer + ?Sized),
    challenges: &mut Vec<String>,
) -> ReconnectChallenge {
    let challenge = ReconnectChallenge {
        node_did: node.did().to_string(),
        nonce: nonce(),
    };
    challenges.push(challenge.nonce.clone());
    challenge
}

pub fn desktop_start_reconnect(
    record: &DesktopNodeRecord,
    desktop: &(impl Signer + ?Sized),
    challenge: ReconnectChallenge,
) -> Result<ReconnectHello> {
    if challenge.node_did != record.node_did {
        return Err(CourierError::NodeMismatch);
    }
    let proof = ReconnectProof {
        node_did: record.node_did.clone(),
        desktop_did: desktop.did().to_string(),
        token: record.token.clone(),
        nonce: challenge.nonce,
    };
    let payload = bincode::serialize(&proof).map_err(|_| CourierError::Decode)?;
    Ok(ReconnectHello {
        node_did: proof.node_did,
        desktop_did: proof.desktop_did,
        token: proof.token,
        nonce: proof.nonce,
        signature: enc(desktop.sign(&[RECONNECT_DOMAIN, &payload].concat())),
    })
}

/// Re-admit a known holder and hand back a fresh token. Reissuing here is what keeps
/// [`CLAIM_TTL`] survivable: the holder's copy ages out, and a revocation or an expiry is
/// checked on the way through rather than trusted from the last time it connected.
pub fn node_accept_reconnect(
    hello: ReconnectHello,
    node: &(impl Signer + ?Sized),
    admins: &[AdminRecord],
    challenges: &mut Vec<String>,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<Token> {
    if !admins.iter().any(|admin| admin.did == hello.desktop_did) {
        return Err(CourierError::UnknownAdmin);
    }
    if hello.node_did != node.did() {
        return Err(CourierError::NodeMismatch);
    }
    let Some(pos) = challenges.iter().position(|nonce| nonce == &hello.nonce) else {
        return Err(CourierError::StaleChallenge);
    };
    let proof = ReconnectProof {
        node_did: hello.node_did.clone(),
        desktop_did: hello.desktop_did.clone(),
        token: hello.token.clone(),
        nonce: hello.nonce.clone(),
    };
    let payload = bincode::serialize(&proof).map_err(|_| CourierError::Decode)?;
    let sig: [u8; 64] = dec(&hello.signature)?
        .try_into()
        .map_err(|_| CourierError::Decode)?;
    let desktop_public = public_key_from_did(&hello.desktop_did).ok_or(CourierError::Decode)?;
    if !verify(
        &desktop_public,
        &[RECONNECT_DOMAIN, &payload].concat(),
        &sig,
    ) {
        return Err(CourierError::BadSignature);
    }
    // Unlike the permit this replaced, the credential itself can now be expired or revoked,
    // and both are checked here rather than only at the connection's edge.
    token::verify_chain(&hello.token, node.did(), &hello.desktop_did, now, revoked)?;
    challenges.swap_remove(pos);
    issue_claim_token(node, &hello.desktop_did, now)
}

fn verify_ticket(ticket: &ConnectionTicket) -> Result<TicketClaim> {
    let claim: TicketClaim = verify_blob(&ticket.claim_token, &ticket.node_did, TICKET_DOMAIN)?;
    let matches = claim.version == ticket.version
        && claim.iss == ticket.node_did
        && claim.cap == "node.claim_admin.bootstrap"
        && claim.node_encryption_key == ticket.node_encryption_key
        && claim.device_public_key == ticket.device_public_key
        && claim.node_id == ticket.node_id
        && claim.name == ticket.name
        && claim.relay == ticket.relay;
    matches.then_some(claim).ok_or(CourierError::BadAttestation)
}

fn sign_blob<T: Serialize>(
    identity: &(impl Signer + ?Sized),
    domain: &[u8],
    payload: &T,
) -> Result<String> {
    let payload = bincode::serialize(payload).map_err(|_| CourierError::Decode)?;
    let signed = SignedBlob {
        signature: enc(identity.sign(&[domain, &payload].concat())),
        payload: enc(payload),
    };
    serde_json::to_vec(&signed)
        .map(enc)
        .map_err(|_| CourierError::Decode)
}

fn verify_blob<T: for<'de> Deserialize<'de>>(
    token: &str,
    issuer: &str,
    domain: &[u8],
) -> Result<T> {
    let signed: SignedBlob =
        serde_json::from_slice(&dec(token)?).map_err(|_| CourierError::Decode)?;
    let payload = dec(&signed.payload)?;
    let sig: [u8; 64] = dec(&signed.signature)?
        .try_into()
        .map_err(|_| CourierError::Decode)?;
    let public = public_key_from_did(issuer).ok_or(CourierError::Decode)?;
    if !verify(&public, &[domain, &payload].concat(), &sig) {
        return Err(CourierError::BadSignature);
    }
    bincode::deserialize(&payload).map_err(|_| CourierError::Decode)
}

fn nonce() -> String {
    let mut bytes = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    enc(bytes)
}

fn enc(bytes: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn dec(text: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| CourierError::Decode)
}

#[cfg(test)]
mod tests;
