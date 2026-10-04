# Group chat sync — permissions, workspace index, shards, ephemeral

Status: **steps 0–1 built 2026-10-04 (T1–T5, T13 green); steps 2–10 open.** A long-horizon, test-first plan
driven by `demo_apps/chat`: §0 lists the end-to-end tests that prove it done; they are
written first, then §7's steps make them pass. Branch `sync-hardening`.

## 0. Done means these pass

Written before the code they test. Each names the step (§7) that turns it green.

**End-to-end (`scripts/e2e_chat_sync.py`, one kunki + two or three offscreen shells, real
sockets, real Lua VMs, real vaults).** Every test saves a screenshot of each peer to
`$OSVAULD_SHOTS` (default `target/e2e-shots/chat-sync/T<n>-<peer>.png`), and a step is
only judged done after those images were looked at, not only the assertions.

| # | scenario | proves | step |
|---|---|---|---|
| T1 | A sends in #general; B, chat open, sees it without any sync request of its own | live push | 1 |
| T2 | B joins after A; the channel list shows each channel once | no duplicate first-run seed | 1 |
| T3 | B's shell is killed; A sends 5 messages; B restarts and sees all 5 | offline catch-up via the index | 7 |
| T4 | Node stopped; A and B each send; node restarted; both show the same messages in the same order | reconcile after partition | 1 |
| T5 | B has chat closed while A sends; B opens it and the message is there in the first frames, not after the 20 s tick | closed items catch up on open | 1 |
| T6 | Messages sent on day 1 and day 2 (virtual clock) land in separate shards; B opening chat loads only day 2; scrolling back pulls day 1 | shards + lazy load | 8 |
| T7 | C joins the workspace late; sees every channel and every shard, and the chat app itself, without hand-fed ids | discovery through the index | 7 |
| T8 | Member B creates a channel (admin-only); node rejects; B's own channel list rolls back; A never sees it | write rule + rollback | 4, 5 |
| T9 | B sends a message with `author = A`; node rejects; nobody sees it | node-side Lua validation | 6 |
| T10 | A–B DM: C's index never lists it; C requesting it by name is refused | read rule + index privacy | 4, 7 |
| T11 | A's presence ("online, in #general") shows on B; A's shell is killed; B shows A offline within 2 s; no node or desktop file holds presence | ephemeral presence | 9 |
| T12 | A types; B shows "A is typing"; A sending faster than the manifest's rate is dropped at the node | ephemeral channel + rate | 9 |
| T13 | Node restarted mid-session; T1 still works afterwards without restarting the shells | Listen/subscribe recovery | 1 |
| T14 | B writes an index entry for a doc path the manifest doesn't declare; rejected, C never sees it | index writes are validated | 7 |
| T15 | B writes an index entry under `profile/<A's did>`; rejected | DID segments bind to the caller | 2, 7 |
| T16 | A request whose `desktop_did` is A's but signed by B's key is refused; a replayed signed request is refused | proven caller | 2 |

**Rust (`cargo test`)** — each step also lands unit tests in the touched crate (manifest
parsing, rule matching, rejection, index fan-out, ephemeral relay), named in the step.

**Smoke.** Smokes stay sparse: one `scripts/smoke_chat_sync.py` running T1, T3, T9, T11
joins `SMOKES` at step 10. The full e2e file is run per step, not by `smoke.py`.

## 1. What

`demo_apps/chat` becomes a real group chat between people, which forces the missing parts
of sync into existence: proven identity, declared permissions enforced on the node, a
per-user workspace index for discovery, time-sharded docs, and an ephemeral channel for
presence and other non-persisted data (the same channel carries in-game state later).

Today (read from the code, 2026-10-04): sync, subscribe and push work over kunki's local
UDS bridge; authorization is workspace membership only (`courier::policy::membership`);
`manifest.osv` supplies a display name; `desktop_did` is request-supplied; only open docs
sync, always with full history (`since: None`); a push for a closed item is dropped;
`PushReceived` ignores the source layer; there is no index, no shard, no ephemeral path.

## 2. Proven caller

