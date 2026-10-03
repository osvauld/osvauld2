# Status — what's built, what's next

Current truth for progress. Extracted from the archived week notes (`archive/w1–w3.md`,
`archive/runtime-3month-plan.md`) and **verified against the code** — where the two
disagree, code wins and this file records that. Update this file as things land; leave the
archive alone.

Plan of record for the *unbuilt* milestones: `design/runtime-rebuild-plan.md` §4 (M2),
§5 (M3), §6 (M4).

## Built

### M0 — runtime plumbing (done, W1)
- ids + keyed state store (`get_or`, register-by-frame, sweep)
- scroll containers with hit-testing that respects clip
- drag gesture: phases + pointer capture; current experimental callback separates screen ghost
  position, content-space movement delta, and captured visual scale
- overlay layer: root-painted fixed-size portals, transformed element anchors, catcher + click-away
  dismiss, flip/clamp; strict Lua `ui.overlay` combinator
- animation: external wakeup (`EventLoopProxy`), timer wheel, retained transitions, and
  id-keyed spring press-scale for click feedback (Lua prop included)
- experimental zoomable runtime viewports: retained per-id Ctrl+wheel zoom, free pan, stable
  child layout, and Lua `zoomable`. **Geometry rebuild in progress 2026-09-11:** paint and the hit
  resolver interpret the same explicit clip/transform boundaries; editor and drag/drop map through
  the inverse; nested wheel scroll chains into camera pan; transformed scrollbars share
  paint/hit/drag geometry. Euclid-backed screen/viewport/content/node units type the camera and
  Geometry's screen/content rectangles, with Kurbo erasure at rendering boundaries. Growing
  scrollers now use remaining main-axis space and kanban's definite board height prevents card
  content from enlarging every column. Lua portal overlays and transformed element anchors are
  built, including stable anchors that ignore transient press-scale (live-verified). Gesture
  occlusion and current-geometry capture are explicitly deferred while Frame planning proceeds;
  neither is implemented. Final event types and motion also remain before product-ready use. See
  [viewport geometry rebuild](design/viewport-geometry-rebuild.md).
- Field/Focus (click + autofocus), single-style text, `PlainEditor` island
- rich runs exist **runtime-side** (`El::rich` + `text::Run`) — not yet a Lua prop (W8's
  rich-text leaf is where it gets exposed)

### M1 — Lua app layer (W2–W3, one tail missing — see below)
- **`app_host`**: sandboxed Luau VM (`sandbox(true)`, interrupt budget, source-only so
  tracebacks keep line numbers); `ui.*` walk → `El<LuaMsg>` (no parallel node type);
  props registry where an unknown prop is an error, not a warning; `ui.state` + sweep;
  error boundaries in `walk` (dev-gated breadcrumbs); staged `reload` (whole second VM,
  keeps doc cores + per-viewer scratch, trial frame, banner over the still-running app on
  failure)
- **error cards**: host-minted errors omit mlua's `runtime error:` plumbing; breadcrumbs use
  one separator; missing bind/input props name what is required; unknown tags enter the
  recorded boundary while siblings stay alive; diagnostic key dumps are deterministic
- **doc binding**: `doc:open(name)` mirror — plain-table reads patched in place at the top
  of `view()`, explicit writes (`:set/:insert/:delete/:move`), stable-id addressing,
  snapshot persistence, wake-on-external-write
- **multi-file `require`** over the app's own source doc, with a module cache
- **`shell2`**: real `vault` accounts (signup / unlock / mnemonic-once, Argon2 off-thread),
  workspaces + items screens, app upload (folder picker → `files` LoroMap → `main.lua`
  entry point), tabs (one running instance per item, retained ids namespaced `tab:<id>`),
  theme
- **`lua_tree`**: full-moon/Luau parse → 22-kind schema → printer; round-trip and
  strong-spike suites (printed source runs and produces an identical element tree); the
  every-table-constructor-on-its-own-line printer rule (two rules, pinned by test)
- **kanban** (`demo_apps/kanban/`, moved there 2026-09-20 from `shell2/src/kanban/`): the reference app — typed drags (card/col
  sharing one `on_drop`), cross-column moves through the doc, resizable columns with
  clamped bounds, floating ghost outside every scroll clip, always-reserved drop guides.
  `demo_apps/tally` and `demo_apps/scratch` are the small examples.

### Voice POC (`voice/`, 2026-10-01)
- standalone binary, not wired into the shell: mic (`cpal`) → capture cleaning → Opus 20 ms
  mono 48 kHz → iroh QUIC datagram `[seq u32][level u8][opus]` → Opus → playout buffer
  (≤100 ms, PLC for ≤3 lost frames) → speaker. `voice listen` / `voice dial`;
  `scripts/voice_pair.sh` runs two peers in tmux on one machine. Connection verified locally;
  audio judged by ear only.
- `--clean none|apm|rnn|rnn-gate` (default `apm` = WebRTC's audio processing module, bundled
  build): echo reference fed from the speaker callback. `rnn` (RNNoise) removes most noise but
  scores worst on speech damage; the VAD gate barely closes on room noise or key clicks.
- `voice process in.wav out.wav` runs cleaning + Opus round-trip offline;
  `scripts/voice_eval.py` scores modes with PESQ/STOI/DNSMOS. Its reference must be studio-clean
  speech — a laptop-mic take skews PESQ/STOI against denoisers.
- `voice dial <ticket>` (iroh `EndpointTicket`, carries the relay URL when online); ctrl-c
  closes the connection so the peer ends at once. `--delay <secs>` holds playback back
  (headphones-only self-test).
- not built: node relay (forward by `level` without decoding),
  DeepFilterNet, courier auth on the connection. `iroh-roq` was dropped: abandoned upstream,
  pinned to iroh 0.35.
- next: split `voice` into a lib + thin test CLI; shell2 owns the device's one iroh endpoint
  (courier + voice ALPNs) and exposes calls to Lua behind a mic capability; the relay lives in
  `kunki`, as its first iroh transport.

### Search (`search/`, 2026-10-03) — design: `design/search.md`
- `search` crate: one tantivy index per item, every file a sealed vault entry under
  `entry/search/<ws>/<item>/` (`VaultDirectory`); facets (`author:anu`), title boost,
  `rank = "recent"`, plain-text snippets. Held unsealed in memory while open.
- `app_host::index`: an app's `index.lua` (`doc`, `each`, `key`, `fields`, `rank`) runs in its own
  sandboxed VM over frozen copies; changes found by per-record fingerprint, so only changed
  records re-run `fields`, and a restart re-runs none.
- shell2 (`indexer.rs`) indexes after every flush and catches up on open, including docs the app
  never opened (`Vault::doc_names`); test tabs get an in-memory index. `search.query(text,
  {limit})` in Lua, bridge `Search`, `t.type` in app tests. `demo_apps/chat` is the reference.
- verified: `cargo test -p search`, `-p app_host index::`, `-p shell2 indexer::`,
  `scripts/smoke_search.py` (S1–S6). `cargo test --workspace` and the other smokes not re-run.
- not built: the default layer for apps without `index.lua` (plan step 6), workspace-wide
  search, indexing off the UI thread, a `lua-apps.md` section.

## Not built

### Workspace permissions, sync, and sovereign node — design baseline

