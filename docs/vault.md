# Vault

The headless account manager: composes `identity` + `storage` into signup, login, lock —
and owns the account's **item tree**: workspaces, typed items, and their sealed records.
No UI, no network. `shell2` is the driver today (the future node, *kunki*, will drive the
same verbs).

## What it is (and isn't)

- The **only** auth code in the tree.
- **Loro-free by design.** An app's source and state docs are opaque sealed snapshots to
  the vault; the shell builds/imports/persists the Loro docs themselves.
- Single-writer: listings are prefix scans over the store; no registry doc. A locked
  decision — the index becomes a CRDT when sync does.

| concern | where it lives |
|---|---|
| key derivation, sealing, DID | `identity` (`vault` calls `generate`/`seal`/`unlock`) |
| bytes on disk | `storage` (`Store::get`/`put`, one redb file per account) |
| who prompts for a passphrase | the driver — `shell2` screens |

## API

```rust
Vault::open(dir)                                  // create_dir_all; opens no account db yet
vault.is_empty() / accounts()                     // {did, label}, by filename scan
vault.signup(label, pass)  -> (did, Mnemonic)     // mnemonic shown ONCE, by the driver
vault.login(did, pass)                            // unlock → active
vault.lock() / current() / store()                // the active account's Store
vault.create_workspace(name) / workspaces()       // newest first
vault.create_item(ws, name, kind) / items(ws)     // ItemKind::{Doc, Table, App, Canvas}
vault.get_src / put_src(ws, item, snapshot)       // an .app's source doc (Loro snapshot)
vault.get_doc / put_doc(ws, item, snapshot, name) // its state docs, name-keyed
```

The Argon2 halves (`prepare_*`/`commit_*`) are split so hashing can run off the UI thread
and only the commit needs `&mut`. They are private today: `shell2` spawns a worker that
runs the composed `signup`/`login` (`Vault` is `Clone`; all state sits behind one mutex,
so the clone is the same vault) and pokes the event loop when done.

## On disk

One redb file per account, under the vault dir (`default_dir()` = `<os-data-dir>/osvauld`,
`OSVAULD_DATA_DIR` overrides for throwaway dev stores). **Every record below the keystore
is sealed to the account's encryption key:**

```
identity/keystore                 Argon2-passphrase-sealed identity
identity/label                    display name (plain)
ws/<id>/meta                      WorkspaceMeta { id, name, created }
ws/<ws>/item/<id>/meta            WorkspaceItem { id, ws, name, kind, created }
ws/<ws>/item/<id>/src             an .app's source doc — a Loro snapshot, sealed
ws/<ws>/item/<id>/doc/<name>      its state docs, name-keyed (no `/`, non-empty)
```

redb allows a single handle per file: the active account's store is held open for the
session; any other account is opened read-only, briefly, to read its label.

## Threat boundary

The store is at rest encrypted to the account key; the node-trust model is
`CONVENTIONS.md`'s — the threat is malicious peers, not the host.
