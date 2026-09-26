//! Kunki node bootstrap: open the node's account, print a connection ticket, then serve.
//!
//! The node keeps its identity in a `vault` account, the same store shell2 uses, and its
//! grants beside it. The ticket's text form belongs to `courier`, not here — a format only
//! the node could write would be one nothing else could read. The ticket is printed on
//! every boot (harmless once an admin exists — the bootstrap claim it carries is then
//! refused), then the process blocks in `bridge::serve_forever`: publishing and sync land as
//! more `Request` variants there, not as more of this boot sequence.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use kunki::push::LiveRegistry;
use kunki::{NodeError, bridge, node};
use zeroize::Zeroizing;

// Not `main() -> Result`: that prints the Debug form, so every message these errors carry
// for the person running the node was replaced by its variant name.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("kunki: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), NodeError> {
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
    println!("{}", ticket.to_text()?);

    bridge::serve_forever(vault, LiveRegistry::new())?;
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
