# Vault

The headless account manager. It composes `identity` and `storage` into signup, login,
and account-switching, and holds the one **unlocked `Identity`** for the session. Lives in
the `vault` crate, which depends on `identity` and `storage`; it has no UI and touches no
network.

`vault` is the *only* auth code in the tree — both the desktop shell (`sthalam`) and the
always-on node (`kunki`) drive it, with different front ends over the same verbs. And it is
**auth only**: it does not own spaces, documents, or app storage. Those are a separate
services layer added later (also headless, also shared with `kunki`). Keeping the account
lifecycle in its own small crate is what stops the old monolithic `butler` from reassembling.

## What it is (and isn't)

```rust
Vault::open(dir)                       // create_dir_all(dir); opens no account db yet
vault.is_empty()        -> bool        // any account on disk? — filename scan, opens nothing
vault.accounts()        -> Vec<AccountInfo>      // {did, label} for each account

vault.signup(label, pass) -> (did, Mnemonic)     // new identity → new db → active; show mnemonic ONCE
vault.login (did, pass)   -> ()                   // open that db → unlock → active
vault.switch(did, pass)   -> ()                   // = lock + login
vault.lock  ()                                    // drop the active account

vault.identity()        -> Option<&Identity>     // for signing / encryption, once unlocked
vault.current()         -> Option<AccountInfo>    // did + label, for "logged in as …"
```

| Concern | Where it lives |
|---|---|
| Key derivation, sealing, DID | `identity` (vault calls `generate` / `seal` / `unlock`) |
| Bytes on disk | `storage` (vault calls `Store::get` / `put`) |
| Who prompts for a passphrase | the driver — `sthalam` egui forms, `kunki` env/file |
| Spaces, documents, app storage | a later services layer — **not** vault |

## Accounts on disk

One **redb file per account**, all under a single directory:

```
<data_dir>/osvauld/
   z6MkpTH…r9.redb        ── filename = the DID with the "did:key:" prefix stripped
        ├─ "identity/keystore" → Keystore::to_bytes()    passphrase-sealed (Tier-0)
        └─ "identity/label"    → "Abe (work)" as UTF-8    cleartext display name
   z6Mng4F…2x.redb        another account, fully independent
```

`data_dir` is the OS per-user data directory plus `osvauld` (`dirs::data_dir()/osvauld`, e.g.
`~/.local/share/osvauld`), overridable via the `OSVAULD_DATA_DIR` env var so several instances
can run on one machine for P2P testing. Vault doesn't parse arguments — the driver resolves the
path and hands it to `open`; `vault::default_dir()` supplies the default.

**Filename = DID, not username.** The DID's multibase form (`z…`, base58btc) is
filesystem-safe on every OS, whereas a `did:key:`-prefixed string carries colons (illegal on
Windows) and a username carries arbitrary characters. Naming by DID is also identity-stable
(renaming an account never moves the file) and yields one file per identity (re-importing the
same mnemonic maps to the same file instead of forking local state). The human label is *data
inside* the db, so it can be any string and is the seed for a future synced display name.

## Two encryption tiers

The keystore is special because it is the **bootstrap**:

| Tier | What | Gate |
|---|---|---|
| 0 | the keystore (`identity/keystore`) | a **passphrase** (Argon2id → AES-GCM) |
| 1 | everything else (spaces, documents — later) | the **unlocked identity's key** (per-doc AES, ECIES-to-self) |

Tier-0 is the only blob readable with a passphrase alone, and the only thing vault writes in
the auth slice. The DID and public keys sit in the clear *inside* the keystore, so the DID is
known before unlock; only the three secret keys are sealed. `identity/label` is cleartext
metadata stored alongside it — a local display name, not a secret.

## State

Session state is one field: `active: Option<Active>`.

- `None` — locked, or nobody has logged in. The driver shows signup (when `is_empty()`) or login.
- `Some(Active)` — an account is open: its unlocked `Identity`, its open `Store`, and its
  `did` + `label` **cached**, so `accounts()` never has to re-open the live db (see the last
  section).

