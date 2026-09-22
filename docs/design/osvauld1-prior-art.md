# osvauld1 prior art: publish, sync, and how a user is stored

> **Status — 2026-09-22: a reading of `/home/abe/osvauld` (osvauld1) made while designing
> osvauld2's publish and sync slices.** It records what the old implementation actually does,
> with file references, and separates what we are taking from what we are deliberately
> leaving. Consistent with `workspace-permissions-sync.md` §1: the old repo is reference
> material for working patterns, not a compatibility requirement, and none of its wire or
> permit formats are prescribed by this note.

Read it before redesigning any of publish, sync, or the user record. Several questions we
were about to decide by argument were already decided there by experience, and at least one
is settled by a comment explaining a refactor they had already done once.

---

## 1. Publishing is headers-only, and they arrived there the hard way

`courier/src/peer_actor/publish.rs:1` states the flow:

1. Owner sends `PublishSpace`.
2. Node stores the space, replies `PublishSpaceAck`.
3. Owner sends `PageAnnounce` per page — **meta and permits only**.
4. Node stores a page *shell*, replies `PageAnnounceAck`.
5. Layer content arrives afterwards, over sync.

The decisive detail is the doc comment on `PageAnnounceMsg` (`courier/src/message.rs:188`):
*"Replaces monolithic PublishPage. Layers delivered via Scribe subscription."* They built the
version that carries content inline, and then removed it. We do not need to repeat that.

The message pair carries a token in each direction, which is the exchange we want:

```rust
PublishSpaceMsg    { request_id, space: PublishedSpace, space_permit }
PublishSpaceAckMsg { request_id, permit, pages: Vec<String> }
PageAnnounceMsg    { request_id, page: PublishedPageMeta, page_permit, owner_permit }
PageAnnounceAckMsg { request_id, page_id, permit }
```

The upward token is not a new root. `butler/src/services/publish_service.rs:30` mints it with
`gurkha::delegate_space(owner_signing_key, owner_permit, "node", node_public_key)` — the owner
**delegating their own permit down to the node**. In osvauld2 terms that is `token::delegate`
narrowing to `Scope::Workspace(id)` with the node as audience, which needs no new machinery
and keeps `sub == node_did` intact.

`PublishSpaceAckMsg.pages` returning the ids the node already holds makes re-publish cheap:
the owner announces the difference rather than everything.

## 2. The pull path, and what the index is on the wire

A viewer that wants a space it does not have:

```rust
SpaceRequestMsg { request_id, space_id, viewer_did, viewer_public_key,
                  viewer_encryption_key, viewer_permit }
SpaceDataMsg    { request_id, space_id, delegated_permit,
                  space: PublishedSpace, pages: Vec<PublishedPageMeta> }
SpaceDataAckMsg { request_id, space_id, delegated_permit }
```

**The index that crosses the wire is a flat list** — `PublishedSpace` plus a vector of
`PublishedPageMeta` (`courier/src/message.rs:541`). No CRDT is involved in transferring it.
`PublishedPageMeta` is documented as "page metadata for publishing (without encrypted_key -
that's local)", so the published header is a deliberate projection of the local record, not
the local record itself.

This matters for us: osvauld2's index-as-LoroDoc is still the right destination, but nothing
about publish or first sync requires it, and v1 shipped both paths without one.

## 3. Sync: three steps, state vectors, node authoritative on divergence

`courier/src/peer_actor/sync/protocol.rs:1`. The unit of sync is a **layer inside a page**,
not a space:

```
SyncOffer  { page_id, layer_name, layer_type, data, state_vector, authority_permit }
SyncAccept { page_id, layer_name, state_vector }
SyncAck    { page_id, layer_name, state_vector }
```

When the vectors still diverge after an exchange, the protocol retries a bounded number of
times — `MAX_RESYNC_ATTEMPTS = 3` (`courier/src/peer_actor/mod.rs:248`) — and then gives up on
merging: the peer sends `SyncReset`, the node replies `SyncSnapshot` with authoritative full
state, and the peer **replaces** its layer wholesale. The stated invariant is *"Node is source
of truth for divergence resolution."*

Two things we had not decided and now get for free: sync granularity is per layer, and
authority travels **on the sync message itself** (`SyncOfferMsg.authority_permit`), not only at
connection time. The second one matters — it means a long-lived connection does not become a
standing grant.

Layer kinds are named by convention (`domains/src/layer.rs:23`): `app:*` is code and UI,
`static:*` is binary assets, everything else is data.

## 4. How a user is stored

One table, keyed by DID (`domains/src/contact.rs:3`, `CONTACTS/{did} → ContactData`):

```rust
ContactData {
    did, encryption_key, username,
    devices: Vec<DeviceInfo>,          // { id, name }
    added_at,
    contact_type: ContactType,         // User | Node
    node_id: Option<String>,           // iroh NodeId, Node contacts only
    permit: Option<String>,            // how we authenticate to it, Node contacts only
}
```

