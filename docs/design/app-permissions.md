# App permissions — tokens, membership docs, manifest, rules

Status: **design, agreed 2026-10-04. Built 2026-10-04: the §3 parser (`manifest` crate), §2 role assignment with cascade, and the §3 read/write rules on the node (roles, DID variables, `members(doc)`). `validate` (§5), the index (§4) and channels are not built yet.** The first slice is built by
[`group-chat-sync.md`](group-chat-sync.md) (chat needs §3–§6). This doc is the general model;
that plan is the first app proven against it. Extends — does not replace —
[`workspace-permissions-sync.md`](workspace-permissions-sync.md) §4 (decided 2026-09-17/18),
whose role → capability → rule split it keeps.

Derived from a pass over twelve app shapes (spreadsheet, wiki, kanban, group chat, forum,
announcements/Q&A, shop, booking, approvals, forms, CRM, calendar, file library, whiteboard,
multiplayer game, dashboards) and osvauld1's permit code. What each app needs is in §9; the
rest of this doc is what they share.

## 1. The rule everything follows from

**The node hides data per doc; it checks writes per record.**

- Read is decided for a whole doc. Anything some readers must not see lives in a different
  doc — a DM, a private group, a guest's slice of a folder, free/busy without titles.
  Filtering fields in the UI is not access control.
- Write is decided per change. The node imports an update into a scratch fork, turns it into
  record-level changes, and Lua rules accept or reject them (§5).

Consequence for layout: the doc boundary is the privacy boundary. When an audience needs part
of a doc, the node derives a separate doc for it (§6 `derive`).

## 2. Two places authority lives

| in a **token** (node-issued, few, long-lived) | in a **doc** (facts rules read) |
|---|---|
| workspace roles: owner, maintainer, member, guest | members of a group, DM, channel, board, calendar |
| app roles: chat `admin`/`moderator`, shop `staff` | moderators of community X, and their rank |
| one-off share: guest on one board/folder (`Scope::Resource`) | bans and mutes (deny list with expiry) |
| | "manager of X", provider, assignee, form owner |

