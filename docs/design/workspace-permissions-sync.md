# Workspace permissions, synchronization, and the sovereign node

> **Status — 2026-09-11: design baseline; validated resource-address syntax, callable handles,
> and exact/terminal-subtree scope matching are built. Authorization, indexes, sync, and the
> node are unbuilt. 2026-09-17: token, authorship, and rule decisions recorded in §4;
> 2026-09-19: signatures are record fields and kunki stores through vault, and the
> platform capability table plus its authorization boundary are built in `courier::policy`.
> 2026-09-21: federation recorded as direction — a workspace's home node stays its only
> authority, so the built token model carries over unchanged.**
> Records the direction agreed with the user, the lessons from the old implementation,
> and the decisions still required. Namespace examples are illustrative, not a grammar,
> wire format, or storage migration contract. Recommendations are explicitly labelled.

## 1. Scope and current truth

This is a fresh implementation in `osvauld2`. The old `osvauld` is reference material for
working patterns, app requirements, tests, and failures—not a compatibility requirement.
Neither its actor hierarchy nor its permit/wire formats are prescribed here. Permissions
and shared workspace data are the main design problem; do not repeatedly invent new
application workflows where the reference apps already demonstrate the requirement.

Today `osvauld2` has accounts, sealed storage, workspaces/items, Lua apps, Loro source and
named state documents, persistence, staged reload, and the automation bridge. It has no
live sovereign node, network sync, or peer authorization system. State docs currently
belong to app items and live in `app_host` cores opened by Lua. The cross-app workspace
model below is a change to implement, not an existing API.

Boundaries to preserve: `identity` is I/O-free, `storage` is raw bytes, `vault` is Loro-free
and seals opaque records. Lua never enters `runtime`; messages remain plain data. See
[architecture](../architecture.md), [vault](../vault.md), [identity](../identity.md), and
[conventions](../CONVENTIONS.md).

## 2. Workspace data and multiple apps

**Workspace data is accessible across apps under rules.** App source remains separate
from business data. Adding an interface must not require copying the underlying records.

A shop may have storefront, fulfilment, accounting, and administration apps over products,
orders, and derived summaries. A participant can have access to some apps and some data,
not necessarily everything in the workspace. App publication may target all admitted
participants, selected roles, or explicit DIDs. Internet-public access is a separate
policy choice; “all” must not ambiguously mean both.

Receiving an app's source or opening its interface does not grant data/action authority.
The product-creation UI is available to its intended audience, but the operation must also
be checked below the UI. A malicious peer need not run our app at all.

Open: how an installed program, a shared app instance, and its workspace bindings are
represented; whether an app additionally receives only a delegated subset of its user's
capabilities. Do not silently grant every installed app all of its user's authority.

## 3. Namespaces, DIDs, and shards

**2026-09-11 implementation note:** `workspace` now validates an ASCII address syntax rooted
at `ws/<workspace-id>/…` and machine-callable handles (`orders`, `shop-v2`). Empty,
traversal-like, wildcard, slash, backslash, percent, Unicode, and oversized input is rejected;
colon remains valid so a DID can occupy one segment. `ResourceBinding` is only an in-memory
handle/target pair—not an index resolver or authorization result. Exact scopes and terminal
`/*` subtree scopes now match validated segment boundaries; a subtree excludes its own base.
Opaque-ID shape, serialized CRDT indexes, resource kinds, display names, and vault integration
remain unbuilt. Stable identity is the intended use of a target, not yet a semantic guarantee.

**Capabilities scope actions to workspace namespaces**, including future matching data.
Do not enumerate every future order or daily shard in a role definition. Examples:

```text
ws/<ws>/products/<shard>
ws/<ws>/orders/<buyer_did>/<shard>
ws/<ws>/channels/<channel>/<period>
ws/<ws>/derived/fulfilment/<shard>
```

Scopes such as `ws_id/*/test/*` were part of the initial exploration. **Revised 2026-09-11:**
the first implementation deliberately permits only an exact address or one terminal `/*`;
interior and recursive wildcards are rejected. Broader patterns must be justified by concrete
app policy before changing this grammar. Physical path layout and opaque-ID kinds remain to
be specified. These addresses are not unchecked redb keys or filesystem paths.

For participant-owned data, a DID segment binds to the identity authenticated at connection
establishment. It is not a writer identity freely supplied with each update. A peer cannot
create another user's private resource merely by choosing its namespace. Creation also
requires the relevant capability; “under my DID” is not blanket write authority.

