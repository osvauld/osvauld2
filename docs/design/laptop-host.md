# Laptop host — collaborative artifacts without a node

Status: **steps 1–7 built 2026-11-02 (H1–H7, H9 green; H8 waits on a Pixel); steps 8–9 open.** Each
step's as-built revision sits in §2 under a dated note. A long-horizon, test-first plan: §0 lists the
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

### Appointed hosts (step 7, designed 2026-11-02 with the user)

A second host must judge members exactly as the owner does, and the owner's judgement reads
three things: every token it minted for that DID, its revoked set, and the app's manifest. The
manifest already travels with the space; the other two become **the ledger**.

**The ledger** (`courier::ledger`). One entry per fact the owner's admin records —
`Issued(token)` and `Revoked { id, at }` — each **signed by the owner's DID key**
(`Signed { body, sig }`, verified with `verify_strict` against the owner DID, the shape of
`courier::attest`). Per space: every issue whose scope is `Node` or inside `Workspace(ws)`,
and every revocation. It is carried by a `Frame::Ledger { entries }` that always holds the
**whole** ledger: replay is idempotent (`record` and `mark_revoked` put by id), so a resend
costs a few KiB and needs no cursor — a `since` is the obvious later economy. An appointed
host replays it into its own `kunki::admin`, kept under `hosted/<owner-did>/…` so that two
owners' `Scope::Node` tokens never meet; that admin's **root is the owner's DID**, not the
host's own (`Admin::hosted`). Nothing the host decides is its own authority: it holds the
owner's ledger and the owner's docs, both verified at the door.

**Appointment** = a `host` role token at `Scope::Workspace(ws)` (`courier::role::HOST`),
minted by the owner to a member's DID (`AppointHost { ws_id, did }` over the bridge,
`Admin::appoint_host`). It is one more ledger entry; the member learns it is a host by
replaying the ledger and finding that token issued to itself. A holder of `host` on a space
is judged with `Access::OPEN` on every doc of it (`Admin::judge`): what it relays was judged
once already, by whichever host accepted it.

**Hosts dial the owner, members dial any host.** An appointed host keeps one ordinary member
session to the owner — its *uplink* — opened by `kunki --data DIR host --join LINK --accept
WS` (join by link, claim, then wait to be appointed). Once the owner sees a session whose DID
holds `host` on that space it becomes a **host session**: the owner sends `Hosts`, the
`Ledger`, then a full copy of the space (`Update` per source and per doc, from the vault —
one flush behind a live doc at most), and from then on **every** accepted update on the space
is relayed to it, subscribed or not (`Pusher::broadcast`, once per fan-out, to the host
sessions except the one it came in on). The uplink is registered in the headless host's own
session table as a host session, so what it accepts from members goes up the same way, and
what comes down is merged as the owner's (the link named that device; nothing to judge). The
owner dials nobody; two appointed hosts do not talk to each other yet (they converge through
the owner) — §6.

**Relay only what was new.** A relayed update is a full snapshot; a host that merged nothing
(its version vector did not move) acks and stops. That is what makes owner → host → owner
loops end, with any number of hosts.

**Members learn the hosts** from `Frame::Hosts { hosts: [(did, addr)] }` — owner first — sent
after every `Welcome` and again when the list changes; `Fetched.hosts` carries the same
entries at join. `Joined.hosts` keeps them (`joined/<ws>`). A host announces where it can be
dialled with `Frame::HostHere { addr }` after its `Welcome`; the owner keeps that under
`hostaddr/<ws>/<did>` and lists it only while the DID holds `host`.

**Failover.** When a session drops, the member dials the **next** host in the list at once
(no pause); a pause from the retry ladder and the `host offline:` console line come only
after a whole round failed. With one host this is exactly today's behaviour. `Hosts` over
the bridge lists every known host of a joined space, `online` for the one the live session
is on.