**2026-09-15:** `kunki` exists as the first sovereign-node slice: it creates or loads a
passphrase-sealed node identity, uses the identity device public key as the transport node id,
and prints a base64url JSON connection ticket carrying node public material plus a node-signed
bootstrap claim token. The follow-up `courier` slice adds the transport-free protocol core for
claim and reconnect: desktop relationship permits, node admin permits, first-admin bootstrap
rejection once an admin exists, node-issued reconnect challenges, desktop-signed reconnect proofs,
and replay rejection are covered by automated tests (branch `kunki-initial`). Tests pass message
structs directly between node and desktop functions, and the happy path round-trips every message
through bincode, so no transport is needed to exercise the protocol. **2026-09-17:** role-token,
signed-update authorship, and Lua-rule decisions recorded in
[`design/workspace-permissions-sync.md`](design/workspace-permissions-sync.md) §4, extended **2026-09-18** with the role/capability/rule split, structure-versus-data writes,
namespaces owning schema and rules, install as the authorization event, and the per-user index.
Built: `courier::token` issues root role tokens, delegates them with the full parent embedded,
and verifies a chain leaf-to-root — depth cap before any crypto, per-link signature/subject/
expiry/revocation, then child-versus-parent linkage, delegability, role, and scope; ids hash the
signed payload. Scope is four levels (node, workspace, app, resource) and
`workspace::ResourceScope::contains` decides the innermost one. **2026-09-19:** authorship
became a signed field on the record rather than a signed update wrapper, so there is no node
update log and Loro peer ids need no DID binding; kunki will store through `vault` as shell2
does, with the admin store on `Vault::store`. Also built: `courier::policy` — platform
capabilities as a closed Rust set, the `(scope level, role) -> capabilities` table pinned cell
by cell, and `authorize` joining the chain check to scope coverage and capability. A role read
one level down is a different role, so narrowing a token to app scope drops platform
capabilities by design. **Gate 1 (the node remembers), in progress:** `identity::Signer` is the outside view of an
identity — DID, signature, the two public keys — so `courier` takes `&(impl Signer + ?Sized)`
and never holds a private key. `vault` gained sealed `entry/` records (opaque, caller-keyed,
reserved namespace so no name can address the keystore) and `with_signer`, which lends a
signer for a closure and yields nothing when locked. `kunki` now keeps its identity as a vault
account instead of its own `identity.bin`, created on first boot and unlocked from
`OSVAULD_KUNKI_PASSPHRASE`; a second account in the node directory stops the boot rather than
guessing which is the node. `kunki::admin` is the node's own record over those entries: an
issue keyed by token id with an empty `users/<did>/tokens/<id>` marker indexing it, and a
`revoked/` set read whole into the chain check — everything at top level being *this* node's
authority, with `nodes/<node-did>/` reserved for the mirror image a federated peer or a desktop
needs. Each issue carries a `Cause` — the node's own decision, or
`Under(parent id)` — which is the lineage a flattened node-signed token no longer carries in
`prf`, and so the only thing a cascade can follow. Revocation accepts ids the node never
issued, because delegations are minted between holders.

**Priority moved to sync, 2026-09-21.** Gate 1 stops here: revocation cascade and `role.assign`
are deferred with their reasoning intact in
[`design/node-backlog.md`](design/node-backlog.md), which is the running note for everything
set aside as we go. The node now remembers enough to be worth syncing, and the rest of the
authorization surface is worth building against a working wire rather than ahead of one. The
route to first sync stays transport-free — courier's handlers are pure message transitions, so
publish and sync extend that shape and an iroh adapter slots in at the end rather than being
designed around. **2026-09-22:** the claim survives a restart. `Admin::admins` rebuilds
courier's admin list from `users/<did>/relationship`, and `accept_claim`/`accept_reconnect`
supply it and persist what courier adds — before this the list was an in-memory `Vec`, so a
reboot handed a claimed node to whoever claimed it next. The ticket's text form moved into
courier as `ConnectionTicket::to_text`/`from_text` — base64url of JSON behind an `osv1.` prefix,
so a ticket from a version this build does not understand is refused by name instead of read as
an older one — and `kunki/tests/printed_ticket.rs` runs the binary, parses what it actually
printed, and claims with it. `kunki::peer` closes the other side of that asymmetry: until now
only the node kept its half, while `desktop_finish_claim`'s record was dropped by every caller,
so a claimant restarting could not name the node it had claimed. It is stored at
`nodes/<node-did>/relationship` and is enough to reconnect from. It is the mirror of `admin`
rather than the same shape — `admin` is what this account issued and appends, `peer` is what it
holds and replaces — and it lives in `kunki` because a node federating with another node holds
permits exactly as a desktop does. `vault::adopt_workspace` takes a workspace that originated
in another account keeping its id and creation time, since publishing requires both ends to
name one workspace; the id is validated against the shape `new_id` mints, because `ws/<id>/meta`
with a slashed id is a legal key addressing something else under `ws/`. The claim now hands out
a `token::Token` rather than a permit — the permit had no `exp` field and no id, so the
credential a node issued was valid forever and could not be revoked. It is `owner` at node
scope, delegable so a publisher can narrow it to a workspace for their own node, lives 30 days,
and is reissued on reconnect, where expiry and the revoked set are both checked on the way
through. `kunki::admin` records it with `Cause::Node`, which gives that variant its first real
caller. Still absent: any transport, any workspace on the node, any sync protocol, and a
desktop UI claim handler. The claimant's half went the same way: `ClaimHello.attestation` is a
token rooted at the claimant rather than the node — `verify_chain` always took the expected
root as a parameter, so no second verification path was needed — carrying its key material in
`Claims.binds` under a role no capability-table entry matches, so a statement about keys can
never be read as authority. `PermitClaim`, `issue_permit` and `verify_permit` are gone, and
courier now has one credential mechanism rather than two, which is also the shape osvauld1 had.

**2026-09-23: publish lands.** `courier::publish` is the wire shape for a desktop announcing a
workspace (and a flat item list) to a node it has already claimed: `PublishHello` carries the
desktop's own DID alongside its existing node-scope claim/reconnect token — un-narrowed, on
purpose. First cut narrowed the token to `Scope::Workspace(id)` before checking it, which is
wrong: `WorkspaceCreate` is a platform capability `courier::policy` only ever grants at
`Scope::Node` for `owner`/`admin`, so a token already narrowed to workspace scope can never carry
it, no matter whose it is. `node_accept_publish` calls `policy::authorize` straight against the
held node-scope token with `Capability::WorkspaceCreate` and target `Scope::Workspace(hello.workspace.id)`
— one call proves the chain, the un-narrowed scope's coverage, the capability, and (since
`verify_chain` checks `holder == leaf.aud`) that the caller is who `desktop_did` claims. A
regression test (`a_token_already_narrowed_to_the_workspace_cannot_create_it`) pins the mistake
so it cannot come back. `known_items` in the `PublishAck` is read before the check runs, so the
ack reflects what the node held *before* this call. `vault::adopt_item` joins the pre-existing
`adopt_workspace` as the item-level counterpart, validating `id` and `ws_id` against the same
`is_minted_id` shape both share. `kunki::admin::Admin::accept_publish` wires the two together:
verify first, write second, and an item `kind` string this build cannot parse (`ItemKind::parse`)
fails closed before that item is adopted — earlier items in the same call stay adopted, which
matches every other non-transactional write in this module and self-heals on republish.
`kunki::bridge` is new: the node's own UDS bridge (`$OSVAULD_KUNKI_SOCKET`, default
`/tmp/osvauld-kunki.sock`), same one-connection-one-request-one-reply shape as shell2's, kept
single-threaded so `vault`'s single-handle rule needs no second mechanism. `Request::Ping` is
the liveness probe; `Request::Publish(PublishHello)` is the only real verb so far, dispatching to
`Admin::accept_publish`. Time is the real wall clock (`now_secs`) rather than virtual — nothing
dispatched yet carries a TTL worth fast-forwarding past in a test — noted in-code as the seam a
`Frame`/`Advance` pair replaces once claim/reconnect grow bridge verbs of their own. Tests drive
`handle` over a real `UnixStream::pair()` against one shared vault clone, so the wire framing and
`#[serde(tag = "op")]` dispatch are exercised, not only `accept_publish` in isolation. Still
absent: a claim/reconnect bridge verb (tests still claim in-process), sync of the workspace's
actual content (publish is headers-only by design), and the invite/role-assign path that lets a
second user reach what got published. A fresh review against `expert-secure-core` found no
blockers and traced authorization, holder, and revocation checks against the actual code paths
rather than the tests alone; it closed two gaps, both now landed: `revoking_a_claimant_stops_it_publishing`
proves a revoked claim is refused end-to-end (previously only exercised with a synthetic set at
the courier layer), and `adopt_item`'s locked-vault guard is now asserted alongside
`adopt_workspace`'s.