The DID identifies ownership, not the entire audience. Node/staff capabilities can authorize
access to an order under the buyer's DID. Relayed changes must also preserve whatever
original-writer or node-result evidence the enforcement model requires; authenticating the
relay alone does not establish authorship of every operation in a CRDT update.

Sharded documents are a target requirement, not an afterthought. A logical order must keep
a stable reference independent of physical shard placement. The mapping, rollover,
concurrent shard creation, and retention rules are open. A shard's decryptable contents
must respect its audience: filtering nested records in the UI is not access control.

Permission to read a namespace does not require loading every shard. Index discovery,
selected date ranges, active subscriptions, and local cache/eviction policy are separate
from entitlement. Local-only documents never enter network discovery or transfer.

A durable local-only draft (such as an unsubmitted order) is document truth: it may use
Loro and must persist across restart. It is not transient viewer scratch such as an
unfinished input in `ui.state`; that scratch stays outside CRDTs and need not survive
restart. The proposed local-only designation excludes the durable document from every
network index, advertisement, and transfer; that network enforcement is not built yet.

## 4. Policy, capabilities, and the grant bundle

| concept | responsibility |
|---|---|
| Manifest/policy | Resource rules, conditions, permitted actions, and delegation limits |
| Role | Administrative grouping of participants and capability bundles |
| Permit | Signed concrete capabilities scoped to a holder and resources/actions |
| Grant bundle | Permit plus content-key material encrypted for the recipient |
| Node code | Computation and effects constrained by the declared policy |
| Sync | Delivery of permitted data, discovery, and permission updates |

**Roles organize grants; permits carry capabilities.** An app declaring that it needs a
resource requests authority; it does not grant itself authority. Distinguish discovery,
read, write/create, invoking an action, and granting/revoking access. Subscription and
replication progress are not capabilities.

**Access permits and decryption material travel in one bundle.** Use the recipient's
X25519 public encryption key to wrap the symmetric content key material; their DID embeds
the separate Ed25519 signing key. Never encrypt to a DID as though it were an encryption
key. The signed bundle must bind its intended recipient, scope, rights, and relevant key
identity/generation so fields cannot be substituted independently.

The node receives the authority and key material needed to issue those bundles. This is
not a blind-issuer assumption. Whether keys are per namespace, shard, or another bounded
unit—and how they rotate—is still open. Avoid granting an entire namespace's decryption
key where the intended audience covers only part of its contents.

Signing, canonical encoding, domain separation, verification, attenuation, and bounded
proof-chain processing need a fresh implementation using the secure-core contracts.
Parsing a token or seeing a newer version is not verification. Key handling must retain
zeroization and sealed-at-rest guarantees.

### Publishing delegates bounded issuance

The owner/publisher may delegate to the node: issue specified capabilities on these
namespaces to authenticated members of these roles, explicit DIDs, or resource-bound
identities such as the buyer. Every issued grant must trace to valid authority and stay
within its scope and actions. Authority to issue grants to staff does not automatically
include authority to appoint staff. Role-membership facts need authenticated provenance.

Publishing includes metadata, source/data availability, policy, delegation, and required
keys. Metadata accepted is not the same as content durably available. Define those states
before exposing success or advertising a resource as ready to serve.

### Permit evolution travels through sync

Adding roles/apps/layers is normal. Policy or membership changes cause authorized issuance
of upgraded/replacement bundles, delivered through synchronization. A new shard within an
existing namespace scope needs appropriate key delivery, not necessarily new privileges.
An app using already-authorized data does not inherently require new data capabilities.

Replacement must identify what it supersedes. Do not union old and new privileges forever
when a change removes access. Grant updates and dependent data need defined ordering; the
receiver must possess valid authority/key material before accepting dependent content.
A sender checks current authorization before disclosure. Exact grant IDs, generations,
rollback prevention, expiry, revocation ordering, and historical-write treatment are open.

### Decided 2026-09-17: role tokens, signed updates, Lua rules

Agreed with the user; supersedes the "Permit" row above (tokens carry roles, not concrete
capabilities) and the open manifest/Lua split in §7. Built: `courier::token` — issuing,
delegation, and the chain check below, over the four-level scope, with
`workspace::ResourceScope::contains` for the innermost level. Everything else here is not.

**The node is the root authority; tokens carry roles.** Fields: `iss`, `aud`, `sub` (node
DID), `role`, `scope`, `delegable`, `nonce`, `iat`, `exp`, `prf`. `prf` embeds the full parent
token inside the signed payload; the root has none and `iss == sub`. Semantics follow UCAN 1.0
delegation, encoding is courier's own: only our node verifies, so DAG-CBOR/varsig and
per-action invocation tokens buy nothing (and `rs-ucan` is marked work-in-progress/unaudited).

