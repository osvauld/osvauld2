# OSV — the app manifest

What an `.osv` is, what it declares, and the access model it expresses. The `.osv` is the
manifest the runtime reads to decide **who may read and write each document** — and little
else. It is a small, declarative access map, not a program.

> **Preliminary.** The shape is settled (boundaries, the three tiers, the rule vocabulary);
> spellings and a few mechanics (marked *open*) are not.

## The boundaries

```
workspace  — who collaborates: a set of members and their roles. One owner.
  page     — a data + rule boundary; holds layers and hosts multiple apps.
    layer  — one CRDT document (type); the unit access attaches to.
```

- **Workspace** is the membership boundary — "what is shared between users." Single-owner;
  this is community management (a shop, a forum, a team), **not** a social network: there is
  no follow graph and no per-user federation.
- **Page** carries the rules. Because a page hosts *multiple apps* that share its layers, the
  rules can't belong to any one app — they belong to the page.
- **App** is UI/code bound to a subset of a page's layers; many per page; never the rule unit.
- A user holds **many roles**; a decision takes their union.

## The one job, and the three tiers

The `.osv` declares only what the runtime must enforce *without running app code*: **access**
(who reads / writes each layer) plus a small set of **bounded rules** it can check generically.
Everything else lives in one of two other tiers. The dividing question is *how many independent
parties must run the logic identically*:

| Tier | Who runs it | For |
|------|-------------|-----|
| **`.osv`** | every peer — declarative, must converge | access + bounded rules |
| **node code** | the node alone — free-form, single authority | validate / derive / aggregate / integrate |
| **app code** | each client | UI, data shape, local logic |

- Access guards run on *every* peer (offline-first, no coordinator); if two peers ran code and
  disagreed, they would fork forever. So access must be declarative and deterministic.
- Node code runs on one authority and writes its result as ordinary synced data — nothing for
  other peers to disagree about — so it can be arbitrary: rankings, vote tallies, order totals,
  payment, email, scheduled jobs.
- **Litmus test for the `.osv`:** a rule belongs *iff* the platform can enforce it from
  `(the op, the actor, the record's settled fields)` alone. `edit by author` passes;
  `total == sum(items)` does not. This makes the vocabulary self-limiting — it cannot grow
  into a programming language.

## Layers are documents

A layer is one CRDT document type. Rule of thumb: **one document per unit of interaction** —
a post, an order, a DM, a channel — so a peer syncs only what it opens. Listings (feeds) are
*not* documents; they are UI over the **catalog** a peer already holds (its per-peer sync
metadata), sorted client-side, paged or ranked by the node when needed.

Dynamic instances are declared with a path template and discovered through the catalog:

```
layer order { namespace order/{buyer} … }   →  order/did:alice, order/did:bob, …
```

We keep hitting the same granularity decision everywhere — orders (per-buyer doc), groups
(per-group doc), posts (per-post doc), DMs (per-pair doc): **one document per unit of
interaction, plus the catalog to list them.** That is the data-model spine, independent of
the permit rules layered on top.

## Access

Read and write are enforced at **different points**, so asymmetric audiences are the normal
case, not a special one:

- **write** is gated at *apply* — a peer/node checks the writer's cap before merging the op.
- **read** is gated at *delivery* — the node only fans a layer's ops to holders of a read cap.

A write-only participant therefore gets *read-own for free*: it holds what it wrote and simply
never receives anyone else's. That is the whole **accumulation** pattern (orders, survey
responses): many write, few read, and "see only your own" is the *absence* of fan-out, not a
filter.

Audiences: `public` · `role R` · `path.<binding>` · `author` · a layer cap. Three gating modes:

| Mode | Access decided by | Example |
|------|-------------------|---------|
| **role-gated** | holding a workspace role | `read role member` |
| **path-gated** | your DID *is* the path/key (self-authorizing) | `order/{buyer}`, `dm/{participants}` |
| **permit-gated** | holding a per-instance layer permit | private groups |

Path-gating is what makes thousands of dynamic instances cheap: no permit is minted per
instance — the path segment authorizes its owner directly.

## Roles

Workspace-level, multi-valued. A role's only declarative content is `grants` — which roles its
holders may issue to others (delegation, attenuating: you cannot grant past your own reach).
The owner is the implicit root and grants everything.

## Permits — the cryptographic ACL

Permits are **our own signed structs** (Ed25519 over a canonical encoding + a domain-separation
tag; the issuer DID self-authenticates via its embedded public key — see `cryptography` /
`identity`). They are **self-contained capability tokens**: a permit carries its holder's caps,
so enforcement reads the *permit*, never the manifest.

