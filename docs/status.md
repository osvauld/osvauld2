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
- **kanban** (`shell2/src/kanban/`, 6 files): the reference app — typed drags (card/col
  sharing one `on_drop`), cross-column moves through the doc, resizable columns with
  clamped bounds, floating ghost outside every scroll clip, always-reserved drop guides.
  `demo_apps/tally` and `demo_apps/scratch` are the small examples.

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
issued, because delegations are minted between holders. Remaining in Gate 1: revocation with
cascade, and `role.assign` — node issuance of a role token with a rank check so an assigner
cannot mint above itself. Next slices: (1) how a
maintainer hands out an app role — delegation cannot change a role, so role assignment needs
node issuance under `role.assign`; (2) a durable node admin store holding
tokens, lineage, and revocations — `admins` is an in-memory `Vec` today and is lost on restart.
QUIC/Iroh wiring comes after. No desktop UI claim handler, durable admin store, QUIC protocol,
workspace publish, or sync exists yet.

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

### Environment — composable 3D interfaces and worlds

**Planning baseline 2026-09-12:** [environment-runtime.md](design/environment-runtime.md) is the
handover and plan of record for the newly required Lua-authored retained environment. No World,
ECS, 3D mesh/depth renderer, physics binding, PBD cloth, projected UI surface or world picking is
built. Frame remains 2D; Taffy/Parley remain candidates for logical UI surfaces. Two GPT Sol
research passes recommend first testing a narrow same-device WGPU compositor while treating Bevy
0.19/Vello 0.9 as a measured challenger—not selecting either by prose. Rapier2D/3D is reserved for
rigid bodies; PBD/XPBD is the candidate for cloth/deformables; custom/Lua systems remain valid where
bounded. Immediate gates: repair/pin callback scheduling semantics, then render and ray-pick two
depth-intersecting Y-rotated Vello/Taffy panels with a bridge screenshot and no CPU texture
readback. Dependency and public World API decisions wait for those results.

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
     reload), senses (`DumpTree`/`Click`/`ReadConsole`), and `AppDataGet` (the write half of
     the old `AppData*` family was removed 2026-09-10 — see the senses bullet) by `item_id`
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
     `ListItems`, `CreateItem`, `OpenItem`), and app source files (`ListFiles`, `ReadFile`,
     `WriteFile`, `ReloadItem`). Everything else answers an honest `not wired yet`. Python harness:
     `scripts/osvauld/` (`client.py` framing + `session.py` spawn/wait/teardown) and
     `scripts/smoke_bridge.py` — the end-to-end proof over a fresh, locked vault.
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
     optional screenshot annotations share the snapshot. This is unbuilt; see
     [`design/app-discovery-and-invocation.md` §7](design/app-discovery-and-invocation.md).
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
   - **needs small runtime/app_host support**: `El::to_json()` + find-by-id for dump/click;
     the console ring buffer
2. **W4 DX**: types gate (generated `.d.luau` stubs from the one binding registry +
   `luau-lsp analyze` before any swap), and per-block `.lua` edits (needs the splitter port).
   *Screenshot landed 2026-09-10; error-card polish is listed under Built; see bridge item 1.*
3. **Hot-reload triggers**: the engine half exists (`Source` version watch + staged
   `reload`), but nothing writes the source doc after upload — the file watcher and the
   bridge's `WriteFile` are the missing triggers
4. **nid channel** (`design/nid-channel.md`) — the provenance channel: a click resolves
   back to the source construct that drew it. Designed, costed, **prerequisites landed;
   the channel itself is unbuilt**:

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

`demo_apps/` (tally, scratch), `app_host/src/tests/scratch.rs`, and `vault/examples/`
are untracked — they should be committed with the next slice.
