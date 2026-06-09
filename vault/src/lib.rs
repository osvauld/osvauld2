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

#[derive(Clone)]
pub struct Vault {
    dir: PathBuf,
    active: Arc<Mutex<Option<Active>>>,
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
        Ok(Self { dir, active: Arc::new(Mutex::new(None)) })
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
        *self.active.lock().unwrap() = Some(Active { did: did.clone(), label, identity, store });
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
        *self.active.lock().unwrap() = Some(Active { did, label, identity, store });
        Ok(())
    }

    pub fn switch(&mut self, did: &str, passphrase: &str) -> Result<(), VaultError> {
        self.lock();
        self.login(did, passphrase)
    }

    pub fn lock(&mut self) {
        *self.active.lock().unwrap() = None;
    }

    /// The data directory, so a driver can hand it to [`Vault::prepare_login`] on a worker.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn identity(&self) -> Option<Identity> {
        self.active.lock().unwrap().as_ref().map(|active| active.identity.clone())
    }

    pub fn store(&self) -> Option<Store> {
        self.active.lock().unwrap().as_ref().map(|a| a.store.clone())
    }

    pub fn current(&self) -> Option<AccountInfo> {
        self.active.lock().unwrap().as_ref().map(|active| AccountInfo {
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
        let sealed = identity::encrypt_for(&active.identity.encryption_public_key(), &plaintext)?;
        active.store.put(&workspace::meta_key(&meta.id), &sealed)?;
        Ok(meta)
    }

    /// A single workspace's header by id, or `None` if there's no such workspace. A direct
    /// key read (`ws/<id>/meta`) — use this to open/restore one workspace without scanning the
    /// whole set. Errs with [`VaultError::Locked`] when no account is unlocked.
    pub fn workspace(&self, id: &str) -> Result<Option<WorkspaceMeta>, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let Some(sealed) = active.store.get(&workspace::meta_key(id))? else {
            return Ok(None);
        };
        let plaintext = active.identity.decrypt_sealed(&sealed)?;
        Ok(Some(serde_json::from_slice(&plaintext)?))
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
            let Some(sealed) = active.store.get(&key)? else { continue };
            let plaintext = active.identity.decrypt_sealed(&sealed)?;
            out.push(serde_json::from_slice(&plaintext)?);
        }
        out.sort_by(|a: &WorkspaceMeta, b: &WorkspaceMeta| {
            b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id))
        });
        Ok(out)
    }

    /// Create a new item of `kind` named `name` inside workspace `ws_id`.
    /// App items are seeded with a starter source tree (`manifest.osv` + `main.lua`) so they
    /// render and can be edited (by an agent over MCP) right away.
    pub fn create_item(&self, ws_id: &str, name: &str, kind: ItemKind) -> Result<WorkspaceItem, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let item = WorkspaceItem::new(ws_id, name, kind);
        let plaintext = serde_json::to_vec(&item)?;
        let sealed = identity::encrypt_for(&active.identity.encryption_public_key(), &plaintext)?;
        active.store.put(&item::meta_key(ws_id, &item.id), &sealed)?;
        if item.kind == ItemKind::App {
            active.store.put(&item::file_key(ws_id, &item.id, "manifest.osv"), item::APP_MANIFEST_SEED)?;
            active.store.put(&item::file_key(ws_id, &item.id, "main.lua"), item::APP_MAIN_SEED)?;
        }
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
            let Some(sealed) = active.store.get(&key)? else { continue };
            let plaintext = active.identity.decrypt_sealed(&sealed)?;
            out.push(serde_json::from_slice(&plaintext)?);
        }
        out.sort_by(|a: &WorkspaceItem, b: &WorkspaceItem| {
            b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id))
        });
        Ok(out)
    }

    /// Read a source file at `path` from an item's folder tree. `None` if it doesn't exist.
    pub fn get_file(&self, ws_id: &str, item_id: &str, path: &str) -> Result<Option<Vec<u8>>, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        Ok(active.store.get(&item::file_key(ws_id, item_id, path))?)
    }

    /// Write a source file at `path` in an item's folder tree (the path *is* the identity;
    /// intermediate folders are implied, never created). Overwrites any existing file.
    pub fn put_file(&self, ws_id: &str, item_id: &str, path: &str, data: &[u8]) -> Result<(), VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        Ok(active.store.put(&item::file_key(ws_id, item_id, path), data)?)
    }

    /// Every source-file path in an item's folder tree, sorted (so the order is stable for a
    /// file-tree UI). Empty for items with no files (e.g. a fresh .doc).
    pub fn list_files(&self, ws_id: &str, item_id: &str) -> Result<Vec<String>, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let mut out = Vec::new();
        for key in active.store.list_prefixed(&item::files_prefix(ws_id, item_id))? {
            if let Some(path) = item::path_from_file_key(ws_id, item_id, &key) {
                out.push(path.to_string());
            }
        }
        out.sort();
        Ok(out)
    }

    /// Read the item's runtime CRDT snapshot (a .doc's blocks, an app's runtime). `None` until
    /// the item first stores state.
    pub fn get_state(&self, ws_id: &str, item_id: &str) -> Result<Option<Vec<u8>>, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        Ok(active.store.get(&item::state_key(ws_id, item_id))?)
    }

    /// Persist the item's runtime CRDT snapshot.
    pub fn put_state(&self, ws_id: &str, item_id: &str, snapshot: &[u8]) -> Result<(), VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        Ok(active.store.put(&item::state_key(ws_id, item_id), snapshot)?)
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
