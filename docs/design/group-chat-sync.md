# Group chat sync — permissions, workspace index, shards, ephemeral

Status: **steps 0–4 built 2026-10-04 (T1–T5, T13, T16, T23a green); steps 5–11 open.** A long-horizon,
test-first plan driven by `demo_apps/chat`: §0 lists the end-to-end tests that prove it done;
they are written first, then §7's steps make them pass. Branch `sync-hardening`. The general
permission model is [`app-permissions.md`](app-permissions.md); this plan builds the slice
chat needs and proves it end to end.

*Revised 2026-10-04 (after app-permissions.md): app roles get their own step (3); read rules
are declarative incl. `members(doc)`; private groups, reactions, moderation, bans and the
people directory join the scope (T17–T25); membership-driven re-indexing moves in from §8.*

*Revised 2026-10-04 (step 3 start): step 4 is built before step 3 — `role.assign` needs the
grant cone, which lives in the manifest. Numbers are kept so §0's step column stays valid.
T23's removal half needs step 7's rule, so it goes fully green there.*

## 0. Done means these pass

Written before the code they test. Each names the step (§7) that turns it green.

**End-to-end (`scripts/e2e_chat_sync.py`, one kunki + two or three offscreen shells, real
sockets, real Lua VMs, real vaults).** Every test saves a screenshot of each peer to
`$OSVAULD_SHOTS` (default `target/e2e-shots/T<n>-<peer>.png`), and a step is only judged done
after those images were looked at, not only the assertions.

| # | scenario | proves | step |
|---|---|---|---|
| T1 | A sends in #general; B, chat open, sees it without any sync request of its own | live push | 1 ✓ |
| T2 | B joins after A; the channel list shows each channel once | no duplicate first-run seed | 1 ✓ |
| T3 | B's shell is killed; A sends 5 messages; B restarts and sees all 5 | offline catch-up | 1 ✓ (index path: 8) |
| T4 | Node stopped; A and B each send; node restarted; both show the same messages in the same order | reconcile after partition | 1 ✓ |
| T5 | B has chat closed while A sends; B opens it and the message is there in the first frames | closed items catch up on open | 1 ✓ |
| T6 | Messages on day 1 and day 2 (virtual clock) land in separate shards; B opening chat loads only day 2; scrolling back pulls day 1 | shards + lazy load | 9 |
| T7 | C joins the workspace late; sees every channel and shard, and the chat app itself, without hand-fed ids | discovery through the index | 8 |
| T8 | Member B creates a channel (admin-only); node rejects; B's channel list rolls back; A never sees it | write rule + rollback | 5, 6 |
| T9 | B sends a message with `author = A`; node rejects; nobody sees it | node-side Lua validation | 7 |
| T10 | A–B DM: C's index never lists it; C requesting it by name is refused | DID read rule + index privacy | 5, 8 |
| T11 | A's presence shows on B by name; A's shell is killed; B shows A offline within 2 s; no node or desktop file holds presence | ephemeral presence | 10 |
| T12 | A types; B shows "A is typing"; A sending faster than the manifest's rate is dropped at the node | ephemeral channel + rate | 10 |
| T13 | Node restarted mid-session, including right after a peer joined; pushes resume without restarting the shells | Listen/subscribe recovery | 1 ✓ |
| T14 | B writes an index entry for a doc path the manifest doesn't declare; rejected, C never sees it | index writes are validated | 8 |
| T15 | B writes under `user/<A's did>`; rejected | DID segments bind to the caller | 2, 5 |
| T16 | A request whose `desktop_did` is A's but signed by B's key is refused; a replayed request is refused | proven caller | 2 ✓ |
| T17 | B edits A's message; rejected and rolled back on B; A's text unchanged everywhere | `owned` | 7 |
| T18 | B reacts 👍 on A's message: accepted. B removes A's reaction: rejected | `slot` | 7 |
| T19 | B deletes A's message: rejected. Moderator removes B's message: accepted, text kept for the mod log, hidden in the UI | `soft_delete` + app role | 3, 7 |
| T20 | Private group A+B: C's index never lists it; C requesting it is refused | `members(doc)` read rule | 5, 8 |
| T21 | A adds C to the group (`history all`): C sees it and its history. A removes C: C receives nothing new. A `since_join` group shows C only shards from the join day | membership-driven fan-out | 8, 9 |
| T22 | Changing a message's `author`, `at` or `id` after creation; rejected | `immutable` | 7 |
| T23 | Admin A assigns B `moderator`; B can then remove messages. Member C cannot assign roles; revoking A's grant revokes B's | `role.assign`, cone, cascade | 3, 7 |
| T24 | A moderator bans B for 10 min; B's post is rejected; after expiry (virtual clock) B posts | `deny` with expiry | 7 |
| T25 | Typing `@b` in A's composer suggests B's handle; the sent message stores B's DID; B gets a mention | people directory | 8, 10 |

