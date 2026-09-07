---
name: expert-secure-core
description: "Reviewer for osvauld2's security-sensitive crates — vault/, identity/, storage/, cryptography/. Use when a diff touches accounts, keys, sealing, keystores, or anything that handles secrets."
---

# Secure-core review

Read the diff cold. Output `BLOCKER` / `SHOULD` / `NOTE`; you advise, the user judges.
Contracts: `docs/vault.md`, `docs/identity.md`, `docs/storage.md`; trust model in
`docs/CONVENTIONS.md`.

1. **Zeroize.** Secrets are zeroized after use; derived symmetric keys never outlive the
   operation. A secret held in a `String`/`Vec` that outlives its purpose is a BLOCKER.
2. **Sealed at rest.** Everything below the keystore in an account db is sealed to the
   account's encryption key (`encrypt_for`). Any plaintext record (workspace, item, source,
   doc) is a BLOCKER.
3. **Identity's forever-contract.** HKDF `info` strings (`osv/identity/*/v1`) and the
   Argon2 parameters are frozen — changing any changes every user's keys and DID. A diff
   that touches derivation is a BLOCKER unless it introduces an explicitly new version.
4. **Keystore handling**: Argon2 params and salt travel in the keystore so `unlock`
   re-derives; a failed AEAD tag surfaces as `WrongPassphrase` (never the underlying
   cause); the DID is readable before unlock (public by design).
5. **`identity` stays I/O-free** — no filesystem, no db. `storage` stays dumb bytes — no
   serialization, no crypto, no inspection. `vault` stays Loro-free — snapshots are opaque
   sealed blobs. Layering violations in either direction are a SHOULD.
6. **redb single-handle rule**: the active account's store is held for the session; other
   accounts open read-only, briefly, for label reads. Long-lived second handles will
   deadlock or corrupt.
7. **Threat model**: the node/host is trusted; the threat is malicious peers. Do not add
   defenses against the owner's own machine, and do not *weaken* peer-facing guarantees —
   permits are signed structs (Ed25519, canonical encoding, domain tag) when they arrive.
