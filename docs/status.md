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

Roughly in dependency order:

1. **The bridge port — `osvauld-rpc` + `osvauld-mcp` onto shell2** (w3 §5–7; the M1 exit
   test). Both crates are sthalam-era and wired to nothing: the RPC surface describes
   block-docs, tables and imports this shell cannot host yet, and sthalam's `bridge.rs`
   pattern — vault mutated *on the bridge thread*, `Refresh` snapshots merged by the UI —
   is explicitly **not** what ports. What the port is:

   - **ports as-is**: the wire transport (`read_msg`/`write_msg`, 4-byte length prefix;
   `Response::{ok,err}`) and the MCP shim's stdio↔UDS *shape*
   - **is replaced**: bridge becomes pure transport — a `UnixListener` thread on
     `$OSVAULD_SOCKET` (default `/tmp/osvauld.sock`), one request per connection, forwarded
     as `Msg::Rpc(Request, Sender<Response>)` over the event-loop proxy; **every request
     executes on the UI thread** inside `Shell::update` (single authority, no merge,
     repaint free via the existing `Wake`/`DocChanged` seam)
   - **is trimmed**: the block-doc family (M3-era), the import/`TableSql` family (M2-era),
     `ExportPdf`/`Screenshot`, and per-block `.lua` edits all drop out. New surface:
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
   `luau-lsp analyze` before any swap), error-card polish (known gaps listed in w3 §
   "Deliberately NOT W3": leaked `runtime error:` prefix, missing-id messages for drag
   binds, unknown-tag not recorded, path separator inconsistency), `screenshot`
   (offscreen wgpu), per-block `.lua` edits (needs the splitter port)
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
