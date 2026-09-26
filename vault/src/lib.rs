//! Headless account manager: composes `identity` + `storage` into signup, login and
//! account-switching, holds the one unlocked `Identity` for the session, and owns the account's
//! item tree — workspaces, items, sealed src/doc records. The only auth code in the tree;
//! `shell2` drives it today, the future node will too. Loro-free by design: snapshots are
//! opaque sealed bytes here, Loro docs live in the caller. See `docs/vault.md`.

mod account;
mod entry;
mod error;
mod item;
mod workspace;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use identity::{Identity, Keystore, Mnemonic, Signer};
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
    pub fn prepare_signup(label: &str, passphrase: &str) -> Result<PreparedAccount, VaultError> {
        let (identity, mnemonic) = identity::generate();
        let keystore = identity::seal(&identity, passphrase)?;
        Ok(PreparedAccount {
            identity,
            mnemonic,
            label: label.to_string(),
            keystore,
        })
    }

    pub fn commit_signup(
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
        let unlocked = self.prepare_login(did, passphrase)?;
        self.commit_login(unlocked)
    }

    /// Slow half (Argon2): opens the keystore read-only and decrypts it. Takes no `&self`, so
    /// the UI can run it on a worker thread; pair with [`Vault::commit_login`].
    pub fn prepare_login(
        &self,
        did: &str,
        passphrase: &str,
    ) -> Result<UnlockedAccount, VaultError> {
        let path = self.dir.join(did_to_filename(did)?);
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

    pub fn commit_login(&mut self, unlocked: UnlockedAccount) -> Result<(), VaultError> {
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

    /// Adopt a workspace that originated in another account, keeping the id and creation time
    /// it already has. [`create_workspace`](Self::create_workspace) is for a workspace this
    /// account originates and mints an id for; this is for one published to it, where both
    /// ends must name the same workspace or nothing either says about it refers to the same
    /// thing. Replaces any header already stored under that id — the originating account owns
    /// the header, and a republish is how it changes.
    ///
    /// Errs with [`VaultError::BadWorkspaceId`] for an id this account would not have minted,
    /// because the id becomes a key; see `workspace::is_minted_id`. Errs with
    /// [`VaultError::Locked`] when no account is unlocked.
    pub fn adopt_workspace(&self, meta: &WorkspaceMeta) -> Result<(), VaultError> {
        if !workspace::is_minted_id(&meta.id) {
            return Err(VaultError::BadWorkspaceId(meta.id.clone()));
        }
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let plaintext = serde_json::to_vec(meta)?;
        let sealed = active.seal(&plaintext)?;
        active.store.put(&workspace::meta_key(&meta.id), &sealed)?;
        Ok(())
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

    /// Adopt an item that originated in another account, keeping its id, workspace and creation
    /// time — the item-level counterpart of [`adopt_workspace`](Self::adopt_workspace), and for
    /// the same reason: a published item and the desktop's own record of it must name the same
    /// thing. Replaces any header already stored under that id.
    ///
    /// Errs with [`VaultError::BadItemId`] for an id, or a `ws_id`, this account would not have
    /// minted — both become part of the key. Errs with [`VaultError::Locked`] when no account
    /// is unlocked.
    pub fn adopt_item(&self, item: &WorkspaceItem) -> Result<(), VaultError> {
        if !workspace::is_minted_id(&item.id) || !workspace::is_minted_id(&item.ws_id) {
            return Err(VaultError::BadItemId(item.id.clone()));
        }
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        let plaintext = serde_json::to_vec(item)?;
        let sealed = active.seal(&plaintext)?;
        active
            .store
            .put(&item::meta_key(&item.ws_id, &item.id), &sealed)?;
        Ok(())
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
    /// `ws_id`/`item_id` come from a caller synced or published from elsewhere — never minted
    /// here — and both become part of the key, so they're checked the same way
    /// [`adopt_item`](Self::adopt_item) already checks them, not trusted the way a purely
    /// local read would be. Without this, `item_id = "X/doc"` with `SyncLayer::Src` and
    /// `item_id = "X"` with `SyncLayer::Doc("src")` land at the same key: an id this account
    /// never minted, used to alias a different item's layer.
    pub fn get_src(&self, ws_id: &str, item_id: &str) -> Result<Option<Vec<u8>>, VaultError> {
        if !workspace::is_minted_id(ws_id) || !workspace::is_minted_id(item_id) {
            return Err(VaultError::BadItemId(item_id.to_string()));
        }
        self.get_sealed(&item::src_key(ws_id, item_id))
    }

    /// Same check as [`get_src`](Self::get_src), for the same reason.
    pub fn put_src(&self, ws_id: &str, item_id: &str, snapshot: &[u8]) -> Result<(), VaultError> {
        if !workspace::is_minted_id(ws_id) || !workspace::is_minted_id(item_id) {
            return Err(VaultError::BadItemId(item_id.to_string()));
        }
        self.put_sealed(&item::src_key(ws_id, item_id), snapshot)
    }

    /// Same check as [`get_src`](Self::get_src), for the same reason.
    pub fn get_doc(
        &self,
        ws_id: &str,
        item_id: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>, VaultError> {
        if !workspace::is_minted_id(ws_id) || !workspace::is_minted_id(item_id) {
            return Err(VaultError::BadItemId(item_id.to_string()));
        }
        self.get_sealed(&item::doc_key(ws_id, item_id, name))
    }

    /// Same check as [`get_src`](Self::get_src), for the same reason, plus the `name` check
    /// this already had.
    pub fn put_doc(
        &self,
        ws_id: &str,
        item_id: &str,
        snapshot: &[u8],
        name: &str,
    ) -> Result<(), VaultError> {
        if !workspace::is_minted_id(ws_id) || !workspace::is_minted_id(item_id) {
            return Err(VaultError::BadItemId(item_id.to_string()));
        }
        if name.is_empty() || name.contains('/') {
            return Err(VaultError::InvalidName(name.to_string()));
        };
        self.put_sealed(&item::doc_key(ws_id, item_id, name), snapshot)
    }
    /// Opaque sealed records this account keeps for itself, outside the workspace/item tree:
    /// the node's tokens and revocations, a desktop's node relationships. Vault seals and
    /// stores them without interpreting them, under a reserved key namespace.
    pub fn put_entry(&self, name: &str, bytes: &[u8]) -> Result<(), VaultError> {
        self.put_sealed(&entry::key(name)?, bytes)
    }

    pub fn get_entry(&self, name: &str) -> Result<Option<Vec<u8>>, VaultError> {
        self.get_sealed(&entry::key(name)?)
    }

    pub fn delete_entry(&self, name: &str) -> Result<(), VaultError> {
        let key = entry::key(name)?;
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        Ok(active.store.delete(&key)?)
    }

    /// Names under `prefix`, sorted. A plain prefix scan, so `order` also matches `orders/1` —
    /// end a prefix with `/` to keep namespaces apart.
    pub fn list_entries(&self, prefix: &str) -> Result<Vec<String>, VaultError> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(VaultError::Locked)?;
        Ok(active
            .store
            .list_prefixed(&entry::scan(prefix))?
            .iter()
            .filter_map(|key| entry::name_of(key))
            .map(str::to_string)
            .collect())
    }

    /// Sign as the unlocked account without handing the identity out. The account is held for
    /// the closure, so the closure must not call back into this vault — sign, return, then
    /// write. `None` when locked, which is what makes this safer than lending a signer out:
    /// nothing can sign once the account is gone.
    pub fn with_signer<R>(&self, f: impl FnOnce(&dyn Signer) -> R) -> Option<R> {
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref()?;
        Some(f(&active.identity))
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
