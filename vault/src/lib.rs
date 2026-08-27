//! Headless account manager: composes `identity` + `storage` into signup, login, and
//! account-switching, holding the one unlocked `Identity` for the session. The only auth
//! code in the tree; both `sthalam` (UI) and `kunki` (node) drive it. See `docs/vault.md`.

mod account;
mod error;
mod item;
mod workspace;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use identity::{Identity, Keystore, Mnemonic};
pub use storage::Store;

pub use account::AccountInfo;
pub use error::VaultError;
pub use item::{ItemKind, WorkspaceItem};
pub use workspace::WorkspaceMeta;

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

impl Active {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
        Ok(identity::encrypt_for(
            &self.identity.encryption_public_key(),
            plaintext,
        )?)
    }
    pub fn unseal(&self, sealed: &[u8]) -> Result<Vec<u8>, VaultError> {
        self.identity
            .decrypt_sealed(sealed)
            .map_err(|e| VaultError::Crypto(e))
    }
}

#[derive(Clone)]
pub struct Vault {
    dir: PathBuf,
    active: Arc<Mutex<Option<Active>>>,
}

/// Split out so the Argon2 hashing ([`Vault::prepare_signup`]) can run off the UI thread,
/// then be committed ([`Vault::commit_signup`]) on it.
struct PreparedAccount {
    identity: Identity,
    mnemonic: Mnemonic,
    label: String,
    keystore: Keystore,
}

/// The unlocked identity produced by [`Vault::prepare_login`] (off the UI thread), installed
/// as the active account by [`Vault::commit_login`].
struct UnlockedAccount {
    did: String,
    label: String,
    identity: Identity,
}

