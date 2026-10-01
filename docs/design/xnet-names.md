# Xnet names — a name in the bar resolves to a public invite

**Status (2026-10-01):** chunks 1 (public invite, `courier`) and 2 (registry contract,
`xnet_names/`, live on devnet at `0xd1d80a37…abcfa3`) and 3 (`shell2/src/names.rs` +
`node::join`) built. Joining reaches only a node on the **same machine**: shell2 talks to
kunki over its local socket and nothing dials a ticket's `node_id` yet (no iroh in the tree).
Devnet is wiped periodically — redeploy, the address changes.

## 1. The idea

A name is a public URL for a workspace. Typing `acme` in Sthalam's bar looks the name up
on-chain, gets back a **public invite ticket**, and redeems it — the visitor joins at
whatever low-power role the owner published. A returning visitor reconnects with the token
they already hold and never touches the chain again.

```
bar "acme" → Resolver → ticket text → verify node signature → iroh dial → redeem → in
```

The chain holds **where** and **how to join**, never authority. Role, scope and what a
visitor may see stay the Osvauld protocol's job, decided by the node at redemption.

## 2. What the chain guarantees

Only the name's owner can change what it points to. The ticket's node signature proves
"this node issued it"; it cannot prove "this is the node the name meant" — that binding is
the registry's.

Never on-chain: `ConnectionTicket` (its `claim_token` makes the redeemer owner) or any
single-use invite. A public chain is readable forever by everyone.

## 3. Chunk 1 — public invite (`courier`)

Today's `InviteTicket` is single-use: `node_accept_invite` spends its nonce. A URL must work
for everyone, so a public invite:

- is **reusable** — its nonce is checked against `revoked`, never spent;
- carries only roles `role_could_gain_capability` rejects as powerless, as invites do now;
- is **rotated** by revoking the nonce and writing a fresh ticket to the name;
- is **rate-limited** by the node, since anyone can now mint a member against it.

Testable alone; the chain chunks are useless without it.

## 4. Chunk 2 — registry contract (Move, Aptos)

Why Aptos: accounts are Ed25519 natively, so a node's DID key *is* its owner account — no
second wallet. Fee-payer transactions let someone else pay gas. Reads are a plain
`POST /v1/view` returning JSON, so the resolver needs no chain SDK.

One module, one `Table<String, Record { owner: address, ticket: String }>`:

- `register(owner, name, ticket)` — fails if taken
- `update(owner, name, ticket)` — owner only; how a revoked invite is rotated
- `transfer(owner, name, new_owner)` — owner only
- `#[view] resolve(name): String`

Names are bare (`acme`). Who pays: on devnet each node funds itself from the faucet. On
mainnet, registration is free to users via Aptos Labs' hosted gas station (sponsors named
functions, with rate limits — product name to verify when we get there); we run no server.
Anti-squatting lives in those sponsorship rules for now, not on-chain.

Measured on devnet (2026-10-01, 502-byte ticket, gas price 100 octas): `register` 6404 gas =
0.0064 APT (~150 per APT — almost all of it the new slot's storage deposit, refunded if the
slot is ever deleted); `update` 85 gas = 0.000085 APT (~11 000 per APT); `resolve` free.

## 5. Chunk 3 — resolver + bar (`shell2`)

A `Resolver` trait (name → ticket text) with one Aptos implementation; the chain is swappable
behind it. The bar decides name vs ticket by the ticket prefixes (`osv1.`, `osvi1.`) — anything
else is a name.

*Revised 2026-10-01:* no trait — one implementation doesn't earn one; `names::resolve` is a
plain function, swapped by editing it. Endpoint and registry come from `OSVAULD_APTOS_URL` /
`OSVAULD_NAMES_REGISTRY`, defaulting to devnet. Names are **user-owned** for now (the desktop
already holds the key), so the "owner is the ticket's node" check is deferred: the worst case is
a squatted name pointing at a real node, never a forged ticket.

## 6. Decided

- **A visitor gets a member row** (2026-09-30). Anonymity comes from the identity, not from
  skipping the row: a DID is a fresh random key that names no one. The row lets an owner see
  who is in and later raise a visitor's role; the cost — anyone can mint rows — is the rate
  limit's job.

## 7. Open

- Record size: a ticket is a few hundred bytes of base64 JSON; storage deposit scales with it.