`lock()` returns it to `None`; dropping `Active` closes the `Store` (releasing redb's file
lock) and drops the `Identity` (zeroizing its secrets).

## Flows

**Signup** — `generate()` a fresh identity, create `<did>.redb`, write the sealed keystore and
the label, become active, and return the **mnemonic for one-time display**. The mnemonic is
never stored; it is the only path to recovery.

**Login** — guard that `<did>.redb` exists, open it, read and `unlock` the keystore with the
passphrase (a wrong passphrase fails the AEAD tag → `WrongPassphrase`), cache the label, become
active.

**Switch** — `lock()` then `login()` the target. One account is active at a time (the
browser-profile model); switching re-prompts, because **each account has its own passphrase**.

**Recover** (driver UI added later) — `identity::recover(words)`, then `seal(new_pass)` and
write: the same write path as signup. This is the *only* way back in after a forgotten
passphrase. The keystore stores sealed **keys**, not the seed, and HKDF is one-way, so **there
is no passphrase reset** — signup must say so when it shows the mnemonic.

## API

```rust
// constructor + queries
Vault::open(dir: PathBuf)             -> Result<Vault, VaultError>
vault.is_empty()                      -> bool
vault.accounts()                      -> Result<Vec<AccountInfo>, VaultError>
vault.identity()                      -> Option<&Identity>
vault.current()                       -> Option<AccountInfo>

// state transitions (&mut self)
vault.signup(label: &str, pass: &str) -> Result<(String, Mnemonic), VaultError>
vault.login (did: &str,   pass: &str) -> Result<(), VaultError>
vault.switch(did: &str,   pass: &str) -> Result<(), VaultError>
vault.lock  ()

pub struct AccountInfo { pub did: String, pub label: String }
```

The passphrase is borrowed (`&str`); zeroizing the caller's copy is the driver's job (`identity`
already zeroizes the derived key it uses internally). DIDs are `String`, matching `identity`.

`VaultError`: `WrongPassphrase` (lifted to the top so a driver matches one variant),
`NoSuchAccount(did)`, `NoKeystore`, `BadDid(did)`, and transparent `Identity` / `Storage` / `Io`
wrappers.

## Labels are local, not your network name

`identity/label` is what *this device's* account picker shows. It never leaves the machine and
is not authenticated. The name *peers* see — in a group chat or a member list — is a different
thing: a signed profile field that syncs, added with the app layer. The two are not the same.

## Trust model

The node is trusted (per [CONVENTIONS](CONVENTIONS.md)); vault inherits that. Specifically:

- The keystore defends only against an attacker who holds its bytes but not the passphrase
  (Argon2id + AES-GCM). It is not tamper-evident against its owner.
- **No passphrase reset.** A forgotten passphrase means re-importing the 24-word mnemonic.
- Accounts are isolated by file *and* by key: Tier-1 data is encrypted to each identity, so one
  account cannot read another's even with filesystem access.

## One change above vault: `storage` grows `open_readonly`

`accounts()` reads each account's label, which for a non-active db means opening it just to read
one key. The normal `Store::open` creates-if-missing and runs a write transaction to ensure the
table — wrong for a read-only peek, and impossible while a write handle is live. So `storage`
gains a small `open_readonly` (`Database::open`, no init). Because redb allows one handle per
file, vault reads the *active* account's label from the cached `Active.label` and only
`open_readonly`s the others; `login` guards on `path.exists()` so a bad DID can't leave a stray
empty db behind.

## Module layout

```
vault/src/
  lib.rs        Vault + open / is_empty / accounts / signup / login / switch / lock / identity / current
  account.rs    AccountInfo, did↔filename mapping, the directory scan
  error.rs      VaultError
  tests.rs      signup → login → switch → wrong-pass → accounts, over a tempfile::TempDir
```

Tests run on a real temp directory rather than `Store::open_in_memory`, because the
one-file-per-account model can't be expressed by a single pathless in-memory db; the byte
layer's own in-memory tests already cover `storage`.