Users and nodes share one record with a type discriminator and `Option` fields for the
node-only parts. `contact_service::upsert_node_contact` is explicitly "viewer → node
relationship... stores node info for future reconnection" — the same job as osvauld2's
`kunki::peer`.

**This cuts on a different axis than we do, and the difference is the useful part.** v1 splits
*identity* (the contacts table: keys, name, devices) from *authority* (permits, in
`butler/src/storage/store/permit.rs`). osvauld2 splits *issued* (`kunki::admin`, `users/`) from
*held* (`kunki::peer`, `nodes/`). Both splits are real. We have no identity record at all —
`users/<did>/meta` is a reserved empty slot — so v1's is the axis we are missing rather than
the one we got wrong.

Two gaps it exposes:

- **`devices: Vec<DeviceInfo>`** — one user, many devices, modelled from the start. osvauld2's
  claim binds a single `desktop_device_key`, so a second machine for the same person has no
  representation today.
- **`username`** — a human name existed from day one. Ours does not, and a DID is not something
  a person can recognise in a member list.

Note that `ContactType` is a participant-type field in *storage*, which does not conflict with
the federation decision in `workspace-permissions-sync.md` §4 — that constraint is on tokens
and permits, where the type can change after issue. A local record may say what it knows today.

## 5. Permits: take the flow, leave the format

v1 has **one** credential mechanism, not two. `gurkha::Permit` (`gurkha/src/parser/mod.rs:558`)
wraps the real `ucan` crate, and its facts carry a great deal:

```
peer_capabilities, layers, layer_patterns, issue_on: HashMap<String, DelegationTemplate>,
proof_chain (CIDs), sync_facts, presence, ephemeral_funcs, dynamic_layer_schemas
```

That is policy inside the credential, which osvauld2 explicitly rejected: tokens carry a role,
and the closed table in `courier::policy` decides what the role may do. We should not import
it back.

The one idea worth understanding before discarding is `issue_on` — "self-describing templates:
what to issue when holder takes actions", keyed by things like `page_request` and
`share_link`. It is how v1 drove the two-token exchange: the credential itself declares what
its holder's actions should mint. Elegant, and unnecessary for us, because our node already
knows what to issue — `policy` is a fixed table rather than something shipped per-grant.

**The relevant lesson for the current work is structural, not about UCAN at all:** v1 never had
a permit/token split. osvauld2 grew one by accident — `issue_permit`/`PermitClaim` for the
claim handshake, `token::Token` for everything the policy engine reads, with nothing bridging
them. Unifying on `Token` is a return to v1's shape, not a departure from it.

## 6. Rules run on the node, in Lua

`kunki/src/validation_service.rs:1` — Scribe sends a `ValidationRequest`, kunki runs
`validation.lua` on a `LuaRuntime`, and replies with the result. This is the same arrangement
`workspace-permissions-sync.md` §4 records for osvauld2 (rules are plain Lua, evaluated by the
node, because the node is trusted and a custom DSL buys nothing). It also confirms the node
needs the Lua runtime eventually, which is one half of the substrate-extraction item in
`node-backlog.md`.

---

## 7. What we take and what we leave

**Take:**

- Publish carries headers only; content follows over sync. Settled by their refactor.
- Two tokens at publish, the upward one a delegation of the publisher's own authority.
- The ack returning what the node already holds, so re-publish sends a difference.
- The wire index as a flat list of headers, independent of how it is stored locally.
- Three-step sync per layer with state vectors; bounded resync, then authoritative snapshot.
- Authority on the sync message, so a long connection is not a standing grant.
- An identity record separate from authority, with `devices` and a human name.

**Leave:**

- UCAN as the format, and policy carried inside credentials.
- `issue_on` delegation templates — our `policy` table replaces them.
- The actor hierarchy. Our handlers are pure message transitions so they can be tested
  without a transport; v1's are `ractor` actors holding connections.
- Per-page encryption keys. Different threat model: our driver is auditability, and the node
  sees plaintext by design.

## 8. Questions this raises for us

- **Multi-device.** The claim binds one device key. Adding a second device for the same person
  is not merely a storage change — `node_accept_claim` refuses a second claim outright, and
  "another device of an existing admin" is not currently expressible.
- **Where the identity record lives.** `users/<did>/meta` is reserved in `kunki::admin`. If it
  holds `username` and `devices`, it is the contacts table, and the desktop needs the same
  record for people it has never shared a node with.
- **Re-publish semantics.** `PublishSpaceAck.pages` implies the node answers "what do you
  already have" cheaply. Our `vault` item listing can do this, but nothing defines what
  happens when a header the node holds disagrees with the one being announced.
- **Layer granularity.** v1 syncs `(page_id, layer_name)`. osvauld2's `vault` item has `meta`,
  `src`, and `doc/<name>` keys, which is nearly the same decomposition under different names.
  Worth confirming they line up before the sync slice rather than during it.
