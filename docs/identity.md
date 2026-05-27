# Identity

How a user's identity is created, derived, persisted, and unlocked. Lives in the
`identity` crate, which depends only on `cryptography`. No storage/redb, no network.

## What an identity is

A single BIP39 mnemonic is the root secret. Everything else is derived from it, so
the mnemonic alone is enough to recover a full identity on a new device.

From the mnemonic we derive three keypairs, each for one job:

| Key        | Curve   | Used for                                  |
|------------|---------|-------------------------------------------|
| signing    | Ed25519 | signing (permits, messages); **the DID**  |
| encryption | X25519  | ECIES seal/open (reading shared data)     |
| device     | Ed25519 | transport / node identity (iroh)          |

The device key is derived from the seed (not random) so it too is recoverable from
the mnemonic.

## Derivation (the forever-contract)

```
seed (64 bytes) = BIP39.to_seed(mnemonic, passphrase = "")

signing_secret    = HKDF-SHA256(ikm = seed, salt = none, info = "osv/identity/signing/v1")
encryption_secret = HKDF-SHA256(ikm = seed, salt = none, info = "osv/identity/encryption/v1")
device_secret     = HKDF-SHA256(ikm = seed, salt = none, info = "osv/identity/device/v1")

signing_public    = Ed25519.public(signing_secret)
encryption_public = X25519.public(encryption_secret)
device_public     = Ed25519.public(device_secret)
```

The `info` strings are domain-separated and versioned. Changing any of them changes
the derived key — and the DID — for every existing user. They are frozen.

## DID

Self-authenticating `did:key`, no resolution or PKI needed: the public key is encoded
into the string itself.

```
DID = "did:key:z" + base58btc( [0xed, 0x01] || signing_public )
```

`[0xed, 0x01]` is the multicodec prefix for an Ed25519 public key. Decoding reverses
this: strip the prefix, base58-decode, check length 34 and the multicodec bytes, take
the trailing 32 bytes.

The DID is built from the **signing** key. Encrypting *to* a peer uses their
**encryption** (X25519) key, which is a different key — never wrap data to a DID.

## Keystore (at rest)

The identity is persisted as a single self-contained, versioned, bincode-encoded file.
Public keys and the DID are stored in the clear so the DID can be displayed before the
user unlocks; only the three secret keys are sealed.

```
EncryptedKeystore {
    version: u8 = 1
    did: String
    public_signing_key, public_encryption_key, public_device_key: [u8; 32]
    sealed_signing, sealed_encryption, sealed_device: Vec<u8>   // AES-256-GCM: nonce(12)|ct|tag(16)
    kdf_salt: [u8; 16]
    argon2_m, argon2_t, argon2_p: u32                           // = 65536, 3, 4
}
```

Sealing:

```
kek = Argon2id(passphrase, kdf_salt, m=65536, t=3, p=4) -> 32-byte key
sealed_X = AES-256-GCM.encrypt(kek, X_secret)
```

The Argon2 parameters and salt are stored in the keystore so `unlock` re-derives the
same `kek`. The `kek` is zeroized after use; on unlock, a failed AEAD tag is reported
as `WrongPassphrase` (we don't leak the underlying cause).

## API

The crate is **I/O-free**: it never touches the filesystem or a database. `seal`
produces a `Keystore`; `Keystore::to_bytes` serializes it; the caller persists those
bytes wherever it likes (a file, redb selected by `-d`, memory). `unlock` consumes a
`Keystore`. Persistence is a separate concern (the `storage` slice), and `identity`
does not depend on it — keeping its dependency graph to just `cryptography`.

**Free functions** (the verbs — transforms over data):

```rust
generate()                  -> (Identity, Mnemonic)   // fresh identity + its mnemonic (show once)
recover(phrase)             -> Identity               // rebuild from a known mnemonic
seal(&Identity, passphrase) -> Keystore               // Identity -> passphrase-sealed keystore
unlock(&Keystore, passphrase) -> Identity             // keystore + passphrase -> Identity
verify(public, message, sig) -> bool                  // check a peer's signature
encrypt_for(recipient_encryption_key, plaintext) -> Vec<u8>   // ECIES seal to a peer
did_from_public_key(signing_public) -> String
public_key_from_did(did) -> Option<[u8; 32]>
```

**`Identity` methods** (accessors + the two operations that need *your secret*):

```rust
did() -> &str
signing_public_key() / encryption_public_key() / device_public_key() -> [u8; 32]
sign(message)        -> [u8; 64]    // needs the secret signing key
decrypt_sealed(data) -> Vec<u8>     // needs the secret encryption key
```

**`Keystore`**: `to_bytes()`, `from_bytes(&[u8])`, `did()` (readable before unlock).

The split is principled and mirrors the cryptography: operations that need a **secret
key** (`sign`, `decrypt_sealed`) are methods on the `Identity` that holds it; operations
that need only **public keys** (`verify`, `encrypt_for`) are free functions. `generate`
and `recover` share the tail "mnemonic -> Identity"; sealing is the separate `seal` step.

"Sign up" / "log in" are the *shell's* user-facing flow names; they compose these
(`generate` + `seal` + store the bytes; load bytes + `unlock`).

## Trust model

The node is trusted; a compromised keystore or host is total compromise. The keystore
defends only against an attacker who has its bytes but not the passphrase (Argon2id +
AES-GCM). It is not designed to be tamper-evident against its owner.

## Module layout

```
identity/src/
  lib.rs        generate / recover / verify / encrypt_for, re-exports
  error.rs      IdentityError (WrongPassphrase, Bip39, Decode, Crypto)
  did.rs        did_from_public_key / public_key_from_did   (+ did/tests.rs)
  identity.rs   Identity: derivation, accessors, sign/decrypt_sealed (+ identity/tests.rs)
  keystore.rs   Keystore + seal / unlock / to_bytes / from_bytes      (+ keystore/tests.rs)
  tests.rs      in-memory generate -> seal -> unlock and recover flows
```