**Tokens stay coarse.** Anything an app creates many of at runtime (an *instance*: a group,
a DM, a board) records its membership as data, not as a minted token — otherwise every DM
mints tokens and every member change is an issue/revoke (osvauld1's shape, and its failure).

**Roles are the union of the node's live records for the caller**, not the token the request
happens to present: `kunki::admin::issued_to(caller)` minus revoked, at scopes covering the
target. Revoking is immediate; several grants add up. Roles never deny — denial is data (bans).

**Issuing.**

- Claim and invite exist (`courier::invite`). Invites today are always `member` at
  `Scope::Node`; they gain workspace/app scope and an app role.
- `role.assign` (new): an assigner may give a role only inside its *grant cone* — the
  manifest's `role admin { grant member, moderator }` — ported from osvauld1's
  `ManifestAuthorizer` reachability. The node mints a flat token and records
  `Cause::Under(assigner)`, so revoking the assigner's grant cascades.
- Policy is never inside a token. Changing the manifest re-reads roles without re-issuing.

**Why a membership doc can be trusted.** It is synced; members hold copies to show the
member list. Authority comes from the node, not the doc:

1. Rules read **the node's own stored copy**; a desktop's copy never decides anything.
2. Every change to it is a write like any other, checked against the node's copy *before*
   merge by the doc's own strict rule, e.g. only an admin listed in it may change `members`.
3. The chain is short: node token says "member of chat" → manifest says members may create
   groups → the first write to `group/X/meta` must set `owner = caller` on a doc that did not
   exist → only admins listed there change it afterwards.
4. The node merges one update at a time, so "remove Bob" and "Bob posts" cannot race;
   whichever arrives second is judged against the first.
5. Its history is the audit log of membership.

Hidden membership: the meta doc becomes node-only and members read a projection (§6).

## 3. Manifest (`manifest.osv`)

Declarations only; the node parses them before running any app code.

```
app "chat" {
  roles admin, moderator, member            -- app roles; owner is the implicit root
  role admin { grant moderator, member }    -- grant cone for role.assign

  doc chat                 { read member  write admin }
  doc group/{gid}/meta     { read members(group/{gid}/meta)  write member  validate "rules.group_meta" }
  doc group/{gid}/{day}    { shard by day  history since_join
                             read members(group/{gid}/meta)  write members(group/{gid}/meta)
                             validate "rules.message" }
  doc dm/{a}/{b}/{day}     { shard by day  read a, b  write a, b  validate "rules.message" }
  doc user/{did}           { read did  write did }                  -- per-user doc (§7)
  doc user/{did}/node      { read did  write node }

  uses "people" read                                                -- directory (§8)
  channel presence { send member  slot sender }
  channel typing   { send member  rate 4/s  slot sender }   -- per-group typing: see §11
}
```

| form | meaning |
|---|---|
| `doc <path/{vars}> { … }` | a declared doc pattern; an undeclared doc is refused (apps with a bare manifest keep membership-only behaviour) |
| `read` / `write` | **declarative only**: a role, a DID path variable, `node`, or `members(<doc path>)` — decided 2026-10-04 so the node knows every read rule's inputs and can re-fan-out when they change (§4). Write rules may add Lua through `validate`. |
| `shard by day\|month\|<field>` | `{day}`/`{month}` bound from `ctx.now`, or from a record field (a calendar shards by event start) |
| `history since_join\|all` | for `members(...)` docs: a new member's index lists shards from the join **day** onward, or all. Day precision, decided 2026-10-04: the join-day shard shows that whole day. |
| `validate "<module.fn>"` | Lua write rule (§5) |
| `derive "<out>" from "<in>" by "<fn>"` | node-only projection (§6) |
| `channel <name> { send … rate N/s slot sender }` | ephemeral, never persisted; node stamps the sender |
| `uses "<namespace>" read\|write` | cross-app binding, approved at install; access = binding ∩ the user's roles |
| `local "<path>"` | durable but never synced (drafts) |
| `sim "<file>" { tick N }` | node-authoritative simulation (games); later |

Parser rules, as built 2026-10-04:
- A doc name is `workspace::valid_doc_name`: segments of 1–128 bytes of `[A-Za-z0-9_:-]`,
  short enough to fit a resource address. Anything else matches nothing.
  *Revised 2026-10-04 (step 5):* `.` was allowed; dropped so every doc name is also a resource
  address.
- Where two patterns match a name, the one with a literal at the first differing position
  wins.
- `{day}`/`{month}` match only real dates.
- `owner` and `node` are reserved as role and variable names.
- Grant cycles and duplicate roles are refused.
- `derive`, `local` and `sim` are refused as not built yet.

## 4. Reads, indexes, fan-out

Each user has one index doc per workspace (§6 of `workspace-permissions-sync.md`), listing
every item and doc they may read. Users may write entries to their own index; each entry is
validated like a write (its path must be declared and creatable by the caller). Only the node
writes another user's index. Because read rules are declarative, a change to an input — a
role grant, a `members(...)` doc, a ban — tells the node exactly which docs to re-check, and
it adds or removes index entries and starts or stops pushes. Removal never retracts plaintext
already delivered; the UI says so.

## 5. Write rules in Lua

`validate(ctx, change)` runs on the node, sandboxed, with a step budget, no I/O, no writes.

```lua
ctx    = { caller, roles = { member = true }, vars = { gid = "…", day = "2026-10-04" }, now,
           doc = function(path) end,      -- read-only, node's copy, state before this batch
           rate = function(key, n, per) end, claim = function(key, holder) end }
change = { op = "insert"|"set"|"delete", path = { "messages", "<id>", "text" },
           before, after, record_before, record_after }
```

Paths use record ids, never list indexes. Any rejected change rejects the whole batch; the
writer's doc is replaced by the node's copy (see `group-chat-sync.md` §5).

**Standard helpers** (a Lua library shipped with the node; app rules compose them):

| helper | rule |
|---|---|
| `owned(field)` | insert: `after[field] == caller`; afterwards only that DID edits the record |
| `slot()` | the path ends in a DID that must equal the caller (reactions, votes, RSVP) |
| `immutable{…}` | id, author, created_at never change |
| `append_only{editable…}` | inserts only; named fields editable by their owner |
| `state_machine(field, T)` | transitions declared as data, each gated by a role or a fact |
| `refs(field, doc, coll)` | referenced ids exist (labels, assignees, parent) |
| `deny(list_doc)` | caller is not banned/muted (expiry checked) — checked first |
| `locked(parent_doc)` | a lock flag on the parent rejects child inserts |
| `soft_delete` | moderator sets `removed{by,at,reason}` only; author sets `deleted` only |

## 6. Node services

| service | for |
|---|---|
| `ctx.doc(path)` | facts: membership, bans, locks, prices, org chart |
| `ctx.rate(key, n, per)` | quotas, slowmode, spam — Lua is stateless |
| `ctx.claim` / `ctx.release` | uniqueness and counters across docs (a booked slot, stock, one response per person). Per-op validation cannot see a conflicting write in *another* doc; a CRDT cannot make a slot exclusive. Committed only if the batch is accepted. |
| `derive` | node-only docs from other docs: free/busy, tallies, karma, summaries, history/blame, guest projections. The only way to hide fields. |
| node actions | writes that cross audiences or have external effects (refund, anonymous submit, transfer between teams); idempotency key + durable result |
| node-written audit | accepted transitions append to an audit doc; client-written history is never trusted |

## 7. Per-user docs and request/response

Two patterns; the test is **who decides the result**.

- **Per-user doc** `…/{did}`: the user's own data, edited freely, checked by rules; the node
  may read it to derive summaries. Form answers, votes, RSVPs, read cursors, settings, cart.
- **Request → response**: the node must decide or act — claim something scarce, write where
  the user cannot, strip identity, cause an external effect. Booking, ordering, anonymous
  answers, join requests, refunds.

Two docs per user, not one per request:

```
user/{did}        user writes: own data + requests/{rid} = {kind, args, at}   (insert-only)
user/{did}/node   node writes: results/{rid} = {status, data, at}
```

`rid` is the idempotency key; the UI shows pending until a result lands. This is
`workspace-permissions-sync.md`'s "document-based submission/results".

## 8. People directory

A node-written doc per workspace: `did → {handle, name, avatar}`, maintained from membership.
Apps bind it with `uses "people" read`. Messages store DIDs, never handles (renames don't break
mentions); @-autocomplete reads the directory; a mention notifies only someone who can read
the doc it is in. Presence and other ephemeral messages carry the node-stamped DID and are
rendered through the directory. A guest reads a projection listing only people they share
something with.

## 9. App catalogue (what each needs)

| app | layout & boundary | rules beyond role read/write | node services |
|---|---|---|---|
| group chat | `group/{gid}/meta`, day-sharded messages, `dm/{a}/{b}`, threads per root | `owned` text, `slot` reactions, `soft_delete`, `deny` bans, pins by mod | `rate`, mention inbox |
| forum | `r/{c}/meta` (mods by rank, bans, flairs), posts by month, one doc per post's comments | `locked` thread, `owned`, mod `removed` | votes in private per-user docs, `derive` tallies/karma |
| announcements / Q&A | one doc, two audiences | insert needs `poster`; others `slot` only; `accepted` by asker | `derive` unanswered |
| spreadsheet | row-block shards, `meta` with protected ranges | cell in range writable by range owners | `derive` formula results |
| wiki | page tree, page per doc, suggestions/comments in page meta | suggestion `state_machine`, no reparent cycles, restricted pages via `members(...)` | `derive` history/blame |
| kanban | board (members, workflow), cards, comments per card | column `state_machine`, `refs` labels/assignees, WIP count | `derive` card history, guest card projection |
| shop | catalog, `orders/{buyer}/{month}`, staff fulfilment | order `state_machine`, price checked against catalog | `claim` stock, `derive` summaries, refund action |
| booking | provider availability, `bookings/{customer}/{month}` | transitions by provider/customer with time limits | `claim` slot, `derive` busy calendar |
| approvals | org doc, `requests/{submitter}/{year}` | `chain[step] == caller`, never self, frozen after final | chain stamped at submit, audit |
| forms | form def, `responses/{form}/{did}` | before `closes_at`, schema check | anonymous submit action + `claim` dedupe, `derive` summary |
| CRM | `teams/{team}/…` (Resource scope per team) | owner/assignee edit, lead reassigns | transfer action, `claim` unique email |
| calendar | events by month of start, `acl` doc, RSVP per DID | organizer edits, `slot` RSVP | `derive` free/busy |
| files | tree doc, blobs outside the CRDT | uploader/editor deletes, blob exists + quota | guest subtree projection, blob store |
| whiteboard | board (shards per frame when large) | locked layers, optional owned shapes | cursors on a channel |
| game | lobby, scores, no tick state in docs | seat claim by self, start by host, scores `write node` | `sim` tick, input/state channels |
| dashboards | reads others' namespaces | — | `uses` binding, `derive` aggregates |

## 10. Today's code, against this model

Found by reading, to be pinned by tests when the step that fixes each lands:

- **App- and resource-scoped tokens cannot sync.** `node_accept_sync` and subscribe check
  membership against `Scope::Workspace` (`courier/src/sync.rs:113`,
  `courier/src/subscribe.rs:61`); an `App`/`Resource` scope never contains a workspace.
  `Listen` checks `Scope::Node` (`subscribe.rs:79`), so a workspace-scoped member could not
  listen either — today's invites are `Scope::Node`, which hides it.
- **Revocation does not cascade.** Every real issue is recorded `Cause::Node`
  (`kunki/src/admin.rs:157,381,406`), including redeemed invites, so revoking an inviter
  leaves everyone they invited in place.
- **No app roles can be handed out.** `role.assign` is unbuilt and delegation cannot change a
  role.

*Fixed 2026-10-04 (group-chat-sync step 3):*
- Sync and subscribe target `Scope::App { ws, app: item }`, so node, workspace and that
  app's grants reach it. `Listen` accepts any live chain this node rooted.
- `role.assign`/revoke exist (`courier::role`, `Admin::assign_role`/`revoke_role`).
- Assigned and invited tokens are recorded `Cause::Under` the grant that allowed them, and
  `Admin::revoke` follows that lineage.
- Fan-out skips a subscriber with no live grant over the item.
- Found in review: a member could sync an edited source, rewrite `manifest.osv` and widen
  its own cone. A source sync that changes anything now needs `AppInstall`.
- Reconnect reissues record `Cause::Under` the token they replace.
- An invite with no recorded inviter, or redeemed after its inviter's grant was revoked,
  is refused.

*Fixed 2026-10-04 (step 5):* a doc's address is `ws/<ws>/<item>/<doc>`; sync and subscribe
target it, an app scope contains its own docs' addresses, and a `Scope::Resource` token reaches
the docs it covers (plus the app's source, to run it).

## 11. Open

- Encryption of shared content (osvauld1's per-page AES key wrapped per recipient) — deferred;
  the node sees plaintext by design (auditability). Revisit with remote relays.
- Install-time approval UI for `uses` bindings and role mapping.
- Exact-cut `since_join` (a membership change rolls the shard) — later; day precision now.
- `sim` and blob store — with the game and file apps, not chat.
- Parameterized channels (`channel typing/{gid} { send members(group/{gid}/meta) }`):
  until a channel has variables, `members(...)` in `send` may not use any (parser refuses
  it, 2026-10-04).
