# Architecture — the current map

What Osvauld is, how the pieces fit, and the invariants the code leans on. Written 2026-09
from the code as it stands; progress lives in [`status.md`](status.md), the future in
[`design/runtime-rebuild-plan.md`](design/runtime-rebuild-plan.md).

## What Osvauld is

A local-first, end-to-end-encrypted workspace of **typed items** — `.app`, `.doc`, `.table`,
`.canvas` — synced between peers through CRDTs (Loro). The desktop shell (`shell2`) hosts
apps authored in **Lua (Luau)**, frequently written by an agent over MCP: the agent writes
`.lua`, the shell runs it, the window repaints. Rust is the substrate; Lua is the product
surface. Accounts own a local encrypted store (`vault`); sync is designed-for but not yet
built.

## The era break (2026-06)

Two shells preceded this one. egui broke on text — raster glyphs smear when rotated, no Indic
shaping, unfixable from the app layer. Iced broke on authorship — its widgets are a
compile-time `Element<M>` tree, hostile to a tree built at runtime by an interpreted language.
Both failed at the same place: *the point where a human stops writing the UI and an agent
does.* The answer was to go one level down — winit (window/events), wgpu (GPU), vello (vector
paint), parley (text shaping, Indic included), taffy (layout): primitives with no opinion
about who builds the tree. The full story is
[`blog/why-we-built-our-own-runtime.md`](blog/why-we-built-our-own-runtime.md); everything
from before the break lives in [`archive/`](archive/README.md) and is read for lessons, never
extended.

## The pipeline

```
Lua app: view() ─ui.* tables─► walk ──► El<LuaMsg> ──┐
                                                     ├──► taffy solve ──► Placed ──► vello paint
Rust screen: El builders (typed M) ──────────────────┘
```

- **One node vocabulary, two front-ends.** `ui.*` in Lua and `El` builders in Rust produce
  the *same* tree through the same layout/paint/state machinery — same pixels, same pipeline,
  neither side is the "real" one.
- **Immediate description + retained islands.** `view = f(state)` rebuilds the description
  every interaction; heavy widgets (text editor, future grid/canvas) are retained islands
  that own their hot state. No VDOM differ, no reactive signals — a deliberate choice for
  LLM-authored code, re-validated in the 2026 landscape scan.
- **Messages are plain data.** Lua callbacks register into a per-frame table and dispatch by
  index (`LuaMsg::Call(u32)`); Rust screens use typed enums. The VM never leaks into the
  runtime — this is what keeps "apps off the UI thread" a door that stays open.
- **The CRDT is document truth** (Loro). Ephemeral per-viewer state (scroll, drags,
  unfinished UI input) never enters it. An app's *source* and its *data* are separate docs.
  **Clarified 2026-09-11:** “draft” previously meant transient viewer scratch here, not
  a durable local-only document such as an unsubmitted order. The latter can use Loro
  and persistence; its exclusion from network discovery/transfer is part of the unbuilt
  [sync design](design/workspace-permissions-sync.md).
- **External event sources start at `App::ready`.** Runner calls it once after winit has a
  window/renderer and is actively polling. Publishing a bridge socket from `run_with`'s builder
  creates a startup race: `EventLoopProxy::send_event` can succeed before events are deliverable.
- **Screenshots are deferred frames.** An app hands Runner a one-shot completion mapping;
  Runner paints before replying. Normal capture reads the live Vello target. A custom viewport
  runs the same layout/paint pipeline with the live text engine and retained store against a
  temporary target, never a second app/VM/device; temporary hit geometry is discarded and a
  normal frame restores the window.

## Crates

**Live — workspace members:**

| crate | what it is |
|---|---|
| `runtime` | the UI substrate: `El<M>` → taffy → `Placed` → vello; ids + keyed state store, scroll, drag, overlay, animation, text (parley), editor island. Owns the `App`/`Runner` loop, `ControlFlow::Wait` on-demand paint, and live/custom-frame PNG capture. |
| `app_host` | the app layer: sandboxed Luau VM (mlua), the `ui.*` walk, the props registry, `doc:open` mirror binding, multi-file `require`, `ui.state`, staged reload. The app-facing guide is [`lua-apps.md`](lua-apps.md). |
| `shell2` | the live shell: accounts over `vault`, workspaces/items, app upload, tabs (one running instance per item), theme; the kanban reference app in `src/kanban/`. |
| `lua_tree` | full-moon (Luau) parse → 22-kind semantic tree → printer; the substrate for surgical agent edits and nids. See [`design/code-as-tree.md`](design/code-as-tree.md). |
| `vault` | headless account manager: identity + storage over redb (one file per DID), workspaces, items, sealed source/doc storage. Loro-free by design. |
| `cryptography` `identity` `storage` | backend crates, unchanged by the rebuild. Contracts in [`identity.md`](identity.md), [`storage.md`](storage.md), [`vault.md`](vault.md). |
| `workspace` | canonical shared-resource addresses, exact/terminal-subtree scopes, and callable index handles. It names and matches targets but does not authorize, persist, sync, or interpret documents. |
| `osvauld-rpc` | UDS wire vocabulary for shell2 automation — auth, workspaces/items, source files, app senses (`DumpTree`/`ReadConsole`/`AppDataGet`) and actions (`Click`/`Type`/`Key`). Wired by `shell2/src/bridge.rs` (status item 1). The sthalam-era `osvauld-mcp` shim (and `.mcp.json`) was removed 2026-09-10, unused — an MCP face, if ever wanted, is a thin rebuild over the bridge. |

