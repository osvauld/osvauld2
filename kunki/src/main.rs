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
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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

    let ticket = courier::issue_connection_ticket(&identity, now_secs(), "kunki")?;
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