Chain check on the node, leaf to root: depth cap before any crypto; every link's signature,
`sub == node`, unexpired, unrevoked; `child.iss == parent.aud`; parent `delegable`; a child
never widens role or scope; root `iss == sub`; leaf `aud` equals the session's proven DID.

- **Node admin** = create workspaces and appoint admins (delegate the admin role). Publishing
  an app is write on a workspace, not admin. The bootstrap claim creates the first admin and
  is refused once one exists — kunki prints a fresh ticket on every start.
- **Reissue keeps chains short**, but a flattened node-signed token no longer contains the
  intermediate issuer: the node must record lineage or revoking that issuer stops cascading.
- **Keys stay out of tokens.** Device/encryption keys live in the relationship permit;
  wrapped content keys travel beside the token (`prf` would copy them into every child).
- **Role meaning lives in the manifest**, so a policy change reinterprets existing role
  tokens by design. That is acceptable only because a policy change is an explicit signed
  publish, never a side effect of a code edit (§5).

**Authorship is a signed update.** The desktop sends `{author DID, update bytes, sig}`. The
node verifies the signature and that every Loro peer id in the update is bound to that DID
(clients choose peer ids; without the binding one peer can write as another). Signed updates
are kept in the node log, so authorship stays verifiable later. `record.author` is the signer
of the creating update; clients cannot set it.

**Revised 2026-09-19 (§4):** the signature moved into the record as a field, so there is no
node-side update log and no peer-id binding requirement. The paragraph above is kept for the
reasoning that led there.

**Rules are Lua in the signed manifest; there is no custom rule vocabulary.** The node is
trusted, so a DSL adds a language without adding security. Kunki runs the rules on merge. The
manifest also declares data shapes — the node needs them to map changed containers to
records and fields. A manifest is accepted only if the publisher's chain reaches the node and
allows publishing there; node-run functions act with the publisher's authority, not the node's.

Rust proves, Lua decides. A rule receives only verified facts and returns `true` or
`false, reason`:

