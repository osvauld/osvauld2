# Search — full-text over app docs, described in Lua

Status: **steps 0–5 built, 2026-10-03; step 6 (default layer) and the guide section open.** A
long-horizon, test-first plan:
§0 lists the end-to-end tests that prove it done; they are written first, then §7's steps make
them pass.

## 0. Done means these pass

Written before the code they test. Each names the step (§7) that turns it green.

**Rust (`cargo test`)**

| # | test | proves | step |
|---|---|---|---|
| T1 | `search`: index two records, drop everything, reopen the vault, query hits | the index persists through the vault | 1 |
| T2 | `search`: after a commit, the raw redb file does not contain an indexed term | nothing reaches disk unsealed | 1 |
| T3 | `search`: upsert replaces, delete removes, title outranks body, `facet:value` filters, `rank = "recent"` orders by `time` | the index API | 2 |
| T4 | `app_host`: kanban-shaped and chat-shaped `index.lua` over a LoroDoc yield the expected records; `key` reads ids from lists; `nil` drops a record | the contract (§4) | 3 |
| T5 | `app_host`: `index.lua` that writes a doc, calls `ui`, errors or loops is refused/stopped, and the app keeps running | the index VM is sandboxed | 3 |
| T6 | `shell2`: Lua writes a card → `flush` → query hits; edit → old term gone, new hits; delete → gone | the chain, through the real flush seam | 4 |
| T7 | `shell2`: a record edit re-runs `fields` once; a change outside `each` re-runs all; an unchanged flush re-runs none | incremental (§5) | 4 |
| T8 | `shell2`: doc written to the vault while the app was closed is indexed when it opens | catch-up after peer/offline writes | 4 |
| T9 | `app_host`: `search.query` from app Lua returns `{doc, id, score, snippet}` for its own item only | the Lua surface | 5 |
| T10 | `shell2`: an app without `index.lua` is findable by item name and by any string in its docs | the default layer | 6 |

**Smoke (`scripts/smoke_search.py`, real shell over the bridge, added to `SMOKES`)**

S1. Sign up, upload `demo_apps/chat` (ships `index.lua` and a search box), send messages by
    driving the UI. `rpc.search(item, "deploy")` returns the right message ids.
S2. Type into the app's own search box; `dump_tree` shows the hit rendered.
S3. Edit and delete a message through the UI; search reflects both.
S4. Lock, unlock: still hits. Kill the shell, restart on the same data dir: still hits, and
    the restart did not re-run `fields` (counter over the bridge).
S5. Every file under the data dir is scanned for a sent word: absent. Encryption, end to end.
S6. `demo_apps/chat/tests/search.lua` passes under `run_tests` — the app tests its own search.

## 1. What

Full-text search over what an app writes into its own state docs. The app ships `index.lua`
saying what a searchable *record* is; the host extracts, indexes and keeps the index current. An
app without `index.lua` still gets a default index (§3).

## 2. Where the index lives

**In the vault, sealed.** Tantivy reads and writes through its `Directory` trait; a
`VaultDirectory` maps each index file to one sealed entry:

```
entry/search/<ws>/<item>/<file>     one tantivy file, sealed like any other record
```

- Writes buffer in memory and seal when the file is finished; reads unseal the whole file.
  Fine at workspace scale; revisit if a single index reaches hundreds of MB.
- Locks and file-watch are in-memory: the shell is the only writer.
- **One index per item.** An app can only query its own index. Workspace-wide search is a
  later fan-out over several indexes, never a shared one.
- Vault stays Loro- and tantivy-free: it stores opaque entries. The `search` crate owns the
  directory and the schema.

## 3. Two layers

**Default (no `index.lua`).**
- App terms: item name, manifest name/description, source file paths — enough to find the app.
- Doc text: every string leaf of every state doc, keyed by its Loro path (`cards/c7/title`).
  Findable, but raw: no titles, no ranking sense, ids and colours are noise.

**Described (`index.lua`).** The app names its records and what in them matters.

## 4. The `index.lua` contract

