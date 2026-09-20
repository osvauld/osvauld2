//! Kunki node bootstrap: open the node's account and print a connection ticket.
//!
//! The node keeps its identity in a `vault` account, the same store shell2 uses, so
//! tokens and revocations can be sealed beside it later. Workspaces, publishing, admin
//! storage, and sync are still absent.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use kunki::{NodeError, node};
use zeroize::Zeroizing;

fn main() -> Result<(), NodeError> {
    let passphrase = Zeroizing::new(
        std::env::var("OSVAULD_KUNKI_PASSPHRASE").map_err(|_| NodeError::NoPassphrase)?,
    );
    let (vault, mnemonic) = node::open(data_dir(), &passphrase)?;

    if let Some(mnemonic) = mnemonic {
        let phrase = Zeroizing::new(mnemonic.to_string());
        eprintln!("created kunki identity: {}", node::did(&vault)?);
        eprintln!("recovery phrase: {}", &*phrase);
    }

    // Signed inside the vault: the node's key never reaches this binary.
    let ticket = vault
        .with_signer(|signer| courier::issue_connection_ticket(signer, now_secs(), "kunki"))
        .ok_or(NodeError::Locked)??;
    println!("{}", encode(serde_json::to_vec(&ticket)?));
    Ok(())
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

fn encode(bytes: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}