| fact | contents |
|---|---|
| `update` | `id`, `author` (verified), `signed_at` (the author's claim, not trusted) |
| `change` | this record's slice: `record`, `kind` (create/edit/delete/move), `fields` old/new |
| `roles` | from the verified token chain, **as of merge**, not signing |
| `record` | pre-state: `author`, `created_at` (node clock), `parent`, own fields |
| `now` | node clock |
| `workspace` | `id`, `get` (reads are tracked), `members` only if the manifest requests it and it is granted |

No tokens, keys, network, writes, or other workspaces. One update is accepted or rejected
whole; the rule runs once per touched record. Desktops may run the same rules before sending
as a courtesy; the node is the authority. Read is per doc, so a data-dependent read rule must
re-run when a field it read changes — record reads during evaluation. The Luau sandbox and
interrupt budget move into a headless crate shared by `app_host` and kunki (`app_host`
depends on `runtime`).

Checklist the Lua rule API must support, from shop, chat, subreddit, and expense-approval
walkthroughs: nearest ancestor by type (`post.locked`), directory relations
(`author.manager`), separation of duties, thresholds, status transitions, lock after a
state, per-field privacy (the node splits private fields into their own doc), soft delete as
the default. Uniqueness and bounds rely on the node merging one update at a time and need a
client-side pending state. Computation such as auto-moderation is a node function.

Open: who may revoke a link (node only, or also its issuer); the creator's initial authority
on a new workspace; client rollback/pending UX for rejected updates. Surfaced while building
the chain check: a delegation cannot change its role, so a maintainer cannot hand out an app
role by delegating — role assignment has to be node issuance under `role.assign`, which also
keeps it in the audit log. Delegation then only ever narrows scope.

### Decided 2026-09-18: roles, capabilities, rules, and how apps reach data

Agreed with the user; refines the block above. **Built 2026-09-19:** `courier::policy` — the
closed platform capability set, the `(scope level, role) -> capabilities` table, and
`authorize` = chain check ∧ the role's scope covers the target ∧ the role carries the
capability. Targets are syntax-checked first, because `Scope::Node` contains every variant and
would otherwise reach addresses a narrower role could not parse. Manifest capabilities, app
roles, and rules are still unbuilt.

**Role, capability, and rule are three things with one owner each.** A *role* is a name a DID
holds in a scope, carried by the token chain and checked in Rust. A *capability* is a named
action (`app.install`, `order.refund`) that policy grants to a role; capabilities never appear
in a token. A *rule* is manifest Lua deciding one capability against one record. The node's
check is always `actor holds a role covering the target` ∧ `capability ∈ capabilities(role)`
∧ `rule(ctx)`.

Capabilities are two closed sets: platform ones fixed in Rust (`app.install`, `app.remove`,
`namespace.declare`, `policy.publish`, `member.invite`, `role.assign`, `workspace.delete`) and
app ones declared by a manifest, alongside `read`/`create`/`edit`/`delete`. A rule cannot
invent a capability; a manifest cannot declare a platform one.

Roles compose by union, so a role can never deny — a ban is revocation or an explicit deny
list the node checks first, not a negative role. Roles are also the unit of delegation: a
narrower grant means the manifest declares a narrower role. Identity facts (`author`,
participants, group members) reach rules as facts and never become roles; otherwise every DM
would mint tokens.

**Scope names the level a role is read against**, replacing the single address scope:
`Node` (platform roles: owner, admin) | `Workspace(id)` (platform roles: owner, maintainer,
member, guest) | `App { ws, app }` (roles from that app's manifest) | `Resource(ResourceScope)`
for a deliberate one-off share.

**Three token layers:** connection (the built relationship/admin permits — a session, not a
data grant), workspace (the index and structure roles), app (manifest roles). Tokens exist
where a human delegates. Per-document access is derived by the node from role plus rules and
is never minted per document.

**Structure writes are not data writes.** Structure — apps, namespaces, schema/rules, role
assignment — is maintainer/owner work under the workspace token. Data is records inside an
existing namespace, open to whoever the rules allow under an app token. Creating a namespace
is structure; creating records inside one is data. This supersedes the earlier phrasing
"publishing an app is a write on a workspace": it is a write to workspace *structure*.

**Namespaces belong to the workspace, not to apps.** A namespace owns its schema and its
rules; apps are lenses that request namespaces, so two apps writing the same namespace obey
the same rules and nothing is copied. Namespaces are declared as **patterns**
(`chat/dm/*`, `orders/<caller>`) so instances are created as data at runtime without a
structural change — otherwise every DM would need a maintainer.

**Install is the authorization event.** Publishing declares the roles an app defines and the
namespaces it needs; a maintainer approves that list for this workspace and maps workspace
roles to app roles. A manifest alone grants nothing, and an unresolved namespace request fails
the install rather than launching a broken app. At runtime access is `namespaces bound at
install` ∩ `what this user's roles allow`: an app can exceed neither its user nor its binding.

**Apps are all-or-nothing (policy).** If a user's roles do not cover everything an app needs,
the app is absent from their index; different audiences get different apps, as osvauld1 shipped
`shop-owner` and `shop-customer` separately. Namespace access is per app, record rules are per
role. The one surviving partial pattern is the viewer-scoped request `orders/<caller>`.

**Apps hold no key.** A write is "user U via app A", and only U's signature is real. App
identity is provenance, not authority; enforcing it would mean per-app keys, not worth it
while every app is internal.

**One index doc per (user, workspace)**, node-written and user-read (§6's recipient-specific
case). New resources are appended to the indexes of everyone allowed to see them; the user
then pulls content, and push is an optimization for open docs. The index diff is the
offline catch-up list, so no separate catch-up protocol is needed.

Scenarios this must satisfy, beyond §8's: storefront, my-orders, order desk, catalog editor,
a dashboard reading node-derived `summaries`, announcements; chat as DM, group, and
announcement. They stress patterns for instances, membership-driven reads, a separate guest
channel, and one person holding two roles at once.

Open: who may declare a namespace (maintainer, or the publisher of an app that needs it);
whether an update records the writing app; how a deny list is represented; re-running read
rules and updating indexes when group membership changes.

### Decided 2026-09-19: signatures are record fields; kunki stores through vault

Agreed with the user. The signature half supersedes "Authorship is a signed update" above;
the storage half is new. Nothing here is built.

**A signature is a field on the record, not a wrapper around the update.** A message is
`{id, author, text, ts, sig}` and the node verifies the field at merge. Provenance then
replicates with the data, so any peer can check it instead of trusting the node's word, and
no node-side log has to hold a second copy of every update. It also survives re-encoding: the
signature covers the record's fields, not an update's bytes, which cannot be re-derived
byte-identically from a merged doc.

Two constraints come with it. **Signed fields are write-once** — a `LoroText` that merges
concurrent edits changes the value out from under its signature, so an edit is a new version
carrying a new signature, and collaboratively edited prose cannot be signed this way at all:
nobody authored the merged text. **The signature covers the record's address**, not only its
content, or a peer could move a signed message into another thread and have it still verify.
Structural ops — delete, move, reparent — have no signed body of their own and stay
rule-enforced at merge.

This drops a requirement: Loro peer ids no longer need binding to DIDs for authorship, because
the field answers it directly. `(PeerID, Counter)` stays what it always was — ordering, not
identity.

**Kunki stores through `vault`, the same way shell2 does**, as vault's header already intends
("the future node will too"). The node identity moves into the account db at
`identity/keystore`, so kunki's `identity.bin`, `load_or_create_identity`, and `write_secret`
give way to signup-then-login with the boot passphrase. Sealing at rest means something
weaker here than on a desktop — the node can read everything it holds, which §5's trusted-host
assumption already grants — so it is disk-theft protection, not access control. Workspaces,
apps, and documents use `create_workspace`/`create_item`/`put_src`/`put_doc` unchanged; the
node's own records (tokens, lineage, revocations, per-user indexes) go on `Vault::store`
rather than a second database.

Open: the canonical encoding a record signature covers (field order, and the form of the
address inside it); whether a signed record is ever amended in place rather than versioned.

### Decided 2026-09-21: nodes federate; a user talks only to their own node

Agreed with the user, as direction rather than near-term work. The end state is that a
published node is usually some user's own node, nodes stay synced with each other, and a user
connects only to their own node to receive everything — never directly to a peer's. Nothing
here is built, and Gate 1 is unaffected; this section exists so the parts being built now do
not have to be untangled later.

**Authority does not change; the topology does.** Every workspace has a *home node* — the one
hosting it, and the only one that decides what is accepted. Bob commenting in a workspace
Alice hosts needs the record signed by Bob's DID and a membership token for that workspace,
and that token was issued by **Alice's** node when she invited him, not by Bob's. So
`sub == node_did` in the chain check stays exactly right: Bob's node never holds authority
over Alice's workspace, it is Bob's relay and cache. The federated case reuses the built model
rather than weakening it.

**A node acts for its user by delegation.** Bob's node fetches and pushes on his behalf by
holding a token Bob delegated to the node's DID — `sub` still Alice's node, `aud` Bob's node,
gated by `delegable`, narrowed in scope and short-lived. This is the case `token::delegate`
was for, so no new mechanism is needed. The cost is stated plainly: compromise of Bob's node
key is compromise of whatever Bob delegated to it, and scope and expiry are the only limits.

**Rules run exactly once, on the home node.** A relaying node never adjudicates — it would be
deciding with a manifest it did not validate. It caches what the home node accepted. That
keeps "Rust proves, Lua decides" a single-authority statement, which is what makes it
analyzable at all. Corollary for the transport gate: build one protocol both ends speak, not
a server half and a client half, because a node is a client of other nodes.

Three concrete consequences for storage and protocol:

- **Addressing becomes node-qualified below the token.** Tokens are already unambiguous —
  `sub` names the node — but a store caching two other nodes' workspaces needs
  `nodes/<node-did>/ws/<id>/…` or their ids collide.
- **A revoked set belongs to one issuing authority, never to a node globally.** What this node
  revoked and what another node told it are different sets; merged, one node's revocation could
  shadow another's ids. Hence the reserved split in `kunki::admin` below.
- **Replication adds a trust edge and removes any pretence of erasure.** "The node is trusted"
  (§5) is a statement about *your* node; a peer's node caching your workspace is mild — the peer
  can read it anyway — but eviction stops future delivery and nothing more, now structurally
  rather than only for desktops.

**Storage layout, built 2026-09-21 in `kunki::admin`.** Top level is this node's own authority:
`token/<id>` is the issue record, `users/<did>/tokens/<id>` indexes it by holder and leaves
room for `users/<did>/meta` when profiles exist, and `revoked/<id>` is what this node revoked.
`nodes/<node-did>/` is reserved for the mirror image — tokens this node holds from another, and
revocations that node announced — which is also exactly what a desktop needs, since a user
holds tokens from several nodes. That the two halves are the same shape is the argument for
moving this store out of `kunki` into a crate both it and `shell2` use, once there is a second
caller.

**A profile is not authority.** Tokens flow node → user; profile data flows user → node. They
share a namespace, not a record, and only the second is a candidate for CRDT sync — a grant or
a revocation that a peer could merge away is neither. A user-authored display name is a label
the node stores and never reads as identity; the DID is the identity.

Open: whether a relaying node needs its own membership token from the home node in addition to
the delegated user token; how a node learns another node's transport address (`did:key` is
self-authenticating for keys, so this is an address book, not a resolver); whether a home node
can migrate.

## 5. Trust, consent, and updates

Connection tickets bootstrap the initial relationship/install-or-join flow. They must bind
the expected node and admission authority, not mean unrestricted future workspace access.
Ticket redemption, replay rules, initial grants, and consent recording are to be specified.

Distinguish permission (allowed), consent (agreed), possession (already held), and authority
(which changes count in the shared application). Installing/joining can establish scoped
consent; it is not consent to every future recipient or policy expansion. Receiving a grant
does not force downloading everything. Local drafts do not become uploads merely because
an app was installed.

**Participants retain their copies by design.** Revocation stops future authorized delivery,
operations, and key access; it cannot retract plaintext already received. A person can
alter their private copy, but that does not make it an authorized shop confirmation.

The host/node is trusted; the threat is malicious peers. Normal runtime enforcement must
keep node code within policy, but cryptography cannot constrain the later use of plaintext
by its trusted recipient. Do not promise purpose enforcement against a compromised node.

A signed manifest establishes provenance/integrity, not participant consent. App authorship,
workspace adoption of a policy, and grant issuance are distinct authorities. Recommendation:
code-only updates can stay easy within established authority; policy expansion/adoption and
broader disclosure require an explicit flow. Exact adoption, versioning, consent, and code
activation rules are unresolved. Never silently reinterpret old permits on a source edit.

## 6. CRDT indexes and selective synchronization

**Indexes are Loro documents.** Indexes of indexes are valid. Each has its own version
vector and incremental sync, just like content. A private node-user index can bootstrap
accessible workspace indexes, published apps, collections, and documents/shards. The exact
hierarchy and writer rules are open; no need to freeze one nesting scheme before the access
model is clear.

Indexes may be shared when their audiences are identical, otherwise recipient-specific or
separately protected. Index entries and their history must not disclose private resources
to unauthorized users. A reference is not a grant: include/reference the verified bundle
required to follow it. A read-filtered UI over a globally shared index is not privacy.

An entry announces what exists/is available; it does not assert that every device downloaded
it. Do not copy `address -> synced bool` as progress truth. Replica progress is local,
per-device knowledge expressed using actual document VVs. Child VVs in indexes are possible
hints, not settled: updating every ancestor on every edit has a cost.

Persist available content/authority before advertising readiness, or explicitly represent
pending availability. Offline recovery must reconcile durable catalogs/grants, not depend
on a notification to a currently connected subscriber. Joining late must discover both new
apps/items and shards inside known collections.

Reconciliation requirements, not a chosen wire protocol:

- Preserve concurrent history; recovery snapshots normally merge, not replace local state.
- Use each replica's actual VV, not an optimistic “sent” watermark. Reconcile both directions.
- Correlate receipts and define them against durable persistence; retain dirty state on failure.
- Handle pending causal imports, reconnects, duplicate delivery, bounded queues and payloads.
- Account-wide discovery must not let one device's progress suppress another device's sync.
- Eviction/unsubscription does not revoke authority or delete other participants' copies.

Tombstones, retained index history, grant/key epochs, and garbage collection require explicit
contracts. Live CRDT convergence alone does not decide these security/lifecycle questions.

## 7. App workflows: requests and responses are documents

A separate RPC-style business-data protocol is not needed for the order/form pattern:

```text
local-only draft
    -> explicit submission in the user's authorized namespace
    -> discovery/sync to node
    -> manifest-authorized processing
    -> result/status written to the same logical record
    -> sync to the user and other authorized recipients
```

The request has a stable identity. Repeated delivery is not a new submission. Submission
readiness, allowed subsequent input edits, processing revisions, durable completion, and
restart/retry behavior need definitions. Idempotent Loro import alone does not make a node
handler or external payment effect execute exactly once.

Node code and peers follow manifest policy. `confirm`, `cancel`, and `available` are
app-defined concepts, not built-in platform meanings. Their meaning comes from conditions,
authorized effects, and subsequent rules. Candidate manifest rules include actor ownership,
immutable fields, and `pending -> confirmed`; calculations, integrations, and derivations
may use Lua, but must not bypass the protected write/action boundary.

The exact manifest/Lua split is **open**. Do not prematurely reduce the rule vocabulary to
role/path checks or adopt a general-purpose inference engine as authorization.
**Revised 2026-09-17 (§4):** rules are Lua in the signed manifest, run by the node over
verified facts; no custom rule vocabulary. Rules must specify their authenticated inputs and
enforcement point. A booking acceptance needs a
serialized availability check and commit at an authority; CRDT convergence cannot make two
concurrent reservations both exclusive. Readable policy should explain allow/deny decisions.

### One authoritative record; references versus projections

Customer and staff indexes should reference the same logical order where staff may read it.
Do not maintain independently authoritative customer-order and staff-order statuses. Status
changes target the canonical record; authorized recipients hold replicas of its history.

A compact summary containing status is a derived projection and can lag. Options are:
references plus fetched records, a revision-labelled summary, or an authorized on-demand
query. If staff must not receive full details, a projection is necessary. A node transaction
across order/summary storage is not automatically atomic visibility across replicated docs.
Embedding related fields can give one-document updates, but embedding differently private
orders into a single decrypted shard defeats isolation. Temporary offline lag is unavoidable;
competing authoritative copies are not. Atomicity/projection guarantees remain to be chosen.

## 8. Reference apps and implementation lessons

Paths below are relative to the reference checkout `/home/abe/osvauld`, not target modules.
Research inspected the current working tree and test definitions; no reference suites were
run. Some newer policy experiments are uncommitted. Comments are not proof of enforcement.

| reference | lesson / actual shape |
|---|---|
| `sample_apps/my-shop/page.lua` | Declares product access, per-customer orders, node-derived summary, and unsynced drafts |
| `sample_apps/my-shop/shop-customer/app.lua` | Writes local drafts; submit pushes a pending order into `orders/<my_did>` and deletes the draft |
| `sample_apps/my-shop/shop-owner/app.lua` | Reads summary, opens the customer's orders list, changes the original order status |
| `sample_apps/my-shop/shared/init.lua` | Derives restricted order summaries from customer layers |
| `sample_apps/my-shop/shared/validation.lua` | Attempts ownership, state-transition, and field checks in Lua; not proof they are correctly wired/enforced |
| `sample_apps/my-booking/shared/init.lua` | Derives availability without customer details from private bookings and blocked times |
| `sample_apps/osvauld-demos/group-chat/app.osv` | Channels, explicit-grant DMs, and daily history shards |
| `courier/src/peer_actor/publish.rs` | Metadata/permit announcement before layer content sync; acceptance is not readiness |
| `scribe/src/sync/sync_meta.rs`, `courier/src/peer_actor/subscribe.rs` | Creator index -> node pull -> authorized recipient indexes -> recipient pull |
| `kunki/src/node_runtime.rs`, `butler/src/scribe_manager.rs` | Headless processing/lifecycle and bounded document retention lessons |

The old shop uses a **per-customer list containing multiple orders**, not one Loro document
per order. Staff directly update that list; the node derives its summary. Local-only handling
also exists in Scribe broadcast/subscription filtering and remote-apply rejection. Reuse
these requirements, not an invented replacement workflow.

The old discovery index is **page x user scoped**. It discovers new layers inside a known
page, but does not by itself bootstrap a newly published page for an existing viewer.
Immediate fanout enumerates current subscribers; late-join recovery reconstructs discovery
from stored creator catalogs/authority. The new design needs an explicit durable lifecycle.

Do not reproduce the reviewed hazards:

- `courier/.../sync/protocol.rs` and `scribe/src/sync/mod.rs`: fallback replaces history in
  both modes; offer/receipt correlation and concurrent-vector knowledge are insufficient.
- `scribe/src/loro_observer.rs`, `sync/broadcast.rs`, `layer_unit/mod.rs`: progress advances
  on enqueue and dirty state clears before successful save.
- `gurkha/src/policy/parser.rs`, `authz/token.rs`, `courier/.../handshake.rs`: token parsing
  does not establish verified signatures/chains; handshake signatures are left empty.
- `kunki/src/validation_service.rs`: the existence of a validation service is not evidence
  that every receive, handler, and write path enforces it.

Existing reference tests include `integration_tests/src/tests/{publish,app_sync,layer_sync}.rs`
and `e2e_tests/test_chat_daily_shards_offline_3peers.py`. Turn useful scenarios into new tests;
do not inherit their success claims or coverage assumptions.

### Prajana / agent_x

Terra inspected `/home/abe/agent_x`: current code includes `vyakarana/lib/kriya_*.ml` and
`brahman/yantra/**/*.tantra4`. It is a reasoning/computation engine, not a verified permission
engine. Useful lessons: explicit facts/dependencies, composed rules, and explanations.
Do not adopt its missing-value coercions, incomplete logical negation, bounded heuristic
fixpoints, or unverified facts as authorization semantics. Exact policy expressibility
must be worked through with shop, booking, chat, and collaborative-board cases.

Earlier [OSV notes](../archive/osv.md) and [merge-referee](merge-referee.md) remain historical
input, not silently adopted contracts. Their page/layer vocabulary and proposed validation
semantics need reconciliation with workspace namespaces and the request-document lifecycle.

## 9. Ownership, shell2, and Kunki

**Recommendation, not a selected framework:** share a headless data/permission/sync service
between the desktop and node. Use serialized ownership and plain commands first. Actors
may implement that ownership, but neither `ractor` nor actor-per-page/shard is required.
`shell2` already serializes UI work through its event loop; do not build another UI actor tree.

One authoritative in-memory instance per loaded address prevents competing open-tab,
background-sync, and closed-document persistence paths. Keep inactive shards unloaded and
retain bounded state. Network/external work must not block the owner; returned results
need state/version revalidation. Actor queues alone provide neither durability nor
cross-document atomicity.

A workspace command loop is a candidate serialization boundary. Its threading, Lua adapter,
cache, and transaction interfaces remain open. Moving documents off the UI thread must
preserve synchronous Lua write behavior, next-frame mirrors, wakeups, and staged reload;
do not silently turn every field access into RPC. Kunki must run without a window/GPU and
must persist/stop cleanly independently of UI tab lifetime.

## 10. Work to do

These are work packages, **not permission to implement them wholesale**. Break each into
reviewed ~100-line code slices with tests, following the repository process.

1. **Permission contract through real apps.** Specify shop submission/fulfilment, booking
   acceptance, private chat, and shared board operations. Pin resource bindings, allowed
   actions, facts, transition checks, Lua limits, role authority, and consent. Resolve the
   canonical-order versus restricted-projection case before promising atomicity.
2. **Namespace and grant format.** Challenge the built exact/terminal-subtree syntax against
   real app policy, then define DID bindings, creation authority, logical-record/shard
   addressing, canonical signed capabilities, delegation, bundled keys,
   recipient-key authentication, role evidence, upgrades/revocation, and version ordering.
   Include policy adoption and old-write behavior—not just happy-path issuance.
3. **Identity/device decision.** The current mnemonic derives the same transport device key
   on every recovery. Define distinct installation/replica identity and any versioned
   migration before claiming multi-device correctness. Do not alter frozen v1 derivation.
4. **Headless document/persistence ownership.** Extract the necessary Loro lifecycle above
   vault; support cross-app handles, named/shared resources, local-only data, closed items,
   bounded shard loading, failed-save retry, and safe account lock/shutdown.
5. **Ticket, publishing, and indexes.** Implement verified bootstrap, bounded node delegation,
   grant/key delivery, publication readiness, recipient-safe CRDT discovery, and offline
   index reconciliation. Choose index hierarchy/writers from the permission contract.
6. **Synchronization implementation.** New protocol and transport integration over the
   shared service: authenticated sessions, authorized delivery/import, correlated durable
   receipts, concurrent histories, causal gaps, backpressure, and selected-shard fetch.
   Start with deterministic in-memory transport tests; old wire compatibility is not required.
7. **Node processing and app integration.** Define durable submit/process/result state,
   retries, policy-governed Lua execution, authorized projections, source updates, and
   two interfaces operating on the same workspace data. Wire shell wake/reload and headless
   Kunki lifecycle without making tab-open state a prerequisite for sync.

### Acceptance scenarios

- Local draft survives restart and never appears in any peer index or transfer.
- Buyer submits offline; node later processes it; repeat delivery/restart does not create
  another logical order. Failure before/after each persistence boundary is exercised.
- Staff and buyer observe the same order history through different interfaces; concurrent
  cancel/confirm produces the policy-defined outcome, not two accepted terminal states.
- Private buyer data never appears in another buyer's index, shard payload, key bundle,
  summary, or error disclosure. An unauthorized namespace/DID cannot be claimed.
- New app, role, resource, and daily shard become available through explicit authorized
  publication/upgrades; an offline and a fresh second device both catch up.
- Permission removal stops future delivery/actions and rotates required keys without
  claiming erasure of existing copies. Stale/replayed upgrades do not restore privileges.
- Open, closed, and evicted data sync correctly; new source stages reload without losing
  shared state. Two apps use the same workspace resource without competing snapshots.
- Concurrent edits, lost receipts, reordered/duplicate messages, pending imports, queue
  overflow, storage failure, reconnect, and process restart preserve acknowledged history.

**Next design checkpoint:** a complete namespace/capability/processing table for the shop,
then challenge it with booking and chat before committing to rule syntax or crate APIs.
