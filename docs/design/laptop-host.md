# Laptop host — collaborative artifacts without a node

Status: **planned 2026-11-02, nothing built.** A long-horizon, test-first plan: §0 lists the
end-to-end tests that prove it done; they are written first, then §5's steps make them pass.
Supersedes the kunki-as-separate-process transport story in
[`group-chat-sync.md`](group-chat-sync.md) §2 and the "remote transport (iroh) remains unbuilt"
line in `architecture.md`'s kunki row; the permission model there (`app-permissions.md`,
`courier`, `manifest`) is unchanged. Prior art is Tinline (`~/p2p_chat`), built on our
identity crates with this exact topology shipping on phones.

## 0. Done means these pass

Written before the code they test. Each names the step (§5) that turns it green.

**End-to-end (`scripts/e2e_host.py`, two or three offscreen shells, real vaults, real Lua VMs,
real iroh over loopback with the relay disabled, `OSVAULD_KUNKI_SOCKET` unset — there is no
socket to fall back to).** Every test screenshots each peer to `target/e2e-shots/H<n>-<peer>.png`;
a step is judged done only after the images were looked at.

| # | scenario | proves | step |
|---|---|---|---|
| H1 | A opens a space; `Whoami` says A hosts it from A's own device endpoint. No second process, no second account, no socket file | host = the app | 4 |
| H2 | The agent builds an app in A's space over the bridge; A shares the space → a link. B pastes the link, sees the space's items **without hand-fed ids**, opens the app and sees A's data | link, fetch, browse, pull | 6 |
| H3 | A and B both edit; each sees the other's edit within a few frames, no sync request of its own | session relay | 5 |
| H4 | A's shell is killed; B edits (B's console says `host offline`); A restarts; both converge in the same order | offline, reconcile | 5 |
| H5 | A forged link, an expired link and a third wrong code on one connection are refused; a 70 KiB first frame is dropped before allocation; B with a clock +5 min still syncs | peer-facing guards | 3, 6 |
| H6 | `scripts/e2e_chat_sync.py` T1–T5, T8, T13, T15, T16, T23a, T8a, T10a, T15a, T20a pass over sessions. T16 reads: a device whose attestation is for another DID is refused; a `desktop_did` that is not the attested one is refused | no regression; proven caller by device | 5 |
| H7 | A appoints a headless `kunki` as host of the space; A goes offline; B's edits still sync through kunki; A returns and converges | appointed hosts, host↔host relay | 7 |
| H8 | A Pixel joins by a typed link, no ADB socket, and plays the edit back | mobile over iroh | 8 |
| H9 | `scripts/e2e_artifact.py`: the whole story driven by the Python client — make, share, friend joins, both edit, friend reopens later | the product path; also the demo | 6 |

**Rust (`cargo test`)** — each step lands unit tests in the crate it touches: attestation open/refuse,
frame caps, the session state machine, link encode/decode, spent-code set, host appointment.

**Smoke.** `scripts/smoke_host.py` runs H1, H2, H3 and joins `SMOKES` at step 9. The full e2e file
is run per step, not by `smoke.py`.

## 1. What and why

The pivot (2026-11-02): someone asks the agent for a small app, it appears, they share it, and
their friends use it live — with the maker's laptop as the host. A hosted node is optional and
later; nobody on the member side can tell which they joined.

Before this plan (read from the code 2026-11-02): sync, subscribe, push, invites, app roles and
manifest-declared read/write rules all work — over kunki's unix socket, on one machine
(`shell2/src/node.rs`, `kunki::bridge`). Android reaches a node only through an ADB-forwarded
socket; iOS cannot at all. There is no way to list what a node holds (`join_item` needs ids typed
in), tickets are printed to the terminal, the request proof assumes a shared clock (60 s), and
`osvauld_rpc::read_msg` allocates whatever length a stranger sends.

## 2. The model

### Trust root = the space owner

A space's tokens chain to its **owner's DID**, not to a node's. The owner's device is the first
host. There is no boot ticket and no claim: opening a space you own makes you its host. Everything
`kunki::admin` records (issues, revocations, spent invites) is host state in the owner's own vault
entries, under `host/<ws>/…`, and `courier::token`'s chains verify unchanged with `root = owner`.

A **host** is a device holding a `host` role at `Scope::Workspace(ws)` and a full copy of the
space. Any number; the owner is one by appointment-of-self. A headless `kunki` is a member
identity that was appointed (step 7), the same code the laptop runs minus the UI — Tinline's
`peer group-host` (`~/p2p_chat/crates/peer/src/group_host.rs`). Members dial hosts; hosts relay
every accepted update to every other open session that may read it; hosts dial each other. All
hosts offline → nothing syncs; members keep editing locally and converge when one returns (T4's
path, already proven). Wake push is out of scope (§6).

