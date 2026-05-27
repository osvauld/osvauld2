# osvauld2 conventions

Minimal, by-concern crates, built one slice at a time and inspired by the older
codebase rather than copied from it.

## Code
- Keep it minimal. No boilerplate doc-blocks (no Context / Peer-sends / We-verify headers).
- Comments only where the *why* is non-obvious. Never narrate what the code already says.
- Idiomatic Rust: `?`, combinators, real types over stringly-typed data.
- Each crate wraps its own errors with `thiserror`.
- Secrets are zeroized after use; derived symmetric keys never outlive the operation.

## Tests
- Unit tests live in `src/<module>/tests.rs`, wired with `#[cfg(test)] mod tests;`.
  Source files stay free of test code.
- Integration tests live in their own crate (added in a later slice).
- Standard `#[test]` harness; reach for a framework only when it earns its place.

## Docs
- Prose docs live in `docs/`. Document the *contracts and decisions* — on-disk formats,
  key-derivation contexts, threat models, trust boundaries — not the code.

## Crates (build order)
`cryptography` → `identity` → `storage` → `transport` → `actors` → `renderer` →
`shell` → `node` → integration tests → automation tests.

## Trust model
The node is trusted: if the node is compromised, everything is. We do not design to be
tamper-evident against the node. The threat is malicious peers. Permits are our own
signed structs (Ed25519 over a canonical encoding + a domain-separation tag); the issuer
DID self-authenticates via its embedded public key.