**Rust (`cargo test`)** — each step also lands unit tests in the touched crate (manifest
parsing, rule matching, rejection, index fan-out, ephemeral relay), named in the step.

**Smoke.** Smokes stay sparse: one `scripts/smoke_chat_sync.py` running T1, T3, T9, T11
joins `SMOKES` at step 11. The full e2e file is run per step, not by `smoke.py`.

## 1. What

`demo_apps/chat` becomes a real group chat between people — public channels, private
groups, DMs, reactions, edits, moderation — which forces the missing parts of sync into
existence: proven identity, app roles, declared permissions enforced on the node, Lua write
rules, a per-user workspace index for discovery, time-sharded docs, and an ephemeral channel
for presence (the same channel carries in-game state later).

Before this plan (read from the code, 2026-10-04): sync, subscribe and push work over kunki's
local UDS bridge; authorization is workspace membership only (`courier::policy::membership`);
`manifest.osv` supplies a display name; `desktop_did` is request-supplied; only open docs
sync, always with full history; a push for a closed item is dropped; `PushReceived` ignores
the source layer; there is no index, no shard, no ephemeral path.

## 2. Proven caller

Every node request is signed by the desktop's key over `(verb, body hash, request_id,
ts)`; the node checks it against `public_key_from_did(desktop_did)`, rejects a `ts` outside
a window, and remembers `request_id`s inside that window. `verify_chain` already binds the
token to `sub == holder`; this binds `holder` to the connection's actual sender. Without it
every DID rule below is decoration.

As built (step 2): the signature covers `(osv/request/v1, node DID, request_id, ts_ms,
sha256(body))`, where body is the request's exact wire bytes, so no canonical encoding is
needed. A timestamp may be up to 60 s old and never in the node's future (desktop and node
share a clock today; a remote transport needs node-issued time). The replay set is in
memory, capped, and the node refuses anything stamped at or before its own start: an earlier
process only admitted requests stamped before it stopped, so a restart re-admits nothing.
`Ping` and the two claims stay unsigned: claims carry their own attestation.

## 3. Chat's manifest

The forms are `app-permissions.md` §3; chat's declaration:

```
app "chat" {
  roles admin, moderator, member
  role admin { grant moderator, member }

  doc chat                   { read member  write admin }             -- public channel list
  doc channel/{cid}/{day}    { shard by day  read member  write member  validate "rules.message" }
  doc group/{gid}/meta       { read members(group/{gid}/meta)  write member  validate "rules.group_meta" }
  doc group/{gid}/{day}      { shard by day  history all
                               read members(group/{gid}/meta)  write members(group/{gid}/meta)
                               validate "rules.message" }
  doc dm/{a}/{b}/{day}       { shard by day  read a, b  write a, b  validate "rules.message" }
  doc mod/bans               { read member  write moderator  validate "rules.bans" }
  doc user/{did}             { read did  write did }                  -- read cursors, drafts

  uses "people" read
  channel presence { send member  slot sender }
  channel typing   { send member  rate 4/s  slot sender }
}
```

- **Roles.** App roles are held as `Scope::App{ws, "chat"}` tokens, issued by invite or
  `role.assign`. The workspace `member` role maps to chat `member` by default (install-time
  approval of the mapping is out of scope).
- **Read rules are declarative** (decided 2026-10-04): a role, a DID path variable, or
  `members(<doc>)`. Lua is for writes only.
- **Undeclared doc → refused**, for an app whose manifest declares any `doc`. An app with a
  bare manifest keeps membership-only behaviour, so the other demo apps don't break.
- **Parsed once per publish/update** of the item's source; a source update that changes the
  manifest is a policy change, recorded on the node, not reinterpreted silently.

Message record and rules (`rules.lua`, composed from `app-permissions.md` §5 helpers):

```
message = { id, author, text, at, edited_at?, removed?, deleted?,
            reactions = { [did] = { ["👍"] = true } } }
