//! Headless account manager: composes `identity` + `storage` into signup, login, and
//! account-switching, holding the one unlocked `Identity` for the session. The only auth
//! code in the tree; both `sthalam` (UI) and `kunki` (node) drive it. See `docs/vault.md`.

mod account;
mod error;

use std::path::{Path, PathBuf};

use identity::{Identity, Keystore, Mnemonic};
pub use storage::Store;

pub use account::AccountInfo;
pub use error::VaultError;

use account::{did_to_filename, scan_dids};

const KEYSTORE_KEY: &str = "identity/keystore";
const LABEL_KEY: &str = "identity/label";

// The currently unlocked account. `did` and `label` are cached so `accounts()` never has
// to re-open the live db (redb allows a single handle per file).
struct Active {
    did: String,
    label: String,
    identity: Identity,
    // The account's data store, held open for the session — data layers (`.doc`
    // snapshots, etc.) live here, exposed via `Vault::store`.
    store: Store,
}

pub struct Vault {
    dir: PathBuf,
    active: Option<Active>,
}

/// Split out so the Argon2 hashing ([`Vault::prepare_signup`]) can run off the UI thread,
/// then be committed ([`Vault::commit_signup`]) on it.
pub struct PreparedAccount {
    identity: Identity,
    mnemonic: Mnemonic,
    label: String,
    keystore: Keystore,
}

/// The unlocked identity produced by [`Vault::prepare_login`] (off the UI thread), installed
/// as the active account by [`Vault::commit_login`].
pub struct UnlockedAccount {
    did: String,
    label: String,
    identity: Identity,
}

impl Vault {
    pub fn open(dir: PathBuf) -> Result<Self, VaultError> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir, active: None })
    }

    pub fn is_empty(&self) -> bool {
        scan_dids(&self.dir).map_or(true, |dids| dids.is_empty())
    }

    pub fn accounts(&self) -> Result<Vec<AccountInfo>, VaultError> {
        scan_dids(&self.dir)?
            .into_iter()
            .map(|did| {
                let label = self.label_for(&did)?;
                Ok(AccountInfo { did, label })
            })
            .collect()
    }

    pub fn signup(&mut self, label: &str, passphrase: &str) -> Result<(String, Mnemonic), VaultError> {
        let prepared = Self::prepare_signup(label, passphrase)?;
        self.commit_signup(prepared)
    }

    /// Slow half (Argon2): no `self`, no I/O — safe to run on a worker thread.
    pub fn prepare_signup(label: &str, passphrase: &str) -> Result<PreparedAccount, VaultError> {
        let (identity, mnemonic) = identity::generate();
        let keystore = identity::seal(&identity, passphrase)?;
        Ok(PreparedAccount { identity, mnemonic, label: label.to_string(), keystore })
    }

    pub fn commit_signup(&mut self, prepared: PreparedAccount) -> Result<(String, Mnemonic), VaultError> {
        let PreparedAccount { identity, mnemonic, label, keystore } = prepared;
        let did = identity.did().to_string();
        let store = Store::open(self.dir.join(did_to_filename(&did)?))?;
        store.put(KEYSTORE_KEY, &keystore.to_bytes())?;
        store.put(LABEL_KEY, label.as_bytes())?;
        self.active = Some(Active { did: did.clone(), label, identity, store });
        Ok((did, mnemonic))
    }

    pub fn login(&mut self, did: &str, passphrase: &str) -> Result<(), VaultError> {
        let unlocked = Self::prepare_login(&self.dir, did, passphrase)?;
        self.commit_login(unlocked)
    }

    /// Slow half (Argon2): opens the keystore read-only and decrypts it. Takes no `&self`, so
    /// the UI can run it on a worker thread; pair with [`Vault::commit_login`].
    pub fn prepare_login(dir: &Path, did: &str, passphrase: &str) -> Result<UnlockedAccount, VaultError> {
        let path = dir.join(did_to_filename(did)?);
        // Guard existence: Store::open would otherwise create a stray empty db for a bad DID.
        if !path.exists() {
            return Err(VaultError::NoSuchAccount(did.to_string()));
        }
        let store = Store::open_readonly(path)?;
        let bytes = store.get(KEYSTORE_KEY)?.ok_or(VaultError::NoKeystore)?;
        let identity = identity::unlock(&Keystore::from_bytes(&bytes)?, passphrase)?;
        let label = read_label(&store);
        Ok(UnlockedAccount { did: did.to_string(), label, identity })
    }

    pub fn commit_login(&mut self, unlocked: UnlockedAccount) -> Result<(), VaultError> {
        let UnlockedAccount { did, label, identity } = unlocked;
        // Re-open read-write to hold the db for the session (prepare's read-only handle is
        // already dropped, so there's no single-handle conflict).
        let store = Store::open(self.dir.join(did_to_filename(&did)?))?;
        self.active = Some(Active { did, label, identity, store });
        Ok(())
    }

    pub fn switch(&mut self, did: &str, passphrase: &str) -> Result<(), VaultError> {
        self.lock();
        self.login(did, passphrase)
    }

    pub fn lock(&mut self) {
        self.active = None;
    }

    /// The data directory, so a driver can hand it to [`Vault::prepare_login`] on a worker.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn identity(&self) -> Option<&Identity> {
        self.active.as_ref().map(|active| &active.identity)
    }

    /// The active account's data store, for reading/writing encrypted data layers
    /// (`.doc` snapshots and the like). `None` when locked.
    pub fn store(&self) -> Option<&Store> {
        self.active.as_ref().map(|active| &active.store)
    }

    pub fn current(&self) -> Option<AccountInfo> {
        self.active.as_ref().map(|active| AccountInfo {
            did: active.did.clone(),
            label: active.label.clone(),
        })
    }

    // The active account's label is cached; any other account is opened read-only just
    // long enough to read its label (that db isn't otherwise open, so it can't conflict).
    fn label_for(&self, did: &str) -> Result<String, VaultError> {
        if let Some(active) = &self.active {
            if active.did == did {
                return Ok(active.label.clone());
            }
        }
        let store = Store::open_readonly(self.dir.join(did_to_filename(did)?))?;
        Ok(read_label(&store))
    }
}

// Where osvauld keeps its accounts by default: <os-data-dir>/osvauld (e.g.
// ~/.local/share/osvauld). A driver may override this (e.g. via the OSVAULD_DATA_DIR
// env var) before calling `Vault::open`.
pub fn default_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("osvauld")
}

fn read_label(store: &Store) -> String {
    store
        .get(LABEL_KEY)
        .ok()
        .flatten()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
