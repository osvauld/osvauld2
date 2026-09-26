# Osvauld

**Xnet is an extended internet for software that groups own. Osvauld is its protocol.
Sthalam is the browser you open it in, and its pages are Lua apps.**

The web made documents addressable. Xnet does the same for shared, private software: a
workspace carries its own identities, data, application code and authority, and it runs on a
node its owners control rather than on a platform's servers. That node can be a Raspberry Pi
or a used laptop — no data centre, static IP or domain required.

| the web | Xnet |
|---|---|
| the internet you browse | **Xnet** — the whole system |
| HTTP, cookies, origins | **Osvauld** — identity, signed role tokens, CRDT sync (`courier`) |
| Chrome | **Sthalam** — the browser, on our own GPU engine (`shell2`, `runtime`) |
| a web server | **kunki** — a sovereign node: a Raspberry Pi, a used laptop, any spare machine |
| a page (HTML + JS) | a **Lua app** — sandboxed Luau describing UI over shared documents |
| a backend database | **Loro CRDT documents**, merged locally and synced through the node |

## A page is a Lua app

```lua
local t = doc:open("tally")                 -- a shared, synced document

if not t.count then t:set({ "count" }, doc.map({ n = 0 })) end

return function()                           -- view = f(state), rebuilt every frame
	local n = t.count and t.count.n or 0
	return ui.col({
		center = true, gap = 12,
		ui.text({ tostring(n), font_size = 72 }),
		ui.button({
			id = "inc", pad = 10, radius = 8, fill = "#2f81f7",
			ui.text({ "+1" }),
			on_click = function() t:set({ "count", "n" }, n + 1) end,
		}),
	})
end
```

No server, API, database adapter or sync code. The app describes the screen and writes to a
document; the protocol carries the write to everyone allowed to see it.

**Built for agents.** An AI writes and edits these apps over a local socket: it uploads
source, applies revision-checked edits to a running app, reads the element tree and console,
clicks, drags, types, and takes screenshots — windowless, on a virtual clock. The Lua surface
is strict (an unknown prop is an error, never a silent no-op) so an agent's mistakes are loud.

## What works today

- **Sthalam, the browser** — our own renderer (winit, wgpu, Vello, Parley, Taffy): layout,
  vector paint, Indic-capable text shaping, scroll, drag and drop, overlays, animation, zoom.
- **Lua apps** — sandboxed Luau VMs, multi-file apps, hot reload that keeps the running app
  alive if new code fails, 2D vector graphics with shape hit-testing, an experimental 3D view.
- **Identity and storage** — BIP39 mnemonic, `did:key` identities, one encrypted store per
  account.
- **The Osvauld protocol, first cut** — claim a node, publish a workspace, invite a member
  with a signed role token, sync and push CRDT changes between two browsers through a node.
- **The agent bridge** — `osvauld-rpc` on a `0600` Unix socket, with a Python client.

**Not yet:** the network transport. It exists in the earlier Osvauld codebase and is being
ported; until then sync runs over a local socket. The protocol is transport-agnostic by design
— `courier`'s handlers are pure message transitions — so a network transport plugs in behind
it without changing the protocol. Also still to come: the full permissions model, tables,
charts, a document editor and a canvas. See
[`docs/status.md`](docs/status.md) for the candid line.

## Try it

Needs Rust (edition 2024) and Python 3.

```sh
cargo run -p shell2                                   # open Sthalam
python3 scripts/upload_app.py demo_apps/kanban        # upload an app and open it
python3 scripts/upload_app.py --keep demo_apps/pie    # --keep reuses a local store
python3 scripts/demo_sync.py                          # two browsers + a node, syncing
python3 scripts/smoke.py                              # the end-to-end smokes
```

`OSVAULD_DATA_DIR=<dir>` points the browser at a throwaway store;
`OSVAULD_OFFSCREEN=900x700` runs any script without a window.

Demo apps live in [`demo_apps/`](demo_apps): `kanban` (the reference app), `dashboard`,
`pie`, `line_chart`, `frame_orbits`, `model_viewer`, `tank`, `pomodoro`, `node_graph` and
more.

## The repo

| crate | role |
|---|---|
| `runtime` | the UI engine: element tree → layout → vector paint, input, animation |
| `app_host` | the Lua layer: sandboxed VM, `ui.*`, `gfx.*`, `doc:open` |
| `shell2` | Sthalam, the browser: accounts, workspaces, tabs, the agent bridge |
| `courier` | the Osvauld protocol: tickets, role tokens, publish, invite, sync |
| `kunki` | the sovereign node |
| `vault`, `identity`, `cryptography`, `storage` | keys and the encrypted store |
| `workspace` | resource addresses and scopes |
| `osvauld-rpc` | the agent bridge's wire format |
| `lua_tree` | Luau parse/print groundwork for structural edits |

Start with [`docs/architecture.md`](docs/architecture.md), then
[`docs/lua-apps.md`](docs/lua-apps.md) to write an app.

## Links

- Site: [osvauld.com](https://osvauld.com) · Docs: [docs.osvauld.com](https://docs.osvauld.com)
- Supported by grants from FOSS United, Zerodha and Kerala Startup Mission (KSUM).
