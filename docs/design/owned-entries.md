# Owned entries — only the author changes or deletes what they wrote

Status: **steps 1–4 built 2026-11-04, uncommitted: O1–O8 green** (O1, O2, O6, O7 in `scripts/e2e_owned.py`; O3–O5, O8 in `courier`). Site 3 (pull) deferred to the roster, §2.4. Agreed 2026-11-03 with the user; not yet reviewed. A long-horizon, test-first plan: §0 lists the
end-to-end tests that prove it done; §4's steps make them pass. Found on the Pixel during
laptop-host step 8: any member could delete every chat message, because `channel/{cid}` grants
`write member` to the whole document and the app writes `author = "me"`. Prior art and its
failures: osvauld1's `scribe/src/sync/apply.rs` and `docs/DESIGN_OSV_V3.md` §13 (summarised in
§3 below).

## 0. Done means these pass

**End-to-end (`scripts/e2e_owned.py`: two offscreen shells, A hosts, B joined, real vaults, real
Lua VMs, the chat app).** Each test screenshots both peers.

| # | scenario | proves | step |
|---|---|---|---|
| O1 | A and B each post. Each sees the other's real name, not "me"; edit and delete show only on their own messages | identity reaches Lua | 4 |
| O2 | B's app deletes A's message. The write is refused inside B's shell before it is committed; nothing is pushed; A still sees it | client-side check | 3 |
| O6 | B edits and deletes B's own messages; both sides converge | owners still can | 3 |
| O7 | A moderator deletes B's message; it syncs | the override | 3 |
| — | laptop-host H1–H7, H9 and `e2e_chat_sync.py` still pass | no regression | all |

**Host-side (`courier/tests/owned.rs`: real tokens, real `LoroDoc`s, a client that skips its
own check — Python cannot forge a Loro change, so these are integration tests, not e2e).**

| # | scenario | proves | step |
|---|---|---|---|
| O3 | B pushes a delete of A's entry. `accept_into` refuses, the live doc is untouched, the rejection carries the rollback | host-side check | 3 |
| O4 | B pushes an edit inside A's entry | edits, not only deletes | 3 |
| O5 | B pushes a change stamped with A's peer ID | the author cannot be forged | 1 |
| O8 | A push the host cannot read op by op is refused, not let through | fails closed | 2 |

**Unit (manifest + courier), app-agnostic:** `own` over a map (`cards/*`), a list, a movable
list and a tree (`blocks/**`) each refuses another member's edit, delete, overwrite and move,
and allows the creator's own. Deleting a tree node with someone else's child under it is
refused. A doc with no `own` line accepts everything `write` allows. The op reader (§2.2) sees each of map insert / map overwrite / map delete /
list delete / movable-list move and delete / tree create, move and delete / text edit, and
names the creator of what each one touches.

## 1. The idea in one paragraph

One shared document stays one shared document. Every Loro change already carries the peer ID
of whoever made it, and every child container, list element and tree node carries the peer ID
of whoever created it in its own ID. If a device's peer ID is derived from its device key, the
document itself records who made and who changed every entry. The host reads each incoming
update op by op — deletes included, because Loro's change log names the deleted element — and
refuses an op that touches an owned entry someone else created. The same check runs in the
writer's own shell before it commits, so a refused write never leaves the device. No
signatures yet: the connection already proves the sender to the host (§5).

## 2. Design

### 2.1 Peer ID = device key

- Every `LoroDoc` a device writes gets `set_peer_id(peer_of(device_did))`, where `peer_of` is
  the first 8 bytes of `blake3("osvauld-peer" ‖ device_did)`. Today peer IDs are random.
- The host refuses a push containing any change whose peer isn't `peer_of(hello.desktop_did)`.
  The exception is an appointed host relaying for others (laptop-host H7). The host is trusted
  for that today, and signatures (§5) remove that trust later.
- Any device maps a peer back to a DID through the space's member roster, which already lists
  device DIDs.
- **Constraint:** Loro needs one writer per peer ID at a time. The "one copy per doc" rule
  (laptop-host §2) already gives each device a single live copy of each doc. A second
  concurrent copy would corrupt the document, so the binding asserts there is only one.
- Docs written before this change keep their random peers. Entries they created belong to no
  member, so only the moderator override can delete them. The seed data is recreated.

### 2.2 The op reader