`Scope::Node` stays only for the headless binary's own bookkeeping; nothing in a space's chain uses it.

*Revised 2026-11-02 (step 4, as built):* the owner's own grant is the same root token a
claimant used to get — `owner` at **`Scope::Node`**, issued by the owner to the owner
(`courier::self_claim`) on every unlock, recorded in `kunki::admin` over the owner's own vault.
`Scope::Node` therefore means "this account's host as a whole"; members' tokens are the
narrower scopes. `kunki::admin`'s records keep their keys (`token/`, `users/`, `revoked/`,
`subscriptions/`): one host per account, so a `host/<ws>/` prefix bought nothing. A space this
account *joined* from another host is marked `joined/<ws>` (written at step 6), and `hosting`
is every workspace without that mark. `shell2::node::Link` routes a request in-process
(`Here`) when the remembered relationship names this account, over the kunki socket when it
claimed another node — so `e2e_chat_sync.py`'s node mode still runs until step 5 retires the
socket. A claim always goes out over the socket: it names a new node, never the one this
account already is.

**One copy per doc (step 5's first requirement, found by step 4's review).** With the host in
the owner's process, a doc the owner has open exists twice: the app thread's live `LoroDoc`,
flushed to the vault on every change, and the vault snapshot `Admin::accept_sync` reads,
merges and stores. Two writers of one key: a flush can overwrite a member's just-merged
update until the owner's app imports the push and flushes again — convergent, but a window
in which the host's copy is behind. Unreachable in step 4 (only the owner writes, after its
own flush); real once members' sessions arrive. The fix is Tinline's shape: the host's copy
*is* the live doc. Every incoming session update goes through the UI thread, which merges
into the open app's doc (`courier::sync` gains a transition over a `&LoroDoc`) and only falls
back to the vault snapshot for a doc nobody has open. The vault then has one writer per doc
again — the app's flush, or the host's store, never both.

### Device key = endpoint id

Each install gets a **random per-install device key**, sealed in the vault beside the identity
(`entry/device/secret`), never derived: our HKDF device key (`identity.rs` `osv/identity/device/v1`)
is deterministic, so a laptop and a phone restored from one phrase would share an endpoint id. The
derivation contract is frozen and untouched; the derived key simply isn't the endpoint key.

The device key's public half is the iroh `EndpointId`. The DID signs an **attestation**
`{did, device, iat}` ("this device is mine"); every session opens with it and the host checks the
attested device against the QUIC-authenticated remote key. Transport proves the device, the
attestation proves the person, and `desktop_did` in a body must equal the attested DID.
`courier::proof` and the node's replay set are retired: the handshake already binds the sender,
QUIC does not replay streams, and nothing needs a shared clock (issued statements allow 5 min skew,
order is causal version vectors). Several devices per DID are fine; linking a second device of one
person (Tinline's `OSVL1:` flow) is later.

### Transport

One iroh endpoint per shell, bound to the device key, ALPN `osvauld/space/1`, n0 relay preset,
`P2P_RELAY_ONLY` for tests. Frames are u32 big-endian length + postcard body: first frame ≤ 64 KiB,
later frames ≤ 16 MiB; dial 20 s, reply 10 s, retry 2/5/15/30/60 s, idle close 60 s. Lifted from
`~/p2p_chat/crates/node-runtime/src/{lifecycle,frames,wire}.rs`. One tokio runtime on its own
thread; shell2's existing "spawn a thread, block, `send_event`" shape calls into it with
`Handle::block_on`, so the UI thread never sees a future.

Kunki's unix socket, `OSVAULD_KUNKI_SOCKET` and the ADB socket path are deleted. The in-process host
is called directly. `osvauld-rpc`'s socket stays — it is the agent bridge, not node traffic — and
gains the same frame cap.

*Revised 2026-11-02 (step 3, as built):* frames are **bincode**, not postcard — courier's
payloads already are, and nothing here is hashed or signed by its frame bytes, which was
postcard's reason in Tinline. The mode knob is **`OSVAULD_NET`** (`auto` | `local` |
`relay-only`), one name for the three cases, rather than Tinline's `P2P_RELAY_ONLY` plus a
preset; `local` is direct addresses only, with no mDNS (iroh 1.3 core has none), so a local
link carries the host's addresses as iroh's own tickets do. The GUI's way in is
**`Net::block_on`** — a spawn and a channel, usable from any thread — not `Handle::block_on`,
which panics on a runtime thread. The receiver reads the four header bytes and nothing more
until the claimed length is under the cap, then exactly that many.

