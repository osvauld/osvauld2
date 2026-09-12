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
- drag gesture: phases + pointer capture, element-relative coords
- overlay layer: anchored, catcher + click-away dismiss, flip/clamp
- animation: external wakeup (`EventLoopProxy`), timer wheel, retained transitions
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
`agent_x` are research references, not compatibility contracts. **Third slice landed
2026-09-12:** Vault rejects malformed workspace/item ids and ambiguous document names at
public key-building boundaries, and listing scans ignore malformed stored keys; Vault remains
an opaque sealed store rather than an authorization engine. The broader design
checkpoint is the shop's namespace/capability/processing table, challenged against
booking and chat; exact rules, grant/key formats, index hierarchy, and backend ownership
remain to be designed. Work packages and acceptance scenarios are in the design note.

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
     arm), `Drag`/`Drop` synthesis, and ids on kanban's un-id'd buttons.
   - **landed 2026-09-10, live screenshots**: `Screenshot` defers its RPC reply until Runner
     paints the next frame. With no dimensions it reads the exact live Vello target; a custom
     logical width/height and physical scale run the same layout/hit/paint path against a
     temporary target, skip surface presentation, clear temporary hit geometry, and request
     a normal restorative frame. Both paths reuse the live device, renderer, text engine,
     retained store, VM and docs — no headless rebuild. Readback strips wgpu's padded rows,
     encodes PNG, and returns base64 plus physical dimensions; custom output is capped at 16
     megapixels. `Bridge.save_screenshot` writes it directly. The smoke proves both the live
     window capture and an exact 320×240 custom capture.
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