**The acceptor moves to `kunki::host`** (from `shell2::host`), generic over a `Sink` the
process supplies: how an update is merged (`shell2`: `Msg::HostSync` to the UI thread, into
the live doc; headless: `Admin::accept_sync` inline), the clock (`shell2::clock` vs the
system's), the admin for a space (own vs `hosted`), the hosts to advertise, and the
in-process listener to poke. `admit`, the guard, `Sessions`, `fetch` and the session loop
are shared code with tests in one place.

**Not in step 7:** issuing invites or role changes while the owner is offline (needs a
delegated `host` with `delegable` and a code store on the host; §6), host↔host sessions,
dismissing a host (revoking the `host` token already cuts it off at the next judgement).

*Revised 2026-11-02 (step 7, as built):*
- **A hosted admin signs nothing.** `Admin::hosted` answers every `with_signer` path with
  `NotOwner`: a host that minted with its own key would record tokens as the owner's that
  fail every chain check — a refusal beats silent garbage.
- **Replay skips an issue already on record** rather than re-putting it, so the owner's own
  `Cause::Under` lineage survives a replay of its own ledger; revocations are re-put.
- **One merge at a time on the headless host.** `accept_sync`, `accept_trusted` and `replay`
  run on `spawn_blocking` behind one mutex: the shell gets one writer per doc from its UI
  thread, a tokio process has to say so.
- **`HostHere` is sent twice**: after the first `Welcome` (dropped by the owner while the DID
  is a plain member) and again when a replayed ledger first names this DID a host. A restart
  seeds that flag from the hosted admin so it is not sent a third time.
- **`Sessions::send` skips host sessions**: every fan-out ends in one `broadcast`, so a host
  that also subscribed would otherwise hear each update twice.
- **Failover dials the next host inline**, from the UI thread, with no pause; only a paced
  retry goes through the sleeper thread. A list shrunk by a `Hosts` frame wraps instead of
  indexing past its end, and `online` over the bridge matches the dialled host by DID, since
  a `Hosts` frame can reorder the list under a live session.
- H7 waits `PUSH × 4` for the member's post to cross: a dead owner is noticed by QUIC's
  10 s idle, then the next host is dialled at once.
- *(2026-11-02, step 7 review)* **`host` is never an invite role**: `issue_invite_ticket`
  refuses it at every scope — it is in no capability table, so the capability check let it
  through, and any member allowed to invite could have handed an accomplice a whole space.
  Appointment is `Admin::appoint_host` alone; an app's own role named `host` stays an app role.
- *(2026-11-02, step 7 review)* **A host session is re-judged before every broadcast and
  ledger send** and demoted when its appointment was revoked or ran out — a member session
  again until re-promoted. The uplink is exempt: the owner's own session.
- *(2026-11-02, step 7 review)* **A presented token is verified at the door**: `admit` walks
  its chain to the owner against the revoked set and requires its scope to reach the space
  (a narrower `App` grant or the `Node` owner grant both do; another space's does not) — a
  forged or stale token used to reach `Welcome` and hear `Hosts` before its first doc.
- *(2026-11-02, step 7 review)* **A ledger frame is all or nothing** (`Admin::replay_all`):
  every entry verifies before any is applied; one bad entry ends the uplink session and
  the host keeps serving from the ledger it had.
- *(2026-11-02, step 7 review, shell)* promoting a session on `AppointHost` runs on a
  worker (it signs the ledger and reads every doc); a lock or account change answers every
  pending join and role request rather than replaying them on the next account's session;
  the failover counts hosts *tried* since the last session was up, so a drop after a
  session on the last host tries the owner at once.

### Mobile (step 8, as built 2026-11-03)

A phone is a member like any other: it dials the link's host over iroh from its own device
key, in `Auto` mode (a phone has no environment to set), so it reaches a laptop in `local`
mode by the LAN address the link carries and a laptop in `auto` mode through the relay.

- **The join box takes the link.** The Spaces screen's `join` box, which took a node ticket
  or an invite for the socket route, now routes by prefix: `osvauld://s/…` goes to the shell's
  own `JoinSpace` (`Msg::JoinLink` → `join_typed`, the bridge's path with the reply carried back
  to the screen as `SpaceScreenMsg::JoinResult`); anything else goes where it went. On Android
  the node route is gone — `node::connect` refuses, the box says "type a share link" — and the
  ADB-reversed `files/kunki.sock` with it; the debug bridge at `files/shell.sock` stays, root
  emulator only. A physical phone has no bridge: H8 is a person at the Pixel (or `adb shell
  input text`, which is typing), the laptop half scripted.
- **The APK asks `INTERNET` and `ACCESS_NETWORK_STATE`** (it asked nothing before: iroh's
  sockets and interface watch need them). Debug builds pipe stdout and stderr to logcat under
  tag `osvauld`, so `net:`/`kunki:` lines and panics can be read; an APK's own stderr goes
  nowhere otherwise.
- **The tree build grows its stack** (`app_host::walk` under `stacker::maybe_grow`, 128 KiB
  red zone, 8 MiB segments). Found here: the launcher's tree overflowed the 2 MiB main thread
  on the Pixel under twenty levels deep — the fault address was the stack pointer — in a debug
  build whose `build` frames are large. The same thread size is every app thread's on desktop.
- **iOS**: no code of its own. `net` is an unconditional dependency of shell2 and the iOS
  build (`--no-default-features`) carries it; not compiled here (Linux, no mac peer online).
- Not here: paste on the phone (the runtime has no clipboard yet; step 9's copy/paste UI),
  a QR, an xnet name that resolves to a link (`xnet-names.md`).

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
   *Detailed 2026-11-02 (§2 "Appointed hosts"), in four parts:*
   - 7a `courier`: `ledger` (signed entries), `role::HOST`, `Frame::{Ledger, Hosts, HostHere}`,
     `HostEntry` in `Fetched`, `node_accept_sync`/`accept_into` report whether the merge moved
     the version vector.
   - 7b `kunki::admin`: `Admin::hosted(vault, owner)` with the `hosted/<owner>/` prefix and
     root = owner; `replay`, `signed_ledger(ws)`, `appoint_host`, `is_host`/`hosts_of`;
     `judge` is `OPEN` for a host; `accept_trusted` for the uplink; `Pusher::broadcast`; no
     fan-out when nothing was new.
   - 7c `kunki::host` (the acceptor lifted from `shell2::host` behind `Sink`), host sessions,
     the owner's side in shell2 (`AppointHost`, `Hosts`, ledger pushes), the member's failover.
   - 7d the binary: `kunki::headless` (join, claim, uplink, replay, serve) and the `host`
     subcommand. H7.
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
  *2026-11-02:* step 7 settles the first half — hosts hold the owner's signed ledger (§2
  "Appointed hosts") — and leaves issuance with the owner. A delegated `host` token with
  `delegable` plus the ledger replicated *up* from the host is the shape for the rest.
- **Open (new):** appointed hosts dial only the owner; two of them converge through it. A
  host↔host session (lower DID dials) is the obvious next piece when a space has two.
- **Open:** whether a member device may also be dialled by other members for the ephemeral
  channel (presence) without being a host.

## 7. Hazards this closes

From `status.md` §"Workspace permissions, sync, and sovereign node": caller identity now bound by
transport (step 5); async completions scoped to an account generation (step 5); `join_item`
overwrite (step 6); bridge frame cap (step 3). Still open after this plan: write deadlines on the
agent bridge.