**Reference-only — not workspace members, port lessons never code:**

`app_engine` (the old egui Lua engine: style vocab, sandbox shape, Loro binding patterns),
`sthalam` (the abandoned shell: screen flow, bridge wiring), `doc_editor`/`block_doc`
(M3's model source), `code_editor`/`code_highlight` (block splitter + spans), `text_edit`/
`rich_text` (lessons), `pdf_paint` (re-target for M3), `table_core`/`table_query`/
`table_import` (M2 ports nearly as-is).

## Where data lives

One redb file per account (`vault`), records sealed to the account key:

```
account db        one per DID, Argon2-passphrase-sealed
ws/<ws>/…                     workspace metadata
ws/<ws>/item/<id>/meta        item metadata (name, kind)
ws/<ws>/item/<id>/src         the app's source doc, exported as a Loro snapshot
ws/<ws>/item/<id>/doc/<name>  the app's state docs, name-keyed
```

The **source doc** holds a `files` LoroMap — path string → one `LoroText` per file
(`main.lua` is the entry point; the path *is* the identity). The **state docs** hold what
the app's `doc:open` reads and writes. Code and data never share a container; a publish/
update ships as a Loro update blob that merges into `src`.

An app runs as: source doc → `LuaApp::open` (sandboxed VM, `require` over `files`) →
`view()` each frame → `walk` → `El`. Doc writes land in Loro immediately, the read-side
mirror is patched in place at the top of the next `view()`, and snapshots persist after
`update`. External writes (a future bridge, a peer) wake the window through the
`EventLoopProxy` seam — everything off-frame must, or it is invisible under
`ControlFlow::Wait`.

## Invariants — don't break these

- **Messages stay plain data**; the VM never leaks into the runtime.
- **Paint order == reverse hit-test order**, both from tree order; overlays paint last and
  hit first.
- **`walk` is the hot path** (~80% of a frame's Lua cost). New per-element reads need a cost
  argument — this is why dev breadcrumbs are gated.
- **Retained-store ids are namespaced per item** (`tab:<id>:…`) — two open apps must never
  share a field.
- **Reload stages a whole second VM and swaps** (Lua can't unload a chunk); doc cores and
  per-viewer scratch outlive the VM; a failed reload leaves the running app untouched.
- **Reads and writes to docs are different paths**: reads are plain Lua tables (the mirror,
  a frame behind), writes are explicit CRDT calls addressed by **stable id, never index**.
- **Elements are tagged tables** — only `ui.*` constructors produce them; a bare table is a
  splice group, a `doc.list`/`doc.map` value is data. Syntax can't tell these apart; the
  runtime can. (This is the `_nid` lesson, paid for once.)

## Document index

| doc | owns |
|---|---|
| [`status.md`](status.md) | what's built, what's next |
| [`lua-apps.md`](lua-apps.md) | how to write an app — the author's guide |
| [`design/runtime-rebuild-plan.md`](design/runtime-rebuild-plan.md) | plan of record: M2–M4, library verdicts |
| [`design/code-as-tree.md`](design/code-as-tree.md), [`design/nid-channel.md`](design/nid-channel.md) | the tree-as-artifact design and the provenance channel |
| [`design/loro-notes.md`](design/loro-notes.md) | Loro mechanics, read out of their source |
| [`design/workspace-permissions-sync.md`](design/workspace-permissions-sync.md) | design baseline: address/handle syntax and exact/subtree matching built; authorization, cross-app data, grant/key bundles, discovery, sync, and sovereign node unbuilt |
| [`CONVENTIONS.md`](CONVENTIONS.md) | code/test/doc conventions |
| [`archive/README.md`](archive/README.md) | everything historical, and why |