`courier::ops::read(live: &LoroDoc, update: &[u8]) -> Result<Vec<Op>>` imports the update into
a fork of the live doc and reads the new changes with `export_json_updates(before, after)`.
Each `Op` says:

- who made it (the change's peer);
- which container it touches;
- what kind it is: insert, set, delete or move;
- who created the thing it touches, read off the ID:
  - a list or movable-list delete: `start_id.peer`;
  - a tree op: `target.peer`;
  - an op inside a child container (editing a message's `text`): the container ID's peer;
  - a map delete or overwrite: only the key is named, so the old value is read from the live
    doc *before* the import and its creator peer is used.

Any decode error refuses the whole update. osvauld1 logged the error and carried on with an
empty op list, which let the update through.

### 2.3 Rules are data in the manifest

*Agreed 2026-11-03 after reading Cedar, CEL and Firestore (the research run's surviving
evidence), osvauld1's three `.osv` grammars, and `app-permissions.md` §5. Supersedes §5's
node-only Lua `validate` (see the dated note there).*

```
doc board/{id} {
  read member  write member                       -- the permit; rules only narrow it
  own cards/* override admin                      -- sugar for the first forbid below
  rule cards/* {
    forbid set, delete, move  unless actor is entry.creator or actor has admin
    forbid set where changed state
           unless state from todo to doing, doing to done, doing to todo
    forbid insert, set where after.title is missing or len after.title above 200
    forbid set unless changed only title, state, body
  }
  rule tree/** { forbid set, delete, move unless actor is entry.creator or actor has editor }
}
```

**Shape.** `write` is the permit. A `rule <path>` block holds `forbid` lines that narrow it;
there is no `allow`, so two rules can never widen each other (Firestore's overlapping-`allow`
trap). An op is admitted when it matches no satisfied forbid; a change (one Loro commit, many
ops) is admitted only when every op in it is. A path with no rule inherits `write` as today.

- **Ops:** `insert`, `set`, `delete`, `move`. **Paths:** `*` one level, `**` any depth.
- **Operands:** `actor`, `entry.creator` (read off the Loro ID, never a field the app wrote),
  `before.<field>`, `after.<field>`, `changed <field>`, `changed only <fields>`, `len <x>`,
  literals. **Operators:** `is`, `is not`, `in`, `above`, `below`, `is missing`, `has`,
  `<field> from a to b, …`, `and`, `or`, `not`.
- **Semantics pinned:** left-to-right short-circuit; any error inside a rule (a missing
  operand, a type mismatch) makes that rule refuse — not skipped as in Cedar, not absorbed
  as CEL's `error || true`. `len` counts Unicode scalars; invalid UTF-8 refuses.
- **Bounded by structure:** no recursion, no user functions, no quantifier over a collection,
  no concatenation, no regex. Cost is the size of the rule, so there is no step budget to get
  wrong, and "who can do X" stays decidable for an offline tool later.
- **`own <path> [override <role>]`** is sugar for the first forbid line and stays the common
  case. **`pattern`** (osvauld1 V3's named rule bundle) is a later convenience, not v1.
- **One engine**, in `courier`, pure over `(rules, roster, ops)`. Lua never evaluates rules;
  it asks `d:allowed{ kind, path }`.

**Generic, not chat.** The engine knows containers, entries, creators and ops; nothing about
messages. Most apps are a collection of entries that each have an author:

```
doc board/{id}  { write member  own cards/*      override admin }      -- kanban cards in a map
doc page/{id}   { write member  own blocks/**    override editor }     -- tree nodes, any depth
doc poll/{id}   { write member  own votes/*                     }      -- one vote per entry
doc sheet/{id}  { write member }                                       -- no rule: fully shared
```

`/**` owns every node under a tree, each by its own creator. A child created by someone else
inside your block is theirs; deleting your block deletes theirs too, which touches theirs, so
it is refused unless the role overrides. A doc with no rule behaves exactly as today:
collaborative text, shared settings and whiteboards stay fully shared.

### 2.6 Refusal

A refused push is the existing `Rejected` path (`sync.rs`, group-chat-sync §5): the host
merges nothing and the writer's copy is cut back to what the host holds. Agreed 2026-11-03:
the node's state wins; nothing is quarantined or forked. With the pre-commit check (§2.4,
site 1) a refusal reaches the host only from a buggy or hostile client. Direct peer-to-peer
sync, if ever added, reopens this — p2panda's answer is to carry, with each refusal, the set
of ops that descend from it — and is out of scope here.

### 2.4 Where the check runs

The same function, `courier::rules::check(manifest, decl, actor, before, read)`, over
`courier::ops::read`, runs in three places:

1. **The writer's shell, before it commits.** `app_host` applies the Lua write to a scratch
   fork of the live doc, reads it (`ops::read_since`), judges it, and only then applies it to
   the live doc. A refused write becomes a Lua error at the call site (`d:delete` raises) and
   the live doc is never touched — no rollback a frame later. This step is O2.
2. **The host, in `accept_into`, before the import.** It runs next to the existing
   `write`/`AppInstall` checks, and a refusal is the existing `Rejected` path: the writer cuts
   its copy back to what both sides hold. This step is O3–O5. *Built 2026-11-03.*
3. **Every client, on pull.** A pull that breaks a rule is refused and logged. Against an
   honest host this never fires. It catches a buggy host and keeps one rule set everywhere,
   which osvauld1's V3 doc asked for. A lying host needs signatures to catch (§5).
   *Deferred 2026-11-03:* `actor has <role>` needs the pusher's roles, and a client knows no
   member's roles but its own — there is no synced roster yet (§5). Until then site 3 would
   refuse an admin's legitimate override and fork the client. It lands with the roster.

**Actor is a device, for now.** `actor is entry.creator` compares peers, so a member editing
from a second device is "someone else" to their own entries until devices are tied to
accounts through the roster (§5). The demo's accounts each have one device.

### 2.5 What Lua sees

- `me()` returns `{ did = ..., name = ... }`.
- `d:owner(path)` returns the DID that created the entry, read from its ID rather than from a
  field the app wrote.
- The chat app drops the `author` field, shows `owner` through the roster's names, and draws
  edit and delete only when `d:owner(...) == me().did`.
- `docs/lua-apps.md` gains both calls.

## 3. What osvauld1 did, and what we keep

| osvauld1 | here |
|---|---|
| Op extraction diffed into an empty temporary doc, so deletes were invisible, and author-only delete was "unenforceable" (V3 §13.1) | The reader forks the *live* doc and reads Loro's change log, so deletes name their target |
| An extraction error logged a warning and carried on with an empty op list (`apply.rs:257`) | Any decode error refuses |
| Peer IDs were never bound; the handshake signature was an empty `vec![]` | Peer = device key; a push in another peer's name is refused |
| A Lua `validate_ops` ran per request with no fuel, a real clock and write access | Declarative first; a Lua hook is a later, refuse-only, fuelled step (§5) |
| Rules ran only on the node; the V3 doc warned peers would diverge | One `check` runs in the writer's shell, on the host and on pull |
| A rejection was dropped silently and the client kept retrying | The existing `Rejected` path rolls the writer back |

## 4. Steps

1. **Peer binding.** `peer_of`, set on every doc a device opens; the host refuses a push in
   another peer's name. O5 turns green.
2. **The op reader**, with its unit tests and fail-closed decoding. O8 turns green.
3. **The rules.** `rule`/`forbid` and the `own` sugar in `manifest`; the engine in `courier`
   with a corpus of `(rules, context, decision)` cases; `check` at the three sites. O2–O4,
   O6 and O7 turn green. Transitions, `len` and `changed only` ship here but the chat uses
   only `own`.
4. **Lua identity.** `me()`, `d:owner()`, `d:allowed()`, and the chat app using them. O1
   turns green.

Each step gets a fresh `pi -p` review: expert-secure-core for steps 1–3 (courier is
security-critical), expert-app-host for steps 3–4, and expert-lua-app for the chat.

## 5. Later, not here

- **Signed changes.** These let a member verify an author without trusting the host, and let
  a host relay without being trusted.
- **A Lua validator**, for rules the manifest can't express (for example, "only the poll's
  creator closes it"). It would run in the same three places, refuse-only, with a fuel limit
  and no clock, no I/O and no writes.
- **Field-level rules**, such as an immutable `sent_at`.

## 6. Open questions for the user

- Should an owner's delete really remove the entry, or leave a "deleted" tombstone that
  others can see?

*Settled 2026-11-03:* the syntax (§2.3, rules as data; Lua only queries), and refusal (§2.6,
the node's state wins).