**2026-09-23: invite lands.** `courier::invite` is a second, deliberately separate ticket type
alongside bootstrap's `ConnectionTicket` rather than a field added to it — bootstrap's fields
(fixed capability, no role/scope) would be dead weight on an invite and vice versa, so this
duplicates the shape rather than overloading it, matching the "duplicate now, unify later"
stance already recorded in [`design/node-backlog.md`](design/node-backlog.md). Its own signed-blob
domain and its own text prefix (`osvi1.`, versus bootstrap's `osv1.`) keep the two kinds
unambiguous on sight. `issue_invite_ticket` checks the inviter's held token for
`Capability::MemberInvite` over the requested scope exactly as `node_accept_publish` checks
`WorkspaceCreate`, then adds one restriction with no precedent to lean on: `role.assign`'s rank
check (an assigner cannot mint above its own role) is designed but not built, so rather than
guess at an ordering, an invite may only name a role that itself carries zero platform
capability. New error: `RoleNotInvitable`. Redemption (`desktop_start_invite_claim`/
`node_accept_invite`) mints the same token shape bootstrap does, but its replay guard cannot be
reconnect's in-memory challenge set — an invite ticket has to still be good after a node restart
between minting and redemption — so `redeemed` is caller-supplied read-only input exactly like
`revoked` elsewhere, and the newly spent nonce comes back as `InviteWelcome::redeemed_nonce` for
the caller to persist. `kunki::admin` supplies that persistence: `Admin::issue_invite`/
`accept_invite` wire the courier calls to a new `invites/<nonce>` store namespace (empty-valued,
same shape as `revoked/<id>`), writing the spent nonce before recording the new token so a crash
between the two leaves the invite merely unusable rather than redeemable twice. `kunki::bridge`
gained `Request::Invite`/`Request::ClaimInvite`. A fresh review against `expert-secure-core`
found the capability-free-role check as first written was **not** the safe restriction it was
believed to be: it tested `platform_capabilities(role, scope)` only at the literal requested
scope, but `token::delegate` lets any holder narrow scope client-side while keeping `role`
unchanged, and `"maintainer"` is empty at `Scope::Node` while fully powered at
`Scope::Workspace(_)` — so an invite for `"maintainer"` at node scope passed the guard, and the
redeemer could then self-delegate that token down into any workspace and recover full
`MemberInvite`/`RoleAssign`/`AppInstall` power there, the exact escalation the guard existed to
block. Fixed: `role_could_gain_capability` also checks workspace scope whenever the request is
at node scope — the only lookahead needed, since a workspace can only narrow further into
app/resource scope, which the table never populates — and `verify_invite_ticket` gained the same
`claim.iss == ticket.node_did` belt-and-braces check bootstrap's `verify_ticket` already had. Two
regression tests pin the fixed escalation (one at the courier layer, one through
`kunki::admin`), and a new locked-vault test covers `issue_invite`/`accept_invite` alongside the
existing claim/publish ones. 17 new tests across the two crates, including one that drops and
reopens the node mid-test to prove the spent nonce survives a restart — the same proof
`a_claimed_node_is_still_claimed_after_a_restart` already established for the admin list. Still
absent: any UI or transport carrying an invite ticket to a second desktop (today's tests hand the
ticket to the redeemer directly, same as every other courier flow so far), and `role.assign`
itself, which is what would let this restriction be lifted.

**2026-09-24: sync, subscribe, and push land.** `courier::sync` is a desktop pushing local
Loro changes for one item's layer to its home node and learning back what it doesn't have, in
one round trip: `desktop_start_sync` commits pending edits and exports only what changed since
a caller-supplied version vector (`None` the first time a layer syncs, which pushes full
history); `node_accept_sync` authorizes by workspace membership alone — no platform capability,
same reasoning `policy::membership` already gives sync's module doc: every role in a workspace
may read and write its content — merges into the node's current snapshot (`None` the first
time), and diffs back only what the desktop's own `vv` doesn't cover. Merge only, never
replace: the node's copy is authoritative by construction, so unlike osvauld1
(`design/osvauld1-prior-art.md` §8) there is no destructive "divergence" fallback to reach for.
`kunki::admin::accept_sync` wires this to `vault::get_src`/`put_src`/`get_doc`/`put_doc` keyed
by `SyncLayer::Src`/`Doc(name)`; `kunki::bridge` gained `Request::Sync`.

`courier::subscribe` is a desktop declaring interest in an item's layer — explicit by design (a
Subscribe/Unsubscribe verb, never implied by sync history), same membership-only authorization
as sync, courier stores nothing. `kunki::admin::subscribe`/`unsubscribe`/`subscribers_for`
persist it under `subscriptions/<ws_id>/<item_id-b64>/<layer-b64>/<did>`; `item_id` and the
layer tag are base64'd before becoming key segments so a crafted `item_id` containing `/`
cannot alias a different subscription's key (pinned by test — see the review paragraph below).
`kunki::bridge` gained `Request::Subscribe`/`Request::Unsubscribe`.

`kunki::push::Pusher` is the delivery trait (`push(subscriber_did, &Push)`, fire-and-forget, no
retry — a lost push is caught by the subscriber's own next sync, the same correction path a
missed message already had, not a new one). `Admin::accept_sync` now fans the new snapshot out
to every other subscriber on that layer once it's stored — a full snapshot, not a delta,
because no per-subscriber version vector is tracked, so there is nothing to advance on send
rather than confirmed delivery, the exact osvauld1 hazard `sync.rs`'s own module doc already
names. Threaded through `bridge::dispatch`/`handle`/`serve_forever` down to `main.rs`, which
runs `NoopPusher` — no real transport exists yet, so fan-out computes and costs a
`subscribers_for` lookup per sync but delivers nowhere; swapping in an iroh-backed `Pusher` is
the whole remaining migration. `two_desktops_converge_through_a_push_neither_one_pulled_for`
(`kunki/src/admin/tests.rs`) is the two-desktop-one-node proof this was built for: alice claims
the node, bob joins by invite and subscribes, alice syncs an edit, and bob's doc converges
having never called sync himself — `MockPusher` stands in for the transport, the same role
osvauld1's own `MockConnection` played for its `Coordinator<C: Connection>`.

A fresh review against `expert-secure-core` (via `pi -p`) found two gaps. Fixed:
`vault::get_src`/`put_src`/`get_doc`/`put_doc` now check `workspace::is_minted_id` on both
`ws_id` and `item_id` before building a key, the same check `adopt_item` already applies —
without it, `item_id = "X/doc"` at `SyncLayer::Src` and `item_id = "X"` at `SyncLayer::Doc("src")`
aliased the same key, an id from a remote sync used to reach a different item's content.
Still open, not yet fixed or formally deferred: `node_accept_sync` always exports a full
snapshot even for an empty-push pull, and `kunki::bridge::serve_forever` is a single-threaded
accept loop, so a flood of sync requests that are cheap to send but expensive for the node to
answer, against a large layer, could stall all other bridge traffic; this fits the stated
threat model (ordinary workspace membership, no elevated privilege needed).

