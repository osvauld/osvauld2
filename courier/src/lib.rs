//! Courier protocol: authenticated peer sessions, then workspace publish/sync messages.
//!
//! Transport-free throughout: node bootstrap claim and reconnect are pure message
//! transitions, [`token`] is the node-rooted role-token chain, and [`policy`] turns a
//! verified chain into an allow/deny. QUIC/Iroh adapters will only carry these bytes later.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use identity::{Identity, public_key_from_did, verify};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod policy;
pub mod token;

const TICKET_DOMAIN: &[u8] = b"osvauld/courier/ticket/v1\0";
const PERMIT_DOMAIN: &[u8] = b"osvauld/courier/permit/v1\0";
const RECONNECT_DOMAIN: &[u8] = b"osvauld/courier/reconnect/v1\0";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CourierError {
    #[error("decode failed")]
    Decode,
    #[error("signature invalid")]
    BadSignature,
    #[error("ticket did not match node identity")]
    NodeMismatch,
    #[error("permit invalid")]
    BadPermit,
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
struct PermitClaim {
    iss: String,
    aud: String,
    cap: String,
    nonce: String,
    iat: u64,
    subject_encryption_key: Option<String>,
    subject_device_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimHello {
    pub ticket: ConnectionTicket,
    pub desktop_did: String,
    pub desktop_encryption_key: String,
    pub desktop_device_key: String,
    pub permit_for_node: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimWelcome {
    pub node_did: String,
    pub permit_for_desktop: String,
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
    pub permit_for_desktop: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReconnectProof {
    node_did: String,
    desktop_did: String,
    permit_for_desktop: String,
    nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminRecord {
    pub did: String,
    pub permit_for_node: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNodeRecord {
    pub node_did: String,
    pub node_id: String,
    pub node_encryption_key: String,
    pub permit_for_desktop: String,
}

pub fn issue_connection_ticket(node: &Identity, now: u64, name: &str) -> Result<ConnectionTicket> {
    let node_encryption_key = enc(node.encryption_public_key());
    let device_public_key = enc(node.device_public_key());
    let node_id = device_public_key.clone();
    let claim = TicketClaim {
        version: 1,
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
        version: 1,
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
    desktop: &Identity,
    now: u64,
) -> Result<ClaimHello> {
    verify_ticket(&ticket)?;
    let desktop_encryption_key = enc(desktop.encryption_public_key());
    let desktop_device_key = enc(desktop.device_public_key());
    Ok(ClaimHello {
        permit_for_node: issue_permit(
            desktop,
            &ticket.node_did,
            "node.relationship",
            now,
            Some(&desktop_encryption_key),
            Some(&desktop_device_key),
        )?,
        ticket,
        desktop_did: desktop.did().to_string(),
        desktop_encryption_key,
        desktop_device_key,
    })
}

pub fn node_accept_claim(
    hello: ClaimHello,
    node: &Identity,
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
    let permit = verify_permit(
        &hello.permit_for_node,
        &hello.desktop_did,
        node.did(),
        "node.relationship",
    )?;
    if permit.subject_encryption_key.as_deref() != Some(&hello.desktop_encryption_key)
        || permit.subject_device_key.as_deref() != Some(&hello.desktop_device_key)
    {
        return Err(CourierError::BadPermit);
    }
    admins.push(AdminRecord {
        did: hello.desktop_did.clone(),
        permit_for_node: hello.permit_for_node,
    });
    Ok(ClaimWelcome {
        node_did: node.did().to_string(),
        permit_for_desktop: issue_permit(node, &hello.desktop_did, "node.admin", now, None, None)?,
    })
}

pub fn desktop_finish_claim(
    ticket: &ConnectionTicket,
    welcome: ClaimWelcome,
    desktop: &Identity,
) -> Result<DesktopNodeRecord> {
    verify_ticket(ticket)?;
    if welcome.node_did != ticket.node_did {
        return Err(CourierError::NodeMismatch);
    }
    verify_permit(
        &welcome.permit_for_desktop,
        &ticket.node_did,
        desktop.did(),
        "node.admin",
    )?;
    Ok(DesktopNodeRecord {
        node_did: ticket.node_did.clone(),
        node_id: ticket.node_id.clone(),
        node_encryption_key: ticket.node_encryption_key.clone(),
        permit_for_desktop: welcome.permit_for_desktop,
    })
}

pub fn node_issue_reconnect_challenge(
    node: &Identity,
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
    desktop: &Identity,
    challenge: ReconnectChallenge,
) -> Result<ReconnectHello> {
    if challenge.node_did != record.node_did {
        return Err(CourierError::NodeMismatch);
    }
    let proof = ReconnectProof {
        node_did: record.node_did.clone(),
        desktop_did: desktop.did().to_string(),
        permit_for_desktop: record.permit_for_desktop.clone(),
        nonce: challenge.nonce,
    };
    let payload = bincode::serialize(&proof).map_err(|_| CourierError::Decode)?;
    Ok(ReconnectHello {
        node_did: proof.node_did,
        desktop_did: proof.desktop_did,
        permit_for_desktop: proof.permit_for_desktop,
        nonce: proof.nonce,
        signature: enc(desktop.sign(&[RECONNECT_DOMAIN, &payload].concat())),
    })
}

pub fn node_accept_reconnect(
    hello: ReconnectHello,
    node: &Identity,
    admins: &[AdminRecord],
    challenges: &mut Vec<String>,
) -> Result<()> {
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
        permit_for_desktop: hello.permit_for_desktop.clone(),
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
    verify_permit(
        &hello.permit_for_desktop,
        node.did(),
        &hello.desktop_did,
        "node.admin",
    )?;
    challenges.swap_remove(pos);
    Ok(())
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
    matches.then_some(claim).ok_or(CourierError::BadPermit)
}

fn issue_permit(
    issuer: &Identity,
    audience: &str,
    cap: &str,
    now: u64,
    subject_encryption_key: Option<&str>,
    subject_device_key: Option<&str>,
) -> Result<String> {
    sign_blob(
        issuer,
        PERMIT_DOMAIN,
        &PermitClaim {
            iss: issuer.did().to_string(),
            aud: audience.to_string(),
            cap: cap.to_string(),
            nonce: nonce(),
            iat: now,
            subject_encryption_key: subject_encryption_key.map(str::to_string),
            subject_device_key: subject_device_key.map(str::to_string),
        },
    )
}

fn verify_permit(token: &str, issuer: &str, audience: &str, cap: &str) -> Result<PermitClaim> {
    let claim: PermitClaim = verify_blob(token, issuer, PERMIT_DOMAIN)?;
    (claim.iss == issuer && claim.aud == audience && claim.cap == cap)
        .then_some(claim)
        .ok_or(CourierError::BadPermit)
}

fn sign_blob<T: Serialize>(identity: &Identity, domain: &[u8], payload: &T) -> Result<String> {
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