impl Vault {
    pub fn open(dir: PathBuf) -> Result<Self, VaultError> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            active: Arc::new(Mutex::new(None)),
        })
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

    pub fn signup(
        &mut self,
        label: &str,
        passphrase: &str,
    ) -> Result<(String, Mnemonic), VaultError> {
        let prepared = Self::prepare_signup(label, passphrase)?;
        self.commit_signup(prepared)
    }

    /// Slow half (Argon2): no `self`, no I/O — safe to run on a worker thread.
    fn prepare_signup(label: &str, passphrase: &str) -> Result<PreparedAccount, VaultError> {
        let (identity, mnemonic) = identity::generate();
        let keystore = identity::seal(&identity, passphrase)?;
        Ok(PreparedAccount {
            identity,
            mnemonic,
            label: label.to_string(),
            keystore,
        })
    }

    fn commit_signup(
        &mut self,
        prepared: PreparedAccount,
    ) -> Result<(String, Mnemonic), VaultError> {
        let PreparedAccount {
            identity,
            mnemonic,
            label,
            keystore,
        } = prepared;
        let did = identity.did().to_string();
        let store = Store::open(self.dir.join(did_to_filename(&did)?))?;
        store.put(KEYSTORE_KEY, &keystore.to_bytes())?;
        store.put(LABEL_KEY, label.as_bytes())?;
        *self.active.lock().unwrap() = Some(Active {
            did: did.clone(),
            label,
            identity,
            store,
        });
        Ok((did, mnemonic))
    }

    pub fn login(&mut self, did: &str, passphrase: &str) -> Result<(), VaultError> {
        let unlocked = Self::prepare_login(&self.dir, did, passphrase)?;
        self.commit_login(unlocked)
    }

    /// Slow half (Argon2): opens the keystore read-only and decrypts it. Takes no `&self`, so
    /// the UI can run it on a worker thread; pair with [`Vault::commit_login`].
    fn prepare_login(
        dir: &Path,
        did: &str,
        passphrase: &str,
    ) -> Result<UnlockedAccount, VaultError> {
        let path = dir.join(did_to_filename(did)?);
        // Guard existence: Store::open would otherwise create a stray empty db for a bad DID.
        if !path.exists() {
            return Err(VaultError::NoSuchAccount(did.to_string()));
        }
        let store = Store::open_readonly(path)?;
        let bytes = store.get(KEYSTORE_KEY)?.ok_or(VaultError::NoKeystore)?;
        let identity = identity::unlock(&Keystore::from_bytes(&bytes)?, passphrase)?;
        let label = read_label(&store);
        Ok(UnlockedAccount {
            did: did.to_string(),
            label,
            identity,
        })
    }

    fn commit_login(&mut self, unlocked: UnlockedAccount) -> Result<(), VaultError> {
        let UnlockedAccount {
            did,
            label,
            identity,
        } = unlocked;
        // Re-open read-write to hold the db for the session (prepare's read-only handle is
        // already dropped, so there's no single-handle conflict).
        let store = Store::open(self.dir.join(did_to_filename(&did)?))?;
        *self.active.lock().unwrap() = Some(Active {
            did,
            label,
            identity,
            store,
        });
        Ok(())
    }

    pub fn lock(&mut self) {
        *self.active.lock().unwrap() = None;
    }

    pub fn store(&self) -> Option<Store> {
        self.active
            .lock()
            .unwrap()
            .as_ref()
            .map(|a| a.store.clone())
    }

    pub fn current(&self) -> Option<AccountInfo> {
        self.active
            .lock()
            .unwrap()
            .as_ref()
            .map(|active| AccountInfo {
                did: active.did.clone(),
                label: active.label.clone(),
            })
    }

    /// Create a new workspace named `name` in the active account, returning its header (with
    /// the freshly generated id). The header is sealed to the account's own key and written
    /// to `ws/<id>/meta`; no registry doc is touched — the workspace exists by virtue of that
    /// key. Errs with [`VaultError::Locked`] when no account is unlocked.
    pub fn create_workspace(&self, name: &str) -> Result<WorkspaceMeta, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let meta = WorkspaceMeta {
            id: workspace::new_id(),
            name: name.to_string(),
            created: workspace::now_secs(),
        };
        let plaintext = serde_json::to_vec(&meta)?;
        let sealed = active.seal(&plaintext)?;
        active.store.put(&workspace::meta_key(&meta.id), &sealed)?;
        Ok(meta)
    }

    /// Every workspace in the active account, newest first (ties broken by id for a stable
    /// order). Found by prefix-scanning the store for `ws/<id>/meta` keys and unsealing each.
    /// Empty when there are none; errs with [`VaultError::Locked`] when no account is unlocked.
    pub fn workspaces(&self) -> Result<Vec<WorkspaceMeta>, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let mut out = Vec::new();
        for key in active.store.list_prefixed(workspace::WS_PREFIX)? {
            if workspace::id_from_meta_key(&key).is_none() {
                continue;
            }
            let Some(sealed) = active.store.get(&key)? else {
                continue;
            };
            let plaintext = active.unseal(&sealed)?;
            out.push(serde_json::from_slice(&plaintext)?);
        }
        out.sort_by(|a: &WorkspaceMeta, b: &WorkspaceMeta| {
            b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id))
        });
        Ok(out)
    }

    /// Create a new item of `kind` named `name` inside workspace `ws_id`. Only the sealed header
    /// is written: an .app's `main.lua` and `manifest.osv` arrive as a source doc via
    /// [`Vault::put_src`], built by the shell (the vault stays Loro-free).
    pub fn create_item(
        &self,
        ws_id: &str,
        name: &str,
        kind: ItemKind,
    ) -> Result<WorkspaceItem, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let item = WorkspaceItem::new(ws_id, name, kind);
        let plaintext = serde_json::to_vec(&item)?;
        let sealed = active.seal(&plaintext)?;
        active
            .store
            .put(&item::meta_key(ws_id, &item.id), &sealed)?;
        Ok(item)
    }

    /// All items in `ws_id`, newest first. Found by prefix scan; each meta is unsealed.
    pub fn items(&self, ws_id: &str) -> Result<Vec<WorkspaceItem>, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let prefix = item::items_prefix(ws_id);
        let mut out = Vec::new();
        for key in active.store.list_prefixed(&prefix)? {
            if item::id_from_meta_key(ws_id, &key).is_none() {
                continue;
            }
            let Some(sealed) = active.store.get(&key)? else {
                continue;
            };
            let plaintext = active.unseal(&sealed)?;
            out.push(serde_json::from_slice(&plaintext)?);
        }
        out.sort_by(|a: &WorkspaceItem, b: &WorkspaceItem| {
            b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id))
        });
        Ok(out)
    }
    //Retrieve lua src code

    pub fn get_src(&self, ws_id: &str, item_id: &str) -> Result<Option<Vec<u8>>, VaultError> {
        self.get_sealed(&item::src_key(ws_id, item_id))
    }

    //update lua src code
    pub fn put_src(&self, ws_id: &str, item_id: &str, snapshot: &[u8]) -> Result<(), VaultError> {
        self.put_sealed(&item::src_key(ws_id, item_id), snapshot)
    }

    //get state document
    pub fn get_doc(
        &self,
        ws_id: &str,
        item_id: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>, VaultError> {
        self.get_sealed(&item::doc_key(ws_id, item_id, name))
    }

    //update state persistance
    pub fn put_doc(
        &self,
        ws_id: &str,
        item_id: &str,
        snapshot: &[u8],
        name: &str,
    ) -> Result<(), VaultError> {
        if name.is_empty() || name.contains('/') {
            return Err(VaultError::InvalidName(name.to_string()));
        };
        self.put_sealed(&item::doc_key(ws_id, item_id, name), snapshot)
    }
    fn get_sealed(&self, key: &str) -> Result<Option<Vec<u8>>, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let sealed = active.store.get(key)?;
        sealed.map(|s| active.unseal(&s)).transpose()
    }
    fn put_sealed(&self, key: &str, snapshot: &[u8]) -> Result<(), VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let sealed = active.seal(snapshot)?;
        Ok(active.store.put(key, &sealed)?)
    }

    // The active account's label is cached; any other account is opened read-only just
    // long enough to read its label (that db isn't otherwise open, so it can't conflict).
    fn label_for(&self, did: &str) -> Result<String, VaultError> {
        {
            let guard = self.active.lock().unwrap();
            if let Some(active) = guard.as_ref() {
                if active.did == did {
                    return Ok(active.label.clone());
                }
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