```lua
return {
  doc  = "board",               -- doc name; "channel:*" matches a family of docs
  each = { "cards" },           -- path to the collection; one record per entry
  key  = function(rec) return rec.id end,  -- lists only: stable id from the record, never index
  fields = function(id, rec, doc)          -- doc = the whole doc, for joins
    return {
      title = rec.title,        -- boosted
      body  = rec.notes,
      facet = { column = rec.col },        -- exact, filterable: column:<id>
      time  = rec.sent_at,      -- optional: sortable, range-filterable
    }                           -- return nil to drop the record from the index
  end,
  rank = "relevance",           -- or "recent"
}
```

- Runs in a **separate read-only VM** (same shape as the test VM in `lua-app-tests.md`): no
  `ui`, no doc writes. A broken `index.lua` leaves the app running and the index stale.
- Whatever `fields` does not return is not indexed.
- **Index ids for anything that changes; resolve names at display.** A chat indexes the
  author's DID and the channel id, not their display names — a rename then re-indexes
  nothing.

## 5. Keeping it current

*Revised 2026-10-03, before any code: the first draft read changed paths off Loro events. List
paths are positions, which shift under concurrent inserts, and an event never fires for a doc
written while the app was closed. Fingerprints answer both.*

Indexing hangs off the shell's `flush` — the one seam every writer (app, bridge, peer) passes
through on its way to the vault. For each flushed doc that `index.lua` covers:

- **Fingerprint every record in Rust** (hash of its deep value) — no Lua, cheap at 50k.
- Changed or new fingerprint → re-run `fields` for that record, upsert. Gone → delete.
- **Fingerprint the rest of the doc** (everything outside `each`). Changed → a join may be
  stale: re-run every record of that doc. The "ids not names" rule keeps this rare.
- Fingerprints are stored sealed beside the index (`entry/search/<ws>/<item>/fp/<doc>`), so a
  restart re-runs nothing, and opening an app after an offline write catches up (T8).

Indexing runs on the UI thread at first (the Lua VM is `!Send`); a full re-index of a large
doc is the case to measure, and the first to move off-frame if it drops frames.

## 6. Querying — Lua and the bridge

```lua
local hits = search.query("from:did:… in:general deploy", { limit = 20 })
-- { { doc = "channel:general", id = "m91", score = 3.2, snippet = "…" }, … }
```

Hits carry `(doc, id)`, so the app opens straight to the record. `app_host` stays vault-free:
the shell hands it a query closure, the way it hands it `Resolve` for docs. The bridge gets a
`Search { item_id, query }` request so smokes and agents can ask too.

## 7. Steps

Each step: write its tests from §0, see them fail, build until green, `cargo test --workspace`
and `scripts/smoke.py` stay green.

0. Write `demo_apps/chat` (with `index.lua`, search box, `tests/search.lua`) and
   `scripts/smoke_search.py` — S1–S6 fail, as they should.
1. `search` crate: `VaultDirectory` over `vault` entries. T1, T2.
2. Index API: schema (title boosted, body, facets, time), records keyed `(doc, id)`, `upsert`,
   `delete`, `query` with `facet:value` and rank. T3.
3. `app_host::index`: load `index.lua` in a sandboxed read-only VM, walk `each`, apply `key`,
   call `fields`. T4, T5.
4. Shell wiring at `flush`: fingerprints, incremental re-run, catch-up on open. T6–T8.
5. `search.query` in Lua + bridge `Search`; `lua-apps.md` section. T9; S1–S6 go green.
6. Default layer. T10.
7. `status.md`, `architecture.md` crates table, this doc's status line.

## 8. Worked examples

**Kanban** — `doc = "board"`, `each = {"cards"}`, title = card title, body = notes, facet =
column id. Renaming a column re-indexes nothing.

**Chat** — `doc = "channel:*"`, `each = {"messages"}` (a list, so `key`), body = text, facets
= channel id, author DID, `has:file`; `time = sent_at`; `rank = "recent"`. Appends are one
upsert each.