Every node request is signed by the desktop's key over `(verb, body hash, request_id,
ts)`; the node checks it against `public_key_from_did(desktop_did)`, rejects a `ts` outside
a window, and remembers `request_id`s inside that window. `verify_chain` already binds the
token to `sub == holder`; this binds `holder` to the connection's actual sender. Without it
every DID rule below is decoration (§3's `{did}` segments, §5's `author == caller`).

## 3. Manifest: declarations in `manifest.osv`, rules in Lua

The manifest is data the node parses before running any app code. It declares roles, the
docs an app may have (path patterns with `{var}` segments), who reads and writes each, an
optional shard rule, an optional Lua validator, and ephemeral channels:

```
app "chat" {
  roles admin, member

  doc chat {                       -- channel list
    read  member
    write admin
  }

  doc channel/{cid}/{day} {
    shard by day
    read  member
    write member
    validate "rules.message"       -- rules.lua, function message(ctx, change)
  }

  doc dm/{a}/{b} {                 -- a, b are DIDs, sorted
    read  a, b
    write a, b
  }

  channel presence { send member }
  channel typing   { send member  rate 4/s }
}
```

- **Role binding.** For this plan an app role names the workspace role it maps to
  (`roles admin = owner, member = member` or the bare form when names match). Install-time
  approval of that mapping (`workspace-permissions-sync.md` §4 "install is the
  authorization event") is out of scope here and keeps the default.
- **A DID variable used as a reader/writer** (`read a, b`) means "the caller's proven DID
  equals that segment". `owner` in a rule means the same for a `{did}` segment named in the
  path.
- **Undeclared doc → refused**, for an app whose manifest declares any `doc`. An app with a
  bare manifest keeps today's membership-only behaviour, so the other demo apps don't break.
- **Parsed once per publish/update** of the item's source; a source update that changes the
  manifest is a policy change, recorded on the node, not reinterpreted silently.

## 4. Enforcement on the node

On every sync the node resolves the doc name against the manifest (a doc name *is* its
path: `channel/general/2026-10-04`), binds the variables, checks the caller's role and DID
against `write` (for a non-empty update) and `read` (for what it sends back), then runs
`validate` if declared. Subscribe and push fan-out use `read`; a subscriber who can't read
a doc is never pushed it.

## 5. Rejection and rollback

A rejected sync merges nothing on the node and replies `Rejected { reason, state }` where
`state` is the node's current copy. The desktop **replaces** that doc with `state` (the
writer's rejected ops are the only thing lost — siblings in the same batch too, and a
batch is one flush, i.e. one user action), the app sees the reverted mirror next frame,
and the reason lands in the app's console. Replace, not a compensating revert op on the
node: a revert would leave the forged content in history every reader receives.

`validate(ctx, change)` gets `ctx = {caller, roles, vars, now}` and `change` = the
record-level difference the update makes (`{path, before, after}` entries, plain tables,
computed by importing into a scratch fork). It returns nothing to accept or errors to
reject. It runs in a sandboxed Luau VM inside kunki with a step budget, no I/O, no doc
writes — the same restrictions `index.lua` already has.

## 6. Workspace index, shards, ephemeral

**Index.** One Loro doc per (user, workspace): `items/<id> → {name, kind}` and
`docs/<item>/<path> → {by, at}`. Each user may write their own index; each write is
validated like a doc write — the entry's path must match a manifest declaration the
caller's role may *create* — and accepted entries are fanned out by the node into the index
of every user whose role can *read* them. Only the node writes into another user's index.
An entry announces existence, nothing more; replica progress stays each device's own VVs
(`workspace-permissions-sync.md` §6). A desktop syncs its index first (on unlock, claim and
push); the diff is the catch-up list, which replaces `join_item`'s hand-fed ids and
`resolver_with_node`'s "nothing stored locally" guess. Lua reads it as
`doc:names(pattern)` — names this item has, locally or announced.

**Shards.** `shard by day` binds `{day}` to `YYYY-MM-DD` of `ctx.now`; the node refuses a
day beyond tomorrow. Opening a shard that is announced but not local pulls it; older shards
are only pulled when the app opens them. `now()` follows a clock offset settable over the
bridge in offscreen mode, so day boundaries are driven, not waited for.

**Ephemeral.** `net.send(channel, value)` and `net.on(channel, fn(from, value))` in Lua.
Desktop → node is a signed `Ephemeral` request; node → desktop rides the existing `Listen`
connection. The node checks `send` and `rate`, then relays to every listening desktop that
can read the item — never persisted, never retried, dropped when a buffer is full.
Presence is the node's own: it emits `join`/`leave` for an item to its readers when a
desktop subscribes or its `Listen` connection drops. The transport is the local socket
behind the same trait an iroh adapter will implement; nothing in Lua sees which.

## 7. Steps

Each leaves `cargo test` for touched crates and the e2e tests turned green so far passing,
and gets a fresh `pi -p` review with the matching expert checklist before the user judges.

0. **Harness.** `scripts/osvauld/net.py`: a kunki plus N offscreen shells in one temp dir,
   signup/claim/invite/join/upload-chat, per-peer screenshots, a `SyncNow` bridge verb so
   tests never wait on the 20 s tick. `scripts/e2e_chat_sync.py` with T1–T16, each one
   selectable by name. All written now, all expected to fail except what step 1 covers.
1. **Baseline fixes.** Run T1, T2, T4, T5, T13 against today's code and fix what fails —
   expected: incremental `since` per doc, closed-item catch-up, seed duplication (seeds
   become admin-only first-run, or idempotent ids), Listen re-subscribe after node restart.
2. **Proven caller** (§2). T16; `courier` + `kunki` + `shell2::node` tests.
3. **Manifest parse.** A small `manifest` crate (pure, no Loro, no Lua): grammar of §3,
   errors with line numbers, `resolve(doc_name) → (decl, vars)`. Unit tests only.
4. **Enforcement** (§4). T8 (node half), T10 (direct-request half).
5. **Rejection and rollback** (§5). T8 green.
6. **Node Lua validation** (§5). T9. Decide here whether the sandbox moves out of
   `app_host` into a crate kunki can share or kunki builds its own minimal one.
7. **Workspace index** (§6). T3, T7, T10, T14, T15. Includes vault storage for the index
   on both sides and `doc:names`.
8. **Shards** (§6). T6, plus the chat app moving to `channel/<cid>/<day>`.
9. **Ephemeral channel and presence** (§6). T11, T12; chat shows who's online and typing.
10. **Chat app finished and the smoke.** Real authors (the account's DID and name),
    DMs, channel creation for admins; `smoke_chat_sync.py` into `SMOKES`; `status.md`,
    `lua-apps.md` (`net.*`, `doc:names`, manifest), `architecture.md` updated.

## 8. Out of scope

Remote transport (iroh) — everything here runs over local sockets on one machine;
multi-device for one account (the push registry is keyed by DID); content encryption keys
and grant bundles; install-time approval UI for role mapping; re-indexing on membership
change; signed record fields (decided 2026-09-19, `workspace-permissions-sync.md`) —
`author == caller` at the node is the guarantee this plan gives; the single-threaded
kunki bridge and frame-size caps (`status.md` open items) unless a test trips them.
