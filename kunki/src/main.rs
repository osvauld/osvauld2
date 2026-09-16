//! Kunki node bootstrap: create/load a node identity and print a connection ticket.
//!
//! This slice stops at node identity and bootstrap material. Workspaces, publishing,
//! resource permits, admin storage, and sync are deliberately absent.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use identity::{Identity, Keystore};
use rand::RngCore;
use serde::Serialize;
use zeroize::Zeroizing;

const DOMAIN: &[u8] = b"osvauld/kunki/claim-ticket/v1\0";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize)]
struct ClaimPayload<'a> {
    version: u8,
    iss: &'a str,
    cap: &'a str,
    nonce: String,
    iat: u64,
    node_encryption_key: &'a str,
    device_public_key: &'a str,
    node_id: &'a str,
    name: &'a str,
    relay: Option<&'a str>,
}

#[derive(Serialize)]
struct SignedClaim {
    payload: String,
    signature: String,
}

#[derive(Serialize)]
struct ConnectionTicket<'a> {
    version: u8,
    node_did: &'a str,
    node_encryption_key: String,
    device_public_key: String,
    node_id: String,
    name: &'a str,
    relay: Option<&'a str>,
    claim_token: String,
}

fn main() -> Result<()> {
    let dir = data_dir();
    fs::create_dir_all(&dir)?;
    let identity = {
        let passphrase =
            Zeroizing::new(std::env::var("OSVAULD_KUNKI_PASSPHRASE").map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "set OSVAULD_KUNKI_PASSPHRASE to create or unlock the node identity",
                )
            })?);
        load_or_create_identity(&dir.join("identity.bin"), &passphrase)?
    };

    let node_encryption_key = encode(identity.encryption_public_key());
    let device_public_key = encode(identity.device_public_key());
    let node_id = device_public_key.clone();
    let claim_token = claim_token(
        &identity,
        now_secs(),
        &node_encryption_key,
        &device_public_key,
        &node_id,
        "kunki",
        None,
    )?;
    let ticket = ConnectionTicket {
        version: 1,
        node_did: identity.did(),
        node_encryption_key,
        node_id,
        device_public_key,
        name: "kunki",
        relay: None,
        claim_token,
    };
    println!("{}", encode(serde_json::to_vec(&ticket)?));
    Ok(())
}

fn load_or_create_identity(path: &Path, passphrase: &str) -> Result<Identity> {
    if path.exists() {
        let keystore = Keystore::from_bytes(&fs::read(path)?)?;
        return Ok(identity::unlock(&keystore, passphrase)?);
    }

    let (identity, mnemonic) = identity::generate();
    let keystore = identity::seal(&identity, passphrase)?;
    write_secret(path, &keystore.to_bytes())?;
    let phrase = Zeroizing::new(mnemonic.to_string());
    eprintln!("created kunki identity: {}", identity.did());
    eprintln!("recovery phrase: {}", &*phrase);
    drop(mnemonic);
    Ok(identity)
}

fn claim_token(
    identity: &Identity,
    now: u64,
    node_encryption_key: &str,
    device_public_key: &str,
    node_id: &str,
    name: &str,
    relay: Option<&str>,
) -> Result<String> {
    let payload = ClaimPayload {
        iss: identity.did(),
        version: 1,
        cap: "node.claim_admin.bootstrap",
        nonce: random_nonce(),
        iat: now,
        node_encryption_key,
        device_public_key,
        node_id,
        name,
        relay,
    };
    let payload_bytes = bincode::serialize(&payload)?;
    let message = [DOMAIN, &payload_bytes].concat();
    let signed = SignedClaim {
        payload: encode(payload_bytes),
        signature: encode(identity.sign(&message)),
    };
    Ok(encode(serde_json::to_vec(&signed)?))
}

fn data_dir() -> PathBuf {
    std::env::var_os("OSVAULD_KUNKI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".kunki"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs()
}

fn random_nonce() -> String {
    let mut bytes = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    encode(bytes)
}

fn encode(bytes: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(unix)]
fn write_secret(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secret(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes)?;
    Ok(())
}
