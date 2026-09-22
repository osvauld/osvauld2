# Node backlog — what we deferred to get sync working

**2026-09-21:** priority moved to making sync work end to end. Everything below was either
designed and not built, or surfaced while building something else and set aside. This file is
the running note; items leave it when they land or when a decision closes them.

Nothing here is a blocker for first sync. That is the point of the list.

---

## Deferred work, with the thinking already done

### Revocation cascade (was Gate 1 slice 5)

Revoking a token must kill what was minted under it. Half already works: `verify_chain` checks
the revoked set against every link, so a child carrying its parent in `prf` dies with it. The
gap is **flattened tokens** — the node reissues a root-signed token with no `prf` to keep chains
short, and nothing in that token points back at the authority that caused it. `Cause::Under(id)`
in `kunki::admin` is the surviving trace.

**Decided approach, not yet built: walk up lazily, do not cascade down eagerly.** At check
time, take the presented token's cause and walk upward — dead if any ancestor is dead. It is
O(lineage depth) rather than O(descendants), it writes nothing so a crash cannot half-apply it,
and a token minted under a dead authority is dead the moment it is minted rather than needing a
separate guard at issuance. Eager cascade can come later as a cache if per-request reads hurt.
Either way this lives in `kunki`: `verify_chain` takes a flat set and knows nothing about
storage, so the ancestor walk layers on top before `policy::authorize`.

Needs a visited set regardless of direction: causes should form a DAG, but nothing enforces it,
and a caller recording a cause that does not exist yet can construct a cycle.

### `role.assign` (was Gate 1 slice 6)

Node issuance of a role token: chain check, `RoleAssign` over the target, a **rank check so an
assigner cannot mint above its own role**, mint through `issue_root`, record it with
`Cause::Under(the assigner's token)`. Needed because delegation cannot change a role (§4), so
handing out an app role has to be node issuance — which also keeps it in the audit log.

### The `nodes/<node-did>/` half of the store

**Partly landed 2026-09-22** as `kunki::peer`: the relationship itself lives at
`nodes/<node-did>/relationship`, so a claimant can still name the node it claimed after a
restart and reconnect from what it kept. Before it, `desktop_finish_claim`'s return value was
dropped by every caller. Still missing: the tokens held from that node, and the revocations
that node announced.

**Correction to an earlier revision of this file**, which called it the identical shape to
`admin`. It is the mirror, not the twin. `admin` is what this account *issued* — it is the
authority, an issue is appended, and losing one loses the audit log. `peer` is what it
*holds* — it is the subject, it verifies rather than decides, and a reissued permit replaces
rather than accumulating. Same storage primitives, opposite direction; two modules rather
than one generic one.

It sits in `kunki` because a node needs it under federation, not only a desktop — a node
holds permits from the nodes it federates with exactly as a desktop holds them from its
nodes. It moves out to a crate `shell2` shares once shell2 actually calls it.

### Extracting the shared substrate from `shell2`/`app_host`

The node is the desktop's substrate minus the UI — Loro to merge, mlua to run rules, the same
vault underneath — so both halves eventually come out into headless crates. Already recorded in
§10 item 4 of the permissions design and in the Luau-sandbox note there. **Decided 2026-09-21:
not before sync works.**

For sync the node needs neither. Loading a sealed snapshot into a `LoroDoc`, importing updates,
exporting deltas, and writing the snapshot back is about fifty lines and touches no `app_host`
code; rules only arrive once the node adjudicates merges. And with one caller, an extraction
extracts *shell2's* shape — which is already visibly wrong for the node: shell2 opens a doc per
tab for editing, with undo and a UI-driven lifecycle, while kunki wants many docs at once, no
undo, no editing, and eviction. The honest shared part is those fifty lines.

The Lua half is the one that must become a genuinely shared crate rather than a copy, and for a
security reason rather than a duplication one: two Luau sandboxes that drift mean a rule that is
safe on the desktop is a denial of service on the node. It is also not a lift — `app_host`
depends on `runtime` (vello, parley), and its `crdt.rs` is `Rc`/`RefCell` throughout, so the
work is splitting the sandbox and the CRDT binding away from gfx. Easier to aim once the node
can say what it needs.

### `users/<did>/meta` profiles

The namespace exists and is pinned by a test; nothing writes it. When it does: a profile is
user-authored and node-stored, flows user → node, and is **never read as authority**. The DID
is the identity; a display name is a label.

### ~~The permit/token unification~~ — done 2026-09-22

Courier now has one credential mechanism. The node's grant is a `Token` (`CLAIM_ROLE` = `owner`
at `Scope::Node`, delegable, `CLAIM_TTL` of 30 days, reissued on reconnect); the claimant's
`permit_for_node` is a `Token` too, rooted at the claimant, with its key material in
`Claims.binds` and a role (`relationship`) that no policy-table entry matches. `PermitClaim`,
`issue_permit`, `verify_permit` and `PERMIT_DOMAIN` are deleted.

One decision left open by it: a reissue is recorded as a new `Issue`, so the log grows by one
per reconnect and the replaced token stays listed as though live. Under-reporting what is live
would be worse, so it stands until there is a reason to prune.

### Raised by the osvauld1 reading (2026-09-22)

[`osvauld1-prior-art.md`](osvauld1-prior-art.md) §8 holds these with their reasoning. In
short: **multi-device** (the claim binds one device key and refuses a second claim, so "another
machine belonging to an existing admin" is currently inexpressible), **the identity record**
(`users/<did>/meta` should probably be v1's contacts row — name and devices — and the desktop
needs the same record for people it shares no node with), **re-publish disagreement** (nothing
defines what happens when a header the node holds contradicts one being announced), and
**layer granularity** (v1 syncs `(page_id, layer_name)`; our `meta`/`src`/`doc/<name>` keys are
nearly the same decomposition, worth confirming before the sync slice rather than during it).

---

## Smaller things surfaced in passing

- **Courier's vocabulary assumes a desktop** — `desktop_did`, `desktop_start_claim`,
  `DesktopNodeRecord`. The mechanism is already symmetric (both sides trade a permit with a DID
  and keys), so this is a mechanical rename, waiting for a second kind of caller to justify it.
- **`kunki::admin` keys assume a DID contains no `/`.** True for `did:key` (base58btc has no
  slash), unasserted anywhere. If it were false, holder `a` and holder `a/b` would share a
  prefix scan. Close it by validating on record or hashing the DID into the key.
- ~~**`vault/examples/seed_demo.rs:79` fails clippy**~~ — fixed 2026-09-22 while adding
  `adopt_workspace`. It is a `deny` lint, so it broke `cargo clippy -p vault --all-targets`
  outright, which made the crate's real lints unreadable the moment we started touching it.
- **`result_large_err`** fires 45 times across the workspace, `kunki` included. Consistent with
  the codebase to leave it; noted so it is a decision rather than an oversight.

---

## Open questions (no answer yet, not just unbuilt)

**Federation** — whether a relaying node needs its own membership token from the home node in
addition to the user token delegated to it; how a node learns another node's transport address
(`did:key` is self-authenticating for keys, so this is an address book, not a resolver);
whether a home node can migrate.

**Signatures on records** — the canonical encoding a record signature covers (field order, and
the form of the address inside it); whether a signed record is ever amended in place rather
than versioned.

**Authority** — who may revoke a link (node only, or also its issuer); the creator's initial
authority on a new workspace; who may declare a namespace.

**Rules and updates** — whether an update records the writing app; how a deny list is
represented; re-running read rules when the group membership they read changes; client
rollback/pending UX for rejected updates.