**2026-09-24: real push replaces the placeholder, a shell2 UI exists, and two real bugs
surfaced by using it are fixed.** Supersedes this same date's earlier claim that "swapping in
an iroh-backed `Pusher` is the whole remaining migration" — that undersold it.
`kunki::push::LiveRegistry` is the real `Pusher`: one bounded channel per currently-connected
desktop (`try_send`, so a full or abandoned receiver can never block the accept loop that calls
it), registered by a new `Request::Listen{desktop_did, token}` verb that — unlike every other
request — doesn't get one reply and close. `kunki::bridge::serve`'s accept loop hands a `Listen`
to its own thread, authorizes it (`courier::subscribe::node_accept_listen`, the same
`policy::membership` check as everything else, at `Scope::Node` since one connection carries
every workspace's subscriptions), then relays whatever `fan_out` registers against that did
until the connection drops. Every other request stays on the existing sequential path.
`kunki/src/main.rs` boots with `LiveRegistry::new()`, not `NoopPusher`.

On the desktop side, `shell2::node` gained the client half (`subscribe`/`unsubscribe`/
`listen`/`next_push`), and `shell2/src/main.rs` gained `spawn_push_listener`, which holds one
`Listen` connection open per claimed relationship — started at boot if a relationship was
already persisted, on a successful claim, and refreshed on every unlock. `Msg::PushReceived`
imports straight into the matching open doc. `SyncTick`'s own poll is now a 20s reconciliation
backstop, not the delivery path: subscribing happens the first time a doc is seen open, in the
same post-flush pass that already runs after every message (not the slow tick, so a freshly
opened doc doesn't wait on the backstop to start receiving pushes), and a local edit reaches
the node immediately too — the same post-flush dirty-check that already drove persistence now
also drives `sync_doc`, a small helper `SyncTick` and the immediate path both call so the two
don't drift apart.

shell2 also gained real UI for all of this: `SpaceScreen` has "join a node" (accepts either a
boot ticket or an invite, told apart by their text prefix), "invite" (mints one and prints it —
no clipboard support anywhere in this codebase, the same reason kunki's own boot ticket is
already meant to be copy-pasted from a terminal), and "publish" widgets. New `osvauld-rpc`
verbs (`ClaimNode`/`Invite`/`PublishAll`/`JoinItem`/`PushSrc`) expose the same actions to
automation, since shell chrome isn't reachable through the existing Click/Type/Key senses —
those are scoped to a running app's own element tree, not the shell around it.
`scripts/demo_sync.py` drives two desktops and one node through the whole thing — signup,
claim, invite, publish, join, a note typed on each side to prove the other receives it — with
no manual clicking, in `scripts/osvauld/client.py`'s existing style. Fixed along the way: a
pre-existing, unrelated bug where `Mnemonic`'s "Continue" sent a fresh account back to
`Screen::Signup` instead of into the app.

Two real bugs surfaced by actually running that demo, both fixed:
- `JoinItem` only ever pulled the *source* layer (`courier::publish`'s "headers only, content
  follows over sync" holds, but nothing pulled the *doc* layer either) — so a joining desktop's
  app saw a genuinely empty doc and re-ran its own first-run seeding logic, and the CRDT union
  of both desktops' independent seeds showed up as literal duplicate content. Fixed with
  `resolver_with_node` (`shell2/src/main.rs`), wrapping the existing `resolver`: if nothing's
  stored locally and a node is claimed, `node::pull_doc` (`shell2/src/node.rs`) tries the node
  first — checked against the doc's own `oplog_vv().is_empty()` after import, not the byte
  length of what came back over the wire, since an empty diff is not reliably zero bytes in
  Loro's own encoding.
- A joining desktop with zero workspaces of its own got stuck on `SpaceScreen`'s empty
  "press ⏎ to create" branch even after `JoinItem` had already adopted one — the same class of
  bug `refresh_after_workspace`'s own doc comment already names for `CreateWorkspace`/
  `CreateItem`, just never wired up for this verb. One missing call, now added.

Also investigated: a demo run that appeared to hang for 10–30s per step, wildly variable
between runs. Root-caused with wall-clock instrumentation (still in the source as
`DBG timing:` prints, `shell2/src/main.rs`) — not fixed, because there was nothing in this
codebase to fix: every actual network/local-processing measurement was 0–15ms, matching
headless exactly; the entire delay was in getting winit's event loop to process an
already-delivered `send_event`. Traced to the window manager (a tabbed/stacked layout) giving
Wayland frame callbacks only to the currently-visible tab of a stack — a backgrounded shell2
window's event loop stalls waiting on a callback the compositor is not sending it, which is the
compositor behaving correctly (it does not drive invisible surfaces), not a bug in `runtime` or
anything built here. Confirmed directly: the same run drops to milliseconds once both windows
are actually visible. Documented as a caveat in `scripts/demo_sync.py`'s own docstring rather
than "fixed."

Still open, carried over from the review above, unchanged by any of this: the caller-identity
gap (`desktop_did` is request-supplied, not proven by a signature — a delegated token's
embedded parent can be replayed to impersonate its own issuer) now also applies to `Listen`;
async worker completions (`Msg::NodeRpcDone`, `Msg::SyncDone`, `Msg::PushReceived`) are not
scoped to an account generation, so a stale reply from before a lock/unlock or account switch
could still write into the wrong account; `join_item` retried after local edits can still
overwrite them (always starts from an empty doc); the bridge still has no per-connection
frame-size cap or write deadline.

**2026-10-03: Xnet names land** ([`design/xnet-names.md`](design/xnet-names.md)). A bare name
typed where a ticket goes resolves on Aptos to a public invite and is redeemed like a pasted
one. `courier::invite` gains a signed `public` flag: a public invite is reusable and revoked by
putting its nonce in the same spent set single-use invites use; pre-flag tickets decode as
single-use. `xnet_names/` is the Move registry (`register`/`update`/`transfer`, free `resolve`
view), 8 Move tests, live on devnet — measured `register` 0.0064 APT, `update` 0.000085 APT.
`shell2::names::resolve` is one `POST /v1/view`; `node::join` is now the single
ticket/invite/name dispatch for the claim box and `ClaimNode`. Verified by `cargo test` plus an
`--ignored` test against live devnet; the UI path and `smoke.py` were **not** run. Limits: a
join reaches only a node on the same machine (shell2 talks to kunki over its local socket;
nothing dials iroh), and names can only be registered with the `aptos` CLI today. Next:
registration from Sthalam with the identity key, a username namespace, a kunki rate limit on
public redemptions.

**2026-09-11:** [`design/workspace-permissions-sync.md`](design/workspace-permissions-sync.md)
records the agreed direction and open decisions for a fresh implementation. **First slice
landed 2026-09-11:** the new `workspace` crate validates bounded workspace-address syntax
and callable index handles, with ambiguous-input rejection tests; `ResourceBinding` is an
in-memory handle/target pair. Semantic opaque IDs and CRDT index resolution remain unbuilt. **Second slice landed
2026-09-11:** exact and terminal-`/*` subtree scopes match validated segment boundaries;
lookalike prefixes, the subtree base, other workspaces, and non-terminal/recursive wildcards
are excluded by tests. Everything below remains unbuilt: workspace namespaces shared across
apps, capability permits bundled with recipient-encrypted keys,
bounded node issuance by roles/DIDs, permit upgrades over sync, CRDT discovery indexes,
local-only data, sharding, and document-based submission/results. The old `osvauld` and
`agent_x` are research references, not compatibility contracts. The next implementation
slice hardens existing Vault identifier validation before adding typed resource storage;
Vault remains an opaque sealed store rather than an authorization engine. The broader design
checkpoint is the shop's namespace/capability/processing table, challenged against
booking and chat; exact rules, grant/key formats, index hierarchy, and backend ownership
remain to be designed. Work packages and acceptance scenarios are in the design note.

### Frame — Lua-programmable visuals

**Implementation in progress since 2026-09-11:** the
[Frame implementation plan](design/frame-implementation-plan.md) records the 2D capability roadmap,
Lua/resource/geometry contracts and acceptance gates. Frame is now a shipped but incomplete 2D
visual resource; the landed slices are listed below. Arcs, radial/sweep brushes, shaped Frame text,
internal clips, identity/hits, dynamic buffers and export remain unbuilt. Safe public mlua buffer
extraction still copies rather than providing a claimed zero-copy slice. The broader retained 3D
world is intentionally not being folded into Frame; see the new
[Environment runtime plan](design/environment-runtime.md). **First implementation slice landed:** runtime exposes an immutable
validated cubic-Bézier `Path`, true local bounds and command count; it rejects invalid sequencing,
non-finite/out-of-range coordinates and more than 65,536 commands. Tests live in
`runtime/src/frame/tests.rs`. **Second implementation slice landed:** immutable measured `Frame`,
solid `Fill`, transformed `Group`, shared `Instance`, optional bounded baseline, and recursive
expanded item/path/depth budgets. Repeated instances count repeatedly. **Third implementation
slice landed:** reusable validated solid and linear-gradient Brushes, bounded ordered stops with
hard-edge duplicates, explicit pad/repeat/reflect policy, and Fill migrated from color to Brush.
**Fourth implementation slice landed:** recursive Vello Fill rendering for solid/linear Brushes;
Group and shared Instance transforms compose with the supplied outer transform, pinned against
Vello's encoded paths, stops and matrices. **Fifth implementation slice landed:** a Rust
`frame(Arc<Frame>)` El leaf measures from intrinsic content dimensions, honors explicit allocation,
paints from the padded content origin, and enters the ordinary Geometry transform/clip pipeline.
Radial/sweep brushes and Stroke remain unbuilt. **Sixth implementation slice landed:** sandboxed
Lua now has strict batched `gfx.path` compilation for M/L/Q/C/close into immutable runtime Path
userdata; malformed commands, sparse/named fields and runtime sequencing errors are pinned.
**Seventh implementation slice landed:** Lua now constructs reusable solid/linear-gradient
Brushes, compiles strict Fill/Group/Instance trees with `gfx.frame`, and displays them through the
normal strict `ui.frame` leaf. `demo_apps/frame_orbits/` is the live proof: cubic/even-odd paths,
gradients, nested transforms, repeated immutable instances, intrinsic sizing, clipping, and camera
zoom. `scripts/screenshot_frame_orbits.py` uploads it into a fresh shell, rejects console errors,
dumps the resolved tree on request, and captures a real bridge screenshot. The 1000×700 capture
exposed and fixed a viewport-centering error in the app and a 2pt moon-center/orbit mismatch.
Live zoom inspection remains manual until bridge gestures land. **Stroke vertical slice landed:**
runtime validates positive bounded width, caps/joins, miter limit and a 64-entry dash pattern before
Vello; Stroke is budgeted and rendered through nested Group/Instance transforms with outer alpha;
Lua exposes strict `gfx.stroke`; and `frame_orbits` now uses solid and dashed real strokes instead
of even-odd filled rings. `scripts/screenshot_frame_orbits.py` produced a clean-console 1000×700
live capture (`frame-orbits-stroke.png`). **Experimental Lua visual clock slice landed:** any
stable-id El can opt into `on_frame(dt, elapsed)`; time is monotonic Runner time, stalls clamp to
0.1s, callbacks dispatch after the current snapshot, and omission stops its redraw request. Only
Runner's first frame is guaranteed zero `dt`; custom screenshots currently dispatch callbacks;
stable callback generation, error quarantine and fixed-step world scheduling remain unbuilt. The orbital demo now computes its
motion in Lua, and the bridge script captured before/after images one second apart with a clean
console (`frame-orbits-motion-before.png`, `frame-orbits-motion.png`). This is an explicit
simulation exception to declaration-only presentation animation. The first motion proof also found
an app-math bug: rotating a radius traced a circle around an elliptical orbit. The demo now places
both planets and the moon parametrically (`x=rx*cos(t)`, `y=ry*sin(t)`); before/after bridge captures
(`frame-orbits-ellipse-before.png`, `frame-orbits-ellipse.png`) verify every body remains on its
painted path. The earlier next step—scaffolding a force graph and Frame-local hits—is superseded pending the
Environment rendering/lifetime gates below.

### 2D world — drawings, animation, a Rust-run game world

**Design draft 2026-09-28:** [2d-world.md](design/2d-world.md) — Lua describes, Rust runs; drawings
and clips are pure-data Lua modules; `ui.world` over `bevy_ecs` + `rapier2d` (proposed, not yet
added); the chest demo is the milestone. **Slice 1 landed:** runtime `drawing::Drawing` validates a
part tree (unique ids, known parents, no cycles, 256-part cap) whose list order is draw order and
whose `parent` is the hierarchy; `pose(overrides)` rotates/moves/scales parts about drawing-absolute
pivots, carries children with their parent, and compiles to an ordinary Frame of part-named groups,
so hits report the part id. Lua has strict `gfx.drawing(module)` over a pure-data module (paths as
command lists, solid fill, `{width, color}` round-joined stroke) and `drawing:pose({...})`; `clips`
and `use` are rejected until their slices. `demo_apps/hero` shows one drawing in four poses with
part hover; `scripts/smoke_hero.py` (in `SMOKES`) pins reachability and part hits. **Slice 2
landed:** a new `world` crate — the only `bevy_ecs` dependent, pinned `=0.18.1` because 0.19 needs
Rust 1.95 and the toolchain is 1.92 — holds `World2d`, whose all-or-nothing `reconcile` spawns,
keeps and despawns entities by author id (`pos` read only at spawn; a new drawing handle replaces
the look) and whose `frame` emits one entity-named instance each, in list order. Lua `ui.world`
reconciles into a per-app registry that outlives reload (carried like the console) and is swept
only after a successful walk; it paints through the ordinary frame leaf, so runtime is unchanged.
`demo_apps/world` and `scripts/smoke_world.py` (in `SMOKES`) pin spawn/despawn, entity hits, and
survival across a hot reload that edits `pos`. **Slice 3 landed:** `world::clip::Clip` validates
keyframe tracks (`x`/`y`/`rot`/`scale` per part; `linear`/`in_out` easing on the arriving segment;
loop or play-once-and-hold) and samples to part poses. An entity's `clip` handle becomes an
`Animator` stamped with the world clock when the handle first appears (same handle keeps playing,
new handle restarts, none returns to rest; a clip naming an unknown part is refused). A world with a
clip playing attaches the runtime's `on_frame` and routes it as `LuaMsg::TickWorld`, which sets the
clock in Rust without calling Lua; `on_frame` on a world is therefore an error. Lua has strict
`gfx.clip{length, loop, tracks}`. `demo_apps/world` idles the hero and opens the chest lid on
click; `smoke_world.py` clicks and uses `rpc.advance` to check the lid open and closed. Not built:
clip events, speed, transitions, layers; a finished once-clip keeps the world ticking. **Slice 4
landed:** an entity's `controller = { speed, axis_x = {neg, pos}, axis_y, moving }` (strict) becomes
a `Controller` component; the world holds key state, attaching the runtime's `on_key` only when a
controller exists and routing it as `LuaMsg::KeyWorld` (a cancel releases all), so `on_key` on a
world is an error. `tick(elapsed, dt)` moves along held axes at `speed × dt` (diagonals
normalised) and plays `moving` while moving, the entity's own clip when still; a reconcile does not
restart a walk. Clips are checked by part name via `Drawing::has_part`. The demo hero walks with
WASD; `smoke_world.py` holds D through `rpc.keyboard` and checks the hero moved and stopped.
**Slice 4b landed:** `world::facing` — the controller sets a `Dir` (larger move component wins, an
exact diagonal keeps the facing, stopping keeps it) and an entity's `facing = { down, up, side,
left }` views (each `{drawing, clip, moving}`, falling back to the entity's own) pick what shows;
`left` mirrors `side` within the drawing's box unless given. One `Appearance` component caches the
rest pose per distinct drawing; every view's drawing is checked against every clip it can play.
`Drawing::size` was added for the mirror. The demo has back and side hero drawings and a side walk;
the smoke tells views apart by outline (profile nose asymmetry, back-view bun).
**Slice 4c, chunk 1 landed:** `ui.world { order = "y" }` sets `world::Order::Feet` — a stable sort
by the bottom of each entity's box (y + height × scale, rotation ignored), so ties keep list order.
The smoke walks the hero onto the chest and checks a leg is covered a step above and in front a step
below. **Slice 4c, chunk A landed:** moments leave the world as `WorldEvent`s — `on_action` for a
fresh press of a key named in the world's `actions`, `on_move` when a controlled entity's held
direction changes — queued in `World2d` and handed to Lua by `LuaApp::world_events` after a tick
or key. The demo opens the chest on E and shows the last heading; the smoke checks both. A jump
done in Rust was built and dropped the same day: a press is a decision, so it belongs to Lua.
**Chunk B landed:** facing and clip choice moved to Lua. `world::facing` (`Dir`, `Facings`,
views) and the controller's `moving` clip are gone; the entity carries `flip` (mirror within the
box), one rest pose, and a clip changed only by reconcile. The demo's `on_move` picks the view,
the flip and walk-or-idle; the smoke's facing checks pass unchanged. **Chunk C landed:** no height
primitive was needed — a jump is a once clip lifting the root part while the entity's feet stay
put. The one Rust addition is `on_clip_end` (`WorldEvent::ClipEnd`, once per play). The demo
jumps on Space with no double jump, all in Lua; the smoke checks the head rises, lands, and a
second jump replays (which only `on_clip_end` makes possible). **Chunk D landed:** `attach = { to, part, at }` —
the world places a carried entity at its carrier's part each tick (`Drawing::part_at` gives a
part's posed transform), draws it just in front of the carrier, and leaves it where it was on
detach; a flipped carrier mirrors the carried box. The demo picks the chest up with E (anywhere,
until slice 5's sensor) and plays `carry` / `carry_walk`, chosen by a small `hero_clip()` in Lua.
Not built: walls/physics, key-code validation, a world-level `DumpTree` (built in slice 5), a per-view carry point,
skipping `view` when only a world ticked (today the runtime re-runs `view` every rendered
frame, so Lua re-describes the world each frame while it animates), and letting a world stop
ticking once every clip is a finished once clip (a held pose still asks for frames).
**Slice 5 (Rapier) started — 5a, walls, landed:** `world` depends on `rapier2d`. An entity's
`collider` (`{ circle | rect, at }`, plain data, strict in Lua) becomes a Rapier body — fixed, or
kinematic under a controller — kept while unchanged and dropped with the collider or the entity.
`tick` moves a solid controlled entity through Rapier's character controller, set up for
top-down (no gravity, no ground snap, no slope limits): it stops at walls and slides along them,
the same in every direction. The world's frame is clipped to its box (`Frame::clipped()`,
opt-in; other frames still spill). The demo has thin side walls and a tall back wall, and the
hero collides by a circle at its feet; the smoke checks it stops at the east and back walls.
**5b chunk 1 landed:** a carried entity has no body (it is off the floor) and gets one again
where it is let go; the demo chest is solid by its footprint, and the smoke's draw-order route now
walks round it. **5b chunk 2 landed:** a let-go entity becomes a dynamic body
with its carrier's last `Velocity`, slides (friction 6, bounce 0.5, no spin) with Rapier copying
it back to `Transform` each tick, and settles to fixed once slow and not pressed into anything —
so one let go inside a wall is pushed out first (Rapier does that slowly, and keeps ~0.012 of
resting overlap on purpose). **5c landed:** `sensor` on any entity (the
`collider` shape words, on the same body); each tick the world diffs Rapier's sensor overlaps
against the last tick's and queues only changes, which Lua gets as `on_zone(e)` with `e.id`,
`e.who`, `e.phase`. Fixed-on-fixed never counts (walls); moved-on-fixed does, switched on per
sensor. The demo's E picks the chest up only within its zone; the smoke checks E out of reach
does nothing and `near` follows through pickup and drop. That completes the chest demo's
mechanics. **Lift and fall** (Lua and clips only): the chest is attached at its floor spot in
front of the hero and `lift` raises its drawing into the hands, so let go, `fall` drops it to a
footprint already on the floor; picking it up shuts the lid. **Lenient reconcile:** a clip track
for a part the drawing lacks is skipped and noted once to the console (`World2d::drain_notes`,
drained after each view) instead of refusing the world — asked for by the assets session, whose
drawings change live; `attach` to a missing part stays an error. **Turning mounts:** `attach` takes
`pivot` (the carried drawing's own point placed on `at`) and `turn = true` (it takes on the part's
rotation, scale and mirror about that pivot) — asked for by the assets session so a hat stays on a
nodding head; without `turn` a carried thing stays upright as before. **The world in the dump:**
`World2d::inspect` gives each entity's place, body kind, velocity, carrier, zones and clip; the
shell's `DumpTree` puts it on the world's element (`world.entities`), and the world smoke now
reads the chest from it. Its first run caught a bug pixels never showed: a chest let go near the
south wall landed half through it, Rapier put it to sleep pressed in, and it never settled — the
world ticked forever. **Fix:** let go, a body comes from its carrier's body to its spot and stops
against what is in the way (`Physics::bring_in`). **5d chunk 1, loose bodies (world only):**
`EntitySpec::loose` with a `Material` (`bounce` 0..1, `friction` as speed lost per second) makes a
Rapier body that physics alone moves; it rests asleep, never fixed, so it can be pushed again, and
spawned at rest it starts asleep. A bounce is now the moving thing's own (walls keep none).
**Chunk 2, pushing:** what a walker's move bumps into that Rapier moves gets Rapier's character
impulses, the walker counting as far heavier; a crate's friction drags, so the hero pushes it at
about half pace. **Lua `loose`:** `loose = true` or `{ bounce, friction }`, strict. **The demo
grew:** a 1080×600 room split by an inner wall with a doorway, two pillars, three crates and a ball;
the world smoke pushes a crate and reads from the dump that it moved and came to rest still loose.
**Table hockey** (`demo_apps/hockey`): two walking paddles, a loose puck, goals as zones Lua
scores. **Collision groups:** `group` and `blocks` — the centre line stops paddles, not the puck.
**Rapier's unit:** it was in metres, capping loose bodies at 400 units a second; now
`length_unit = 100`, and settling measures overlap afresh. **Hockey rules (Lua):** a faceoff
puck drops for a second with no body (a clip; `on_clip_end` makes it live), first to 5 wins.
`smoke_hockey.py` reads it all from the dump — and caught a puck put down inside a paddle asleep
for good: a loose body spawned in something now starts awake and is pushed out. **Commands:** a
handler changes an entity at a moment with `world(id):set(entity, { pos, velocity })` — refused
in `view`; hockey's puck keeps one id and is put back on the spot. **Timers:**
`world(id):after(secs, name)` → `on_timer`, a deadline Rust holds on the world's frame clock (the
dump lists them); a goal now shows for 1.5 s with the puck caught in the net. It shook out a
panic: a body removed inside a sensor, then a frame with no time in it, left Rapier a stale pair.
**Hits:** `on_hit { id, who, speed }` — a loose thing's new contact, at the speed it closed
along the normal; hockey's puck squashes on each, harder for a harder hit, and the smoke reads
the squash from the dump. The Lua ↔ world contract is three kinds of call: describe (`view`),
command (`set`, `after`), and questions. Next, in order: questions (`ray`, `at`); moving things give Rapier their
velocity, so a paddle hits rather than shoves; controller acceleration; a `follow` controller.
**Owed — the agent-as-maker test:** no agent has yet built a game from `docs/lua-apps.md` alone
(hockey was written with full context). A fresh agent, given only the docs, builds carrom and
logs every wall it hits in `gap-log.md`, as `six-apps.md` did for apps. **Lua app tests started:**
[design/lua-app-tests.md](design/lua-app-tests.md) records app-shipped Lua tests run beside, not
inside, the app VM; slice 1 is built (`RunTests` discovers `tests/*.lua`, opens a non-persisting
temporary app tab, runs a separate test VM with `t.expect`, Runner-driven `t.step`/`t.click_at`,
`t.rects`/`t.centre_of`, `t.text`, and `t.world`, and returns pass/fail results). This supersedes the narrower hints-in-dump and
bridge-test-pushes as the route to app behaviour assertions. Next for tests: deterministic stepping
and `t.world()` snapshots. Soft bodies (rope, cape)
are the assets session's own verlet solver, not Rapier joints.

### Environment — composable 3D interfaces and worlds

**Planning baseline 2026-09-12:** [environment-runtime.md](design/environment-runtime.md) is the
handover and plan of record for the newly required Lua-authored retained environment. No World,
ECS, 3D mesh/depth renderer, physics binding, PBD cloth, projected UI surface or world picking is
built. Frame remains 2D; Taffy/Parley remain candidates for logical UI surfaces. Two GPT Sol
research passes recommend first testing a narrow same-device WGPU compositor while treating Bevy
0.19/Vello 0.9 as a measured challenger—not selecting either by prose. Rapier2D/3D is reserved for
rigid bodies; PBD/XPBD is the candidate for cloth/deformables; custom/Lua systems remain valid where
bounded. **2026-09-20 revision:** the immediate experiment is the
[Lua-first 3D model viewer proof](design/3d-model-viewer.md): two depth-intersecting built-in
objects inside a shell-hosted Lua app, camera/object controls, picking/highlighting and final
composite bridge screenshots, with no CPU texture readback between rendering passes. GLB import
follows separately; Blender is only an authoring tool, not a runtime dependency. The selected
substrate is an owned WGPU pipeline using focused libraries (`gltf`, `glam`, Parry and optional
direct Rapier) rather than Bevy or its coupled rendering crates; no ECS is selected. Pin callback
scheduling before relying on continuous animation. The earlier projected Vello/Taffy panel gate
remains a broader Environment target, not something this narrower proof establishes. **First 3D
slice landed:** runtime validates immutable perspective-camera scenes containing at most 256
stable-id built-in cubes, renders instanced geometry with a shared-device depth pass into both live
and custom capture targets, and places one visible `ui.scene3d` leaf through ordinary Lua layout.
`gfx.scene3d` is strict and `DumpTree` includes its bounded camera/object inspection. The live
`demo_apps/model_viewer` proof shows two depth-intersecting cubes with face normals, simple
directional lighting, 4× MSAA, app-driven rotation buttons, drag orbit and wheel zoom; clean-console
1000×700 bridge captures are `model-viewer.png` and the corrected `model-viewer-msaa.png`. Wheel
input enters normal topmost-element eligibility before scroll/zoom ancestors and is pinned by the
headless Runner test. **Picking slice landed:** ordinary scene `on_click` now constructs the camera
ray, intersects every transformed cube in local space, chooses the nearest visible hit independent
of declaration order, and reports stable object ID, distance, world point and world normal as plain
Lua event data. The demo highlights the selected cube and updates its ordinary UI label; a headless
Runner test pins that picking enters through normal pointer eligibility. Current limits are explicit:
one rendered viewport, no hover picking, and the resolved 3D overlay currently follows the complete
Vello scene so foreground overlays are not yet correct. GLB, retained resource lifetime and the
public World API remain open.

### Runtime and app milestones

Roughly in dependency order:

1. **The bridge port — `osvauld-rpc` + `osvauld-mcp` onto shell2** (w3 §5–7; the M1 exit
   test). *Revised 2026-09-10: the port is built* — transport plus every wired family is
   documented in the dated bullets below; `osvauld-mcp` was deleted instead of ported (see
   the 2026-09-10 senses bullet). sthalam's `bridge.rs` pattern — vault mutated
   *on the bridge thread*, `Refresh` snapshots merged by the UI — is explicitly **not** what
   ports. What the port is:

   - **landed 2026-09-09, the rpc vocabulary**: `osvauld-rpc` rewritten — auth
     (`Ping`/`ListAccounts`/`Signup`/`Unlock`/`Lock`, an addition to this list: headless
     login is the automation story's first step), workspaces/items (incl. `CreateWorkspace`
     and `OpenItem`), files (`WriteFile` reloads an open tab; `ReloadItem` forces the staged
     reload), senses (`DumpTree`/`Click`/`ReadConsole`), app-shipped test runner (`RunTests`),
     and `AppDataGet` (the write half of the old `AppData*` family was removed 2026-09-10 — see the senses bullet) by `item_id`
     alone (ids are 128-bit random). The sthalam families are deleted. Not
     ported from the old repo's control server, on purpose: `eval`, coordinate `ui_mouse_*`,
     p2p, recording. Socket must be created `0600` — passphrases cross it.
   - **landed 2026-09-09, the bridge transport**: `shell2/src/bridge.rs` — a pure-transport
     UDS thread on `$OSVAULD_SOCKET` (default `/tmp/osvauld.sock`, `0600` by construction —
     staged, locked, atomically renamed; a live or non-socket path is never clobbered), one
     request per connection, forwarded as
     `Msg::Rpc(Request, Sender<Response>)` and executed on the UI thread in `Shell::update`
     the single authority; the event delivery is the loop's wakeup). *Revised 2026-09-10:*
     the socket starts from `App::ready`, after winit's window/renderer exist and its loop is
     polling — publishing it from `run_with`'s builder lost rapid startup events despite
     `send_event` returning success. Three consecutive full smokes pin the fix. Live families:
     `Ping`/`ListAccounts`/`ListWorkspaces` (handler is a testable free fn over the vault),
     auth — `Signup`/`Unlock`/`Lock`: Argon2 prepare runs on a worker (mirroring the login
     screen), then `Msg::AuthDone` commits the account, replies, and lands the screen
     transition on the UI thread; a script-side signup returns the mnemonic and skips the
     mnemonic screen (the script is its reader), workspaces/items (`CreateWorkspace`,
     `ListItems`, `CreateItem`, `OpenItem`), app source files (`ListFiles`, `ReadFile`,
     `WriteFile`, `ReloadItem`), and the first `RunTests` slice. Everything else answers an honest `not wired yet`. Python harness:
     `scripts/osvauld/` (`client.py` framing + `session.py` spawn/wait/teardown) and
     `scripts/smoke_bridge.py` — the end-to-end proof over a fresh, locked vault.
   - **landed 2026-10-01, app-shipped test runner slice 1**: `RunTests { item_id, filter }`
     discovers `tests/*.lua` in the app source doc, opens a non-persisting temporary app tab with
     empty docs and a `test:<item>:<run>` retained-id namespace, runs each file in a separate
     source-only test VM with `t.expect`, Runner-driven `t.step`/`t.click_at`, `t.rects`/`t.centre_of`, `t.text`, and `t.world`, and returns `{name, ok, frames, failure}` results. `tally`,
     `scratch` and `pomodoro` now ship basic `tests/*.lua`, pinned by `scripts/smoke_lua_app_tests.py`.
     Test VMs run on a worker and step/click through deferred Runner driver ops; the batch remains
     bounded (32 files / 256 KiB plus the Luau interrupt budget). Pinned by `scripts/smoke_bridge.py`
     over the real socket. See [Lua app tests](design/lua-app-tests.md).
   - **landed 2026-09-10, senses & actions on running apps**: `DumpTree` (the pre-layout
     `ElInfo` tree as JSON — kinds, ids, text, handler flags; overlays included), and the
     verbs `Click`/`Type`/`Key` (`enter`/`esc`): each resolves the element by id on a fresh
     `view()` — the same registration the next frame uses — fires its behaviour, and routes
     the produced `Msg::Tab` to the tab directly (RPCs already run inside `Shell::update`;
     recursing would flush twice). `DumpTree` and the verbs reload-if-stale first, so a dump
     right after `WriteFile` shows the new source. Senses: `ReadConsole` — a bounded (512),
     consecutive-deduped console on every `LuaApp` fed from view/handler/reload/open errors,
     surviving VM swaps like the cores do; `AppDataGet` — the live core docs as sorted-name
     deep JSON (pre-flush; Lua numbers arrive as doubles). Principle, settled against the
     kanban `add` handler: **drive the UI, not the doc** — the app's own handlers run the
     checks, stamps and side effects (an empty-draft guard, `uuid()`, draft clearing) that a
     doc write skips, and half an action's input (the `ui.state` draft) is not in the doc at
     all. So the specced `AppData` write family (`SetText`/`RowAdd`/`RowSet`/`RowRemove`) was
     removed unwired — re-spec against a real seeding need. `osvauld-mcp` (the sthalam-era
     MCP shim) and `.mcp.json` were deleted the same day, unused — an MCP face rebuilds over
     the bridge if ever wanted. Still open from the survey: right-click (runtime ready, one
     arm), `Drag`/`Drop` synthesis, and fresh-view-validated action locators for un-id'd controls.
     *Revised 2026-09-11:* locators supersede a blanket id retrofit for ordinary buttons; stable
     ids remain preferred for scripts and required wherever retained or multi-phase identity matters.
   - **landed 2026-09-10, live screenshots**: `Screenshot` defers its RPC reply until Runner
     paints the next frame. With no dimensions it reads the exact live Vello target; a custom
     logical width/height and physical scale run the same layout/hit/paint path against a
     temporary target, skip surface presentation, clear temporary hit geometry, and request
     a normal restorative frame. Both paths reuse the live device, renderer, text engine,
     retained store, VM and docs — no headless rebuild. Readback strips wgpu's padded rows,
     encodes PNG, and returns base64 plus physical dimensions; custom output is capped at 16
     megapixels. `Bridge.save_screenshot` writes it directly. The smoke proves both the live
     window capture and an exact 320×240 custom capture.
   - **designed 2026-09-11, app discovery and invocation:**
     [`design/app-discovery-and-invocation.md`](design/app-discovery-and-invocation.md) separates
     untrusted app prose/source from host instructions, current UI action locators from stable ids,
     and optional explicit app commands from forbidden arbitrary Lua evaluation. These additions
     are unbuilt; a future MCP face remains a thin client of the same RPC surface.
   - **designed 2026-09-11, resolved UI senses and gestures:** keep pre-layout `DumpTree` intact;
     Runner instead answers bounded, fresh-frame element/subtree and screen hit-stack queries with
     explicit content/screen geometry, clips, computed layout, scroll/thumb, and camera state.
     Pointer sequences and wheel modifiers route through normal eligibility for zoom/pan/drag tests;
     optional screenshot annotations share the snapshot. See
     [`design/app-discovery-and-invocation.md` §7](design/app-discovery-and-invocation.md).
     *Revised 2026-09-20: partly built.* The Runner-owned deferred seam (`App::take_driver`),
     `Rects` (reachable elements with their **visible** rects — the rect the hit-test actually
     tests), pointer/drag synthesis through the normal path, and `Frame`/`Advance` on the virtual
     clock all landed. Still unbuilt: `InspectElement`/`InspectSubtree`, `Wheel` and modifiers,
     query bounds, and the *ordered* hit stack with clip rejections — §7 carries the full split.
   - **ports as-is**: the wire transport (`read_msg`/`write_msg`, 4-byte length prefix;
   `Response::{ok,err}`) and the MCP shim's stdio↔UDS *shape*
   - **is replaced**: bridge becomes pure transport — a `UnixListener` thread on
     `$OSVAULD_SOCKET` (default `/tmp/osvauld.sock`), one request per connection, forwarded
     as `Msg::Rpc(Request, Sender<Response>)` over the event-loop proxy; **every request
     executes on the UI thread** inside `Shell::update` (single authority, no merge,
     repaint free via the existing `Wake`/`DocChanged` seam)
   - **is trimmed**: the block-doc family (M3-era), the import/`TableSql` family (M2-era),
     `ExportPdf`/the old **headless** `Screenshot`, and per-block `.lua` edits all drop out.
     *Revised 2026-09-10: a live-frame Screenshot replaced that headless implementation.* New surface:
     `ListWorkspaces`, **`CreateWorkspace` (the old enum never had it)**, `ListItems`,
     `CreateItem`, `ListFiles`, `ReadFile`, `WriteFile`, `AppDataGet`, `AppDataRow*`,
     `AppDataSetText`
   - **is new**: the senses — `DumpTree` (the `El` tree as JSON, no rects), `Click` by
     element id, `ReadConsole` (LuaApp's errors become a bounded ring buffer, not
     `eprintln`); and `WriteFile` against an open tab reloads its VM keeping the doc —
     the free half of hot reload
   - **landed 2026-09-20, the driver family**: `shell2 --offscreen WxH` runs the whole
     shell with no window — real layout, real pixels (`capture_scene` never needed a
     surface), and a virtual clock that moves only when a request asks. `Frame(n)` /
     `Advance(secs)` drive time, `Rects` says where a pointer must land, and
     `PointerMove`/`PointerPress`/`PointerRelease`/`Drag` go through the same methods a
     window calls — pinned by a test asserting one gesture is event-for-event identical
     across both drivers. `OSVAULD_OFFSCREEN=WxH` makes every existing `scripts/` Session
     windowless untouched. **Windowless, not headless**: `EventLoop::build()` still needs a
     `DISPLAY`. See [`design/six-apps.md` §7](design/six-apps.md).
   - **needs small runtime/app_host support**: `El::to_json()` + find-by-id for dump/click;
     the console ring buffer
2. **W4 DX**: types gate, and per-block `.lua` edits (needs the splitter port).
   An editor-shaped gate was built and removed on 2026-09-21 — `lua-language-server` stubs
   generated from the sandbox. Two reasons, and the second is the one that matters. It never
   ran: `workspace.library` resolves relative to the folder being checked, so the per-app runner
   loaded no definitions at all and was green because `diagnostics.globals` silenced the names.
   And the author is an agent writing over the bridge, which opens no editor and reads no
   `.luarc.json` — for it, the type system is the error the runtime hands back. A gate here
   should be that, not stubs.
   *Screenshot landed 2026-09-10; error-card polish is listed under Built; see bridge item 1.*
3. **Agent source editing — landed 2026-09-22:** `ReadFileVersioned` returns source plus a
   SHA-256 content revision; `EditFile` applies bounded, revision-checked exact replacements
   directly to the existing `LoroText`, without disk working files or whole-file normalization.
   Missing/ambiguous matches, stale revisions and overlapping batches reject before mutation;
   Unicode offsets, snapshots and wire round trips are pinned. Open apps persist then stage and
   report activation separately; closed apps report that no VM is running. The Python client and
   `smoke_bridge.py` prove live editing, stale rejection, failed-reload survival, repair and
   persistence across reopen. Explicit persistence-failure injection remains unbuilt. See
   [agent-source-editing.md](design/agent-source-editing.md). Semantic Loro source storage and
   structural node operations are deferred; `lua_tree` remains isolated groundwork.
   **Hot-reload correction:** `WriteFile` already writes source after upload and triggers the
   existing staged reload for open apps. The earlier claim that this trigger was missing was
   stale. Persistence is not proof of successful activation; invalid source can remain durable
   while the old VM runs. A file watcher remains unbuilt and is not required for bridge editing.
4. **nid channel — deferred 2026-09-22** (`design/nid-channel.md`) — the provenance channel:
   a click resolves back to the source construct that drew it. Nice to have, not a prerequisite
   for agent text edits. Source nids are distinct from existing runtime/UI ids, which stay.
   Designed, costed, **prerequisites landed; the channel itself is unbuilt**.
   The earlier implementation outline is retained below for later reconsideration:

   - **built**: the tree carries ids (`lua_tree` mints them, a printed `_nid` round-trips
     back as identity, not a field); `print_bare` (id-free — what apps run today) with the
     one-constructor-per-line printer rules pinned by test; chunks are named
     (`set_name`) so `debug.info` keys are well-defined; the cost measurement exists in
     `cost_curve`'s id probe (~+180–220 ns/element — affordable, use a string)
   - **unbuilt, in build order**: `print_bare` returns `(text, line→nid map)` — signature
     change, `round_trip` is its only caller · host installs the map as per-chunk `_nids`
     tables **inside `build`** (not beside it — a staged reload must not leave a map aimed
     at the wrong text) · tagger gains `debug.info(2, "sl")` (today it asks only `"l"`) and
     stamps `t._nid` · `_nid` joins `STRUCTURAL` so `props::apply` accepts it · `walk`
     carries it onto `El` · something consumes it (the bridge's `DumpTree`/`Click` are the
     natural first consumers, and the right-click → `set(nid, prop, value)` edit path is
     the payoff)
   - **to pin with tests**: the `[string "…"]` wrapper normalisation (a silent whole-app
     miss is the failure mode — every element gets a nil nid and nothing errors);
     multi-file end-to-end; map/text travelling as one artifact
5. **M2 — table + data plane + charts** (plan §4): Loro scale spikes, `table_core` port
   (T1), grid island, rich-text Lua surface, Polars mirror (T2) + `data.*`, `chart` crate
6. **M3 doc editor, M4 canvas** (plan §5–6)
7. **Deferred M0 polish**: IME cursor-area positioning, a11y stub, dirty-gate, images —
   each deferred to where its consumer lands (see the table in the archived 3-month plan)

## Working-tree notes

**2026-09-26 cleanup:** the twelve reference-only crates (`app_engine`, `sthalam`,
`doc_editor`, `block_doc`, `code_editor`, `code_highlight`, `text_edit`, `rich_text`,
`pdf_paint`, `table_*`) left the tree; they are readable at the `reference-crates` tag, which
M2's `table_core` port reads from. Committed dev vault stores (`osvauld-drive-data/`), sample
`.xlsx` sheets and root screenshots were removed; screenshots now go to the git-ignored
`shots/`.

**2026-09-26 talk deck:** `demo_apps/slides` is a Lua slide deck that embeds whole demo apps
(an app's `main.lua` returns its view, so the deck calls it). `require` resolves from the app
root, so `scripts/deck_upload.py` mounts each demo under `demos/<name>/` with its requires
rewritten, and uploads every demo standalone too; `scripts/demo_wave.py` drives the embedded
arm through its own buttons. Keyboard zoom (Ctrl +/-/0) is Lua-side `scale` + `offset` around
the last hover point. The shell's app grid now scrolls. `README.md` added.