### Sessions replace Sync/Subscribe/Listen/tick

One long-lived bi-stream per (member device, host, space):

```
member → host   Hello { attestation, token, ws, docs: [(name, vv)] }
host   → member Welcome { docs: [(name, vv)] } | Refused { reason }
both            Update { doc, bytes }            -- only what the other lacks (export_since)
both            Ack { doc, vv }
host   → member Rejected { doc, reason, vv }     -- manifest rule said no; member rolls back
```

Meta/src first, then docs. The host judges every `Update` with `courier::access` against the
manifest exactly as `Admin::accept_sync` does today, stores it, and relays it to its other
sessions that may read that doc (the per-subscriber read re-check stays). `Subscribe` is implicit
in the session; a push is a relayed `Update`. A member dials on open and on local change, with the
retry ladder; sessions close idle. `courier::sync`'s pure transitions (`desktop_start_sync`,
`SyncAck.vv`, the `missing`/full-resend rule) are kept behind the new frames; the 20 s tick, the
2 s listen loop, `subscribe_if_new` and `spawn_push_listener` go.

### Sharing = a link

`osvauld://s/<base58>` = host endpoint id + 12-byte code + relay hint, ~77 characters. Dialling it
with `Fetch { code }` returns the signed invite (a `Scope::Workspace` token, 7-day bearer, single- or
multi-use as `courier::invite` already distinguishes) and every current host's address. Guessing is
rate-limited: 300 ms per wrong code, three wrong codes close the connection, 30/min per host. Lifted
from `~/p2p_chat/docs/group-core.md` §invite links. A joined member's shell pulls the space's
published headers (`Browse`) and adopts them — the end of `join_item` and its empty-doc overwrite.

Desktop: the link in a text box with a copy button (`arboard`); the join box accepts paste
(runtime editor gains `Ctrl/⌘+V`). Phone: typed link, or an xnet name that resolves to one
(`xnet-names.md`). QR is later.

*Revised 2026-11-02 (step 5, as built — steps 5 and 6 landed together, since a session test
needs a joined member and a join needs a session):*
- **`Browse` is `Fetched`'s payload.** The answer to a code carries the invite, the hosts and
  the space's headers (`ws`, `items`); there is no separate verb. `JoinSpace` adopts them,
  saves `joined/<ws>` (host, address, and later the minted token), opens the first session
  with the invite claim and a pull of every item's source, and answers once the sources are
  stored — so an item can be opened the moment the reply lands. Asked before the endpoint is
  up, it waits for the bind.
- **A pull is answered with one `Update`, always, before the `Ack`** — empty when the host
  holds nothing, and also when the host refuses — so a member's app thread, which asks the
  host for a doc before seeding it (`resolver_with_session`), waits for exactly one frame.
  The session loop hands that frame to the waiting thread itself: the UI thread may be
  waiting on that very app thread to finish opening.
- **The owner's own edits never take the vault round trip.** A doc the owner has open *is*
  the host's copy; its edits only fan out to the subscribed sessions from the live doc
  (`here_fan_out`). Member updates merge into it through the UI thread (`Admin::judge` +
  `accept_live`), into the vault only for a doc nobody has open.
- **`Role` frames** carry a member's `AssignRole`/`RevokeRole` to the host, answered by
  `RoleDone`; the owner's go to its own admin as before.
- **A vanished host is noticed by QUIC's idle timeout** — 10 s (keep-alive 5 s), not Tinline's
  20 s — then the retry ladder redials; the app's console says `host offline: <why>`.
- **The endpoint's UDP port is remembered per account** (`net/port`, sealed), so a local
  link's direct addresses survive the host's restart; a busy port falls back to any.
- **`Advance` moves the shell's clock** (`shell2::clock`) as well as the runtime's, so share
  codes expire in tests.
- Not done here: the copy button, paste in the editor, idle close of a quiet session, and
  the 20 s `SyncTick` (still the backstop for the socket route; for a hosted space it fans
  out the live doc again, harmless and removable).

## 3. What stays, what Tinline lends

Stays ours: `manifest.osv` and `courier::{token, policy, role, access, invite, sync}`; the agent
bridge; the Lua runtime; `vault` as the sealed layer; `identity`'s frozen derivation.

