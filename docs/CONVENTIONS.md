# osvauld2 conventions

Minimal, by-concern crates, built one slice at a time. Inspired by the older codebase
rather than copied from it — `docs/architecture.md` says what is live and what is
reference-only; this file says how we write.

## Code

- Work lands in reviewable slices: **one write lands at most ~100 new or changed lines of
  code** (tests ride along free). A task that needs more is broken down first — thought
  through, explained, then written chunk by chunk — so every landed piece can be judged in
  one sitting. The process half of this rule (who judges, when to pause) lives in
  `AGENTS.md`.
- Keep it minimal. No boilerplate doc-blocks (no Context / Peer-sends / We-verify headers).
- Comments only where the *why* is non-obvious — and the bar is high: the best comments here
  record a decision someone will be tempted to reverse (see `props.rs` on `grow`, or
  `model.lua`'s unguarded resize write). Never narrate what the code already says.
- Idiomatic Rust: `?`, combinators, real types over stringly-typed data.
- Each crate wraps its own errors with `thiserror`.
- Secrets are zeroized after use; derived symmetric keys never outlive the operation.
- Each live crate opens with a `//!` header: what it is, its boundary, the invariants it
  keeps — contract-level, a few lines, never rustdoc boilerplate.

### Lua

Apps are Luau. Tabs for indentation, `local` everywhere, palette and geometry in a
`theme.lua` module required by everything that draws. Positional table entries are the child
list and their order is meaning. How to write an app is `docs/lua-apps.md` — the author's
  guide, written for app authors, not host contributors.

## Tests

- Unit tests live in `src/<module>/tests.rs`, wired with `#[cfg(test)] mod tests;`.
  Source files stay free of test code.
- Standard `#[test]` harness; reach for a framework only when it earns its place.
- Lua behavior is pinned by `app_host`'s suite — including the round-trip tests that
  re-print the kanban corpus and require identical element trees. Keep it green.
- Small repro apps go in `demo_apps/`.

## Docs

Prose docs live in `docs/` and record **contracts and decisions** — on-disk formats,
key-derivation contexts, threat models, trust boundaries, status — not code walkthroughs.

Layout:

- `docs/*.md` — current truth: architecture, the Lua contract, status, per-crate contracts
  (vault, identity, storage), these conventions.
- `docs/design/` — living design notes. Each opens with a **status line** saying what is
  built; when a design decision is revised, the doc keeps the old section with a dated
  revision note rather than silently rewriting (see `code-as-tree.md` §13 for why).
- `docs/archive/` — history, read for lessons, never followed without checking the live
  docs. Its README maps era and reason.
- `docs/status.md` is the progress ledger — update it as things land; leave the archive
  alone.

## Crates

Workspace members are the live set (`architecture.md` carries the table and the one-line
roles). Everything else in the tree is **reference-only**: port its lessons, never its code,
and never extend it.

## Trust model

The node is trusted: if the node is compromised, everything is. We do not design to be
tamper-evident against the node. The threat is malicious peers. Permits are our own signed
structs (Ed25519 over a canonical encoding + a domain-separation tag); the issuer DID
self-authenticates via its embedded public key. Access-control design is future work
(`docs/archive/osv.md` holds the shape; it returns with permits and sync).