```

- insert: `owned("author")`, `|at − now| < 300`, `deny(mod/bans)`
- `text`, `edited_at`, `deleted`: author only; `removed`: moderator/admin only, sets nothing else
- `reactions/<did>/…`: `slot()`
- `id`, `author`, `at`: `immutable`
- `group_meta`: on create `owner == caller`; `members` changed only by an admin listed in it

## 4. Enforcement on the node

On every sync the node resolves the doc name against the manifest (a doc name *is* its
path: `channel/general/2026-10-04`), binds the variables, computes the caller's roles from
its own grant records, checks `write` (for a non-empty update) and `read` (for what it sends
back), then runs `validate` if declared. Subscribe and push fan-out use `read`; a subscriber
who can't read a doc is never pushed it.

## 5. Rejection and rollback

A rejected sync merges nothing on the node and replies `Rejected { reason, state }` where
`state` is the node's current copy. The desktop **replaces** that doc with `state` (the
writer's rejected ops are the only thing lost — siblings in the same batch too, and a
batch is one flush, i.e. one user action), the app sees the reverted mirror next frame,
and the reason lands in the app's console. Replace, not a compensating revert op on the
node: a revert would leave the forged content in history every reader receives.

`validate(ctx, change)`: shapes in `app-permissions.md` §5 — `ctx` carries `caller`,
`roles`, `vars`, `now`, read-only `doc(path)` (the node's copy before this batch) and
`rate`; `change` is one record-level op with record ids in its path. It runs in a sandboxed
Luau VM inside kunki with a step budget, no I/O, no doc writes.

## 6. Workspace index, people, shards, ephemeral

**Index.** One Loro doc per (user, workspace): `items/<id> → {name, kind}` and
`docs/<item>/<path> → {by, at}`. Each user may write their own index; each write is
validated like a doc write — the entry's path must match a manifest declaration the
caller may *create* — and accepted entries are fanned out by the node into the index of
every user who can *read* them. Only the node writes into another user's index. A change to
a read rule's input (a role grant, a `members(...)` doc) re-checks exactly the docs that
depend on it and adds or removes entries. An entry announces existence, nothing more;
replica progress stays each device's own VVs. A desktop syncs its index first (on unlock,
claim and push); the diff is the catch-up list, which replaces `join_item`'s hand-fed ids
and `resolver_with_node`'s "nothing stored locally" guess. Lua reads it as
`doc:names(pattern)`.

**People.** The node-written directory (`app-permissions.md` §8): `did → {handle, name}`.
Chat stores DIDs, renders names through it, and autocompletes `@` from it.

**Shards.** `shard by day` binds `{day}` to `YYYY-MM-DD` of `ctx.now`; the node refuses a
day beyond tomorrow. `history since_join` lists shards from the join **day** onward (day
precision, decided 2026-10-04). Opening an announced shard that isn't local pulls it; older
shards are pulled only when the app opens them. `now()` follows a clock offset settable over
the bridge in offscreen mode, so day boundaries are driven, not waited for.

**Ephemeral.** `net.send(channel, value)` and `net.on(channel, fn(from, value))` in Lua.
Desktop → node is a signed `Ephemeral` request; node → desktop rides the existing `Listen`
connection. The node checks `send` and `rate`, stamps the sender's DID, and relays to every
listening desktop that can read the item — never persisted, never retried, dropped when a
buffer is full. Presence is the node's own: it emits `join`/`leave` for an item to its
readers when a desktop subscribes or its `Listen` connection drops. The transport is the
local socket behind the same trait an iroh adapter will implement; nothing in Lua sees which.

## 7. Steps

Each leaves `cargo test` for touched crates and the e2e tests turned green so far passing,
and gets a fresh `pi -p` review with the matching expert checklist before the user judges.

0. **Harness.** ✓ `scripts/osvauld/net.py`, `scripts/e2e_chat_sync.py`, `SyncNow`.
1. **Baseline fixes.** ✓ incremental `since`, first-sight sync, Listen re-subscribe, retry of
   failed subscribes, sync generation.
2. **Proven caller** (§2). ✓ `courier::proof`; the kunki bridge wraps every request in an
   `Envelope` and its `Gate` checks it before dispatch; shell2 signs as the vault account.
3. **App roles and scopes.** ✓ `role.assign` with the grant cone; invites carrying an app role
   at `Scope::App`; sync/subscribe/listen accept `App` scopes against the target (`Resource`
   moved to step 5); `Cause::Under` recorded so revocation cascades (fixes
   `app-permissions.md` §10); roles computed from the node's grant records. T23.
   As built:
   - The caller's authority is `Admin::grants` (live issues on record). The presented token
     only proves membership.
   - The node reads the cone from the `manifest.osv` in the item source it holds.
   - Invites remember their inviter by nonce (`invite-causes/`).
   - Fan-out re-checks each subscriber.
   - Bridge verbs `AssignRole`/`RevokeRole`.
   - T23a (the role half) is green. `Resource` scope waits for step 5.
4. **Manifest parse.** ✓ (before 3) A small `manifest` crate (pure, no Loro, no Lua): grammar of §3,
   errors with line numbers, `resolve(doc_name) → (decl, vars)`, plus `can_grant`/`satisfies`
   over the transitive grant cone (osvauld1's `ManifestAuthorizer`). Unit tests only.
5. **Declarative enforcement** (§4): roles, DID variables, `members(doc)` for read and
   write; `Scope::Resource` tokens reaching single docs. T8 (node half), T10 and T20
   (direct-request half), T15.
6. **Rejection and rollback** (§5). T8 green.
7. **Node Lua validation** (§5) with the helper library, `ctx.doc`, `ctx.rate`. T9, T17,
   T18, T19, T22, T24. Decide here whether the sandbox moves out of `app_host` into a crate
   kunki can share or kunki builds its own minimal one.
8. **Workspace index and people directory** (§6). T3 (index path), T7, T10, T14, T20, T21
   (fan-out half), T25 (directory half). Includes vault storage for the index on both sides
   and `doc:names`.
9. **Shards and history policy** (§6). T6, T21 (`since_join`), plus the chat app moving to
   `channel/<cid>/<day>`.
10. **Ephemeral channel and presence** (§6). T11, T12, T25 (mention half).
11. **Chat app finished and the smoke.** Real authors and names, groups, DMs, reactions,
    moderation UI; `smoke_chat_sync.py` into `SMOKES`; `status.md`, `lua-apps.md`
    (`net.*`, `doc:names`, manifest), `architecture.md` updated.

## 8. Out of scope

Remote transport (iroh) — everything here runs over local sockets on one machine;
multi-device for one account (the push registry is keyed by DID); content encryption keys
and grant bundles; install-time approval UI for role mapping and `uses`; `claim`, `derive`,
`sim`, blob store (later apps); exact-cut `since_join`; signed record fields (decided
2026-09-19, `workspace-permissions-sync.md`) — `author == caller` at the node is the
guarantee this plan gives; the single-threaded kunki bridge and frame-size caps
(`status.md` open items) unless a test trips them.
