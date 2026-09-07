# AGENTS.md

Osvauld: a local-first, encrypted workspace whose apps are written in **Lua (Luau)** and run
by our own UI runtime. Rust is the substrate; Lua is the product surface.

**Read before working** (in this order, they are short):
1. `docs/architecture.md` — the map: crates, pipeline, invariants, where data lives
2. `docs/status.md` — what's built, what's next
3. `docs/lua-apps.md` — the app author's guide (read fully before writing any `.lua`)
4. `docs/CONVENTIONS.md` — how code, tests and docs are written here

## Traps — these have all been fallen into once

- **Reference-only crates are not workspace members.** `app_engine`, `sthalam`, `doc_editor`,
  `block_doc`, `code_editor`, `code_highlight`, `text_edit`, `rich_text`, `pdf_paint`,
  `table_core`, `table_query`, `table_import` — port lessons, never code; never extend. The
  extension blocks writes into them.
- **The docs keep their history.** Design docs carry dated revision notes and disproved
  sections on purpose (`code-as-tree.md` §4 vs §13). Status lines at the top of each design
  doc say what's real. When code and doc disagree, code wins and the doc gets fixed.
- **The Lua surface is strict.** Unknown props are errors; the doc mirror is a frame behind
  your own write; `on_drag`/`on_drop`/scroll need an `id`. The guide says all of it — read it.
- **`.mcp.json` is wired but dead** until the bridge port lands (status item 1). The
  osvauld tools will not respond.

## How we work — the process rules

- **Write in slices.** One write lands at most **~100 new or changed lines of code**; tests
  ride free. A task needing more is *first* broken into chunks — think through each, explain
  it so the user understands it, get a nod, then write chunk by chunk. The extension warns
  when a write exceeds the cap.
- **The user is the judge.** Reviews (yours, other agents') are advisory; nothing lands
  without their nod.
- **Self-check before writing**: load the matching expert skill
  (`.agents/skills/expert-*`) and run its checklist against your plan. After the slice
  lands, a **fresh** review instance reads the diff — `pi -p` with the expert's checklist —
  before the user judges. Fresh eyes, no sunk cost.
- **Discuss before writing docs or designs.** Decisions get talked through first; the repo's
  docs are contracts, not transcripts.
- **When something lands, update `docs/status.md`** (and any contract doc that changed).
  When a design decision is revised, keep the old section with a dated note.

## Commands

```sh
cargo check --workspace     # must be clean
cargo test  --workspace     # app_host's suite includes the kanban round-trip — keep it green
cargo run   -p shell2       # the shell; OSVAULD_DATA_DIR=<dir> for a throwaway store
```

## Expert routing

| diff touches | expert skill |
|---|---|
| cross-crate, new patterns, docs claims | `expert-architect` |
| `runtime/` | `expert-runtime` |
| `app_host/`, `lua_tree/` | `expert-app-host` |
| app `.lua` files (`shell2/src/kanban/`, `demo_apps/`) | `expert-lua-app` |
| `shell2/` | `expert-shell` |
| `vault/`, `identity/`, `storage/`, `cryptography/` | `expert-secure-core` |