Lent by Tinline, as lessons with paths, not crates: random device key + attestation
(`crates/proto/src/lib.rs:109-179`, `docs/protocol.md` §Identities), endpoint bind and QUIC
config (`node-runtime/src/lifecycle.rs:173-204`), frames and caps (`node-runtime/src/frames.rs`),
timeouts (`node-runtime/src/wire.rs:13-23`), the group session shape (`group-runtime/src/wire.rs`,
`docs/group-core.md` §80-139), invite links and their rate limits (`docs/group-core.md` §262-293),
the headless host (`crates/peer/src/group_host.rs`), tokio placement in a GUI
(`crates/core/src/node/mod.rs:148-160`), and `storage::apply` (atomic multi-op,
`crates/storage/src/store.rs:157`) plus `verify_strict`.

Not adopted: Tinline's record-keyed Loro model (our docs are judged by manifest rules, not per-record
signatures), `codec`/`hash`/`Secret32`, `Sealed`, rotation (mechanism ports later, off by default
there too), iroh-blobs, wake push.

## 4. Decisions (2026-11-02, with the user)

1. **Owner-rooted trust; hosts are appointed devices.** Not "embed kunki with its own node
   identity": that builds a second identity per laptop we would later delete, and moving a host
   would mean re-claiming a new root.
2. **iroh key is the device key; identity is different.** Random per install, attested by the DID.
3. **QUIC device auth + attestation replaces `courier::proof` per request and the replay set.**
4. **One session per (member, host, space)** replaces Sync/Subscribe/Listen/tick.
5. **Secure core: no crate swap.** Only `storage::apply`, `verify_strict`, the device-key entry.
6. **Share unit = the space.** Links, not tickets.

## 5. Steps

Each step leaves `cargo test --workspace` and the smokes green and is reviewed by a fresh
`expert-*` instance before the user judges.

1. **Tests.** `scripts/e2e_host.py` (H1–H7, H9 as skips where the verb is missing) over a
   `Net` that starts no kunki; all red.
2. **Secure core** (`expert-secure-core`): per-install device key sealed in `vault`
   (`entry/device/secret`, created at signup/login if absent), `Vault::device_public_key`,
   `courier::attest`/`verify_attestation`; `storage::apply` + `scan`; `verify_strict`.
3. **`net` crate** (`expert-architect`): tokio thread, endpoint on the device key, frames, caps,
   timeouts, relay-only flag, `Fetch` rate limits as pure state. Unit-tested over loopback.
   H5's frame and skew halves.
4. **Host in shell2** (`expert-shell`): `kunki::admin` becomes `host` state keyed `host/<ws>/…`
   in the owner's vault, root = owner DID; opening an owned space serves it; `Whoami` reports it.
   `Scope::Node`, boot tickets and `ClaimNode` leave the shell. H1.
5. **Sessions** (`expert-architect`): the frame set above in `courier::session` (pure) and
   `net`; shell2's node worker becomes a session table; rejection/rollback and the
   `missing`/resend rule carried over; account-generation scoping applied to every completion
   (closes the `NodeRpcDone` hazard). H3, H4, H6.
6. **Link, fetch, browse** (`expert-shell`): `osvauld://s/…`, `Fetch`, `Browse`, join adopts
   headers; copy/paste; `e2e_artifact.py`. H2, H5's link half, H9.
7. **Headless host** (`expert-secure-core`): `kunki --data DIR host --join LINK --accept WS`;
   `host` role token; host↔host sessions; the binary's own bookkeeping stays at `Scope::Node`. H7.
8. **Mobile** (`expert-shell`): Android drops the ADB socket; iOS dials the same `net`; typed
   link/name join. H8 on the Pixel, manual acceptance.
9. **Docs and smoke**: `architecture.md` kunki/courier rows, `status.md`, `group-chat-sync.md`
   §2 revision note, `smoke_host.py` into `SMOKES`.

## 6. Out of scope, open

- Wake push (Tinline: a Cloudflare worker + FCM/APNs, `docs/push.md`) — a sleeping phone is not
  woken; a sleeping laptop is an offline host.
- Presence/typing (T11/T12), shards (T6), node-side Lua validation (T9, T17–T22), the index proper
  (T7 — `Browse` is its minimal form): they stay in `group-chat-sync.md`.
- Device linking (one person, two devices), endpoint rotation, QR, iroh-blobs for large assets.
- **Open:** when the owner is offline and only an appointed host is up, who issues invites and
  role changes? Tinline answers "any admin's device, as signed records the host stores" — we
  will need the same once H7 exists; until then issuance needs the owner online.
- **Open:** whether a member device may also be dialled by other members for the ephemeral
  channel (presence) without being a host.

## 7. Hazards this closes

From `status.md` §"Workspace permissions, sync, and sovereign node": caller identity now bound by
transport (step 5); async completions scoped to an account generation (step 5); `join_item`
overwrite (step 6); bridge frame cap (step 3). Still open after this plan: write deadlines on the
agent bridge.