- **Synced, not requested.** A permit is a row in the per-peer catalog (the upgraded sync
  metadata), keyed by layer; its value is the signed cap **plus the layer key wrapped for that
  holder**. Grant = write the row; upgrade = replace it; revoke = delete it + re-key the
  survivors. There is no separate key store — keys ride in the rows.
- **Issued by the node, under delegation.** The owner (or a layer's creator) signs a token
  delegating "may issue cap C for this layer" to the node; the node mints leaf permits whose
  proof chains back to the owner. An online owner/admin may also sign directly.
- **Validated on apply, everywhere.** Every peer re-checks each entry before merging: signature
  valid, issuer authorized by its own chain, the grant permitted. A shared CRDT does not mean
  writes are trusted — a forged or over-reaching entry fails the gate and never merges.
- **Membership is the set of rows.** "Who's in this group?" = the keys of its permit rows. The
  node holds the authoritative peer×layer matrix (its fan-out table) and projects each peer's
  own row into that peer's catalog (need-to-know by default).

Self-contained permits dissolve most version skew: because enforcement reads the permit and not
the manifest, two peers on different manifest versions cannot disagree about an already-issued
permit. The only ordering rule is local: a peer must have synced a (re)issued permit before it
can validate an op that leans on it — the node gates fan-out to guarantee that.

## Encryption tiers

There is no E2E *flag*. A layer is node-readable **iff its key was wrapped for the node**:

- **node-readable** (default): the node can validate content, derive, aggregate, filter.
- **end-to-end**: the key is wrapped for members only; the node is a blind relay. Integrity
  then rests on app-side signatures, verified by every recipient (the node can't).

So "I want node-side logic on this data" and "I want this end-to-end" are mutually exclusive.

## The rule vocabulary

The bounded set that passes the litmus test — the rules that recurred across every app:

| Construct | Meaning |
|-----------|---------|
| `read <aud>` / `write <aud>` | the two enforcement points |
| `authored T` | T's records carry an immutable `author` stamped by the runtime; edit by author |
| `delete by <aud>` | who may delete (e.g. `delete by moderator` for an override) |
| `keyed by actor` / `self-keyed M` | a map whose key is the writer: one entry per person, write-only-your-own-key (reactions, votes, claps) |
| `F immutable` | a field locked after it is first set |
| `F one of [ … ] by <aud>` | a status field: fixed value set, fixed who-may-set it (a state machine) |
| `cap C { grants … revokes … }` + `permits { create if <pred>  issue <who> as C }` | a permit-gated instance and its genesis |
| `shard by <grain>` | platform-managed time sharding of a high-volume layer |
| `live` | ephemeral, no integrity (presence, typing) |
| `… when <field>` | a conditional, e.g. the publish gate: `read public when published` |

Two disciplines keep it small:

1. **A field appears only when it carries an enforced rule.** Pure data shape — line items,
   addresses, body formatting — never appears; the `.osv` is as small as the *rules*, not the
   schema.
2. **Rules attach to a container *kind*, not a location.** `reactions` is `self-keyed`
   wherever it appears — on a post or on a comment — so nesting in the *data* does not grow the
   `.osv`. Complexity is bounded by the number of rule-kinds (a handful), not by data depth.

`authored`, `self-keyed`, etc. compose as reusable patterns (`comments uses authored`).

> `edit by author`, "unreact by reactor", and "a buyer sees only their own order" are one
> primitive: **actor == the owner of this thing**, where the owner is read from a stamped
> *field* (`author`) or from the *key/path* (`reactions[did]`, `order/{buyer}`).

## Evolution — publish & reconcile

The manifest is itself a **published, versioned** document — an explicit publish gate, not
live-edited into force (a draft branch can be tested, then merged to a new version). Because
permits are self-contained:

- Publishing a new version changes the **template**, not the existing population. Existing
  permits keep meaning what they say, so peers on different versions never fork on enforcement.
- Bringing holders in line is a separate **reconcile**: the node bulk-issues/revokes (under its
  delegation) as ordinary row writes. Adding a layer to an existing role costs no re-issue —
  just key delivery; only principal-level changes (a new role, a promotion, a new cap) write
  permits.
- **Never bind a permit to a manifest CID** — that would invalidate every permit on every edit.
  Bind to the page identity; carry cap names.

## Worked examples

The same ~10 constructs cover real apps; the differences are which combination each picks.

### Slack — all three gating modes at once

```osv
workspace "acme" version "1" {
  role guest
  role member { }
  role admin  { grants member, guest }
  role owner  { grants admin, member, guest }

  page "main" {

    layer channel {                       // public channels (role-gated)
      namespace channel/{name}
      read role member   write role member
      shard by day
      authored   message
      delete by admin
      self-keyed reactions
    }

    layer private_channel {               // invite-only (permit-gated)
      namespace private_channel/{id}
      shard by day
      cap member
      cap admin { grants member, admin   revokes member, admin }
      permits { create if role member   issue creator as admin }
      authored   message
      self-keyed reactions
    }

    layer dm {                            // 1:1 / group DMs (path-gated)
      namespace dm/{participants}
      read path.participants   write path.participants
      authored   message
      self-keyed reactions
    }

    layer presence {                      // typing / online
      read role member   write role member
      self-keyed status
      live
    }
  }
}
```

Guests never match `role member`, so they only see the `private_channel`s an admin issued them
a cap into. node code: search index, push notifications. app code: thread UI (`reply_to`),
unread counts.

### Subreddit — minimal; the whole auth surface is one layer

```osv
workspace "r/rust" version "1" {
  role member
  role moderator { grants member }
  role owner     { grants moderator, member }

  page "main" {
    layer post {
      namespace post/{id}
      read public   write role member
      authored   post, comment
      delete by moderator                 // mods remove anyone's content
      self-keyed reactions
      self-keyed votes                     // one per user → feeds ranking
    }
  }
}
```

Feed = UI over the catalog by time; hot/top = node code over `votes`. Ban = a mod revokes
`member` + denylists. Comments are embedded and `authored`.

### Ecommerce — per-identity accumulation + state machine

```osv
workspace "shop" version "1" {
  role customer
  role staff { grants customer }
  role owner { grants staff, customer }

  page "main" {

    layer product {                       // catalog: public to browse, staff edit
      namespace product/{id}
      read public   write role staff
    }

    layer order {                         // one private doc per buyer
      namespace order/{buyer}
      read  path.buyer, role staff
      write path.buyer, role staff
      total  immutable                     // locked at checkout
      status one of [placed, paid, shipped, delivered, cancelled] by role staff
    }

    layer review {                        // public, one thread per product
      namespace review/{product}
      read public   write role customer
      authored   review
      self-keyed helpful
    }
  }
}
```

Buyer sees only their own order (`path.buyer`); staff see all; only staff move `status`.
node code: compute `total`, call the payment API on `placed`, email on `shipped`.
`total immutable` means a later price change never rewrites a placed order.

### Substack — the publish gate + free/paid tiers

```osv
workspace "my-newsletter" version "1" {
  role reader                             // free
  role subscriber                         // paid
  role writer
  role owner { grants writer, subscriber, reader }

  page "main" {
    layer post {
      namespace post/{id}
      write role writer                    // writers create
      read  author                         // draft: author only…
      read  public         when published and free   // …public once published
      read  role subscriber when published and paid  // …or subscribers, if paid
      authored  post
      claps     keyed by actor
      comments  uses authored
    }
  }
}
```

The publish gate is just a conditional read: author-only until `published` flips. node code:
the payment integration that grants the `subscriber` role on payment; email on publish.

## Trust model

The node is trusted (per `CONVENTIONS.md`): a compromised node is total compromise, and we do
not design to be tamper-evident against it. The threat is **malicious peers**. Defenses:

- permits are signed and **validated on apply by every peer**, not just the node;
- access is **default-deny** — nothing is permitted that isn't declared;
- provenance fields (`author`) are **runtime-stamped**, never client-supplied, so attribution
  can't be forged;
- **end-to-end** layers additionally withhold the key from the node, at the cost of any
  node-side processing on that data.

## Open questions

- Exact spellings (`namespace` vs `path`, `keyed by actor` vs `self-keyed`, the `when` clause).
- Grammar nesting (roles at workspace, layers in page) and multi-page workspaces.
- The **node-code interface**: how a derivation / validation / integration hook is declared,
  what it may read and write, and how it is triggered (on-op / schedule / webhook).
- How far the `when` vocabulary may go before a conditional becomes code.
- **Identity isolation** (a buyer never sees which staff member touched their order) — a node
  re-attribution feature, only if needed.
- Integrity default: app-signed everywhere, or an opt-in `signed` the node enforces generically.
- Permit / manifest **version causal-ordering**: ensuring an op is evaluated against the version
  it was authored under, and that a (re)issued permit precedes the ops that depend on it.
