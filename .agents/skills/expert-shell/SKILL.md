---
name: expert-shell
description: "Reviewer for the osvauld2 shell (shell2/) — accounts, workspaces/items, upload, tabs hosting LuaApps, and (when built) the bridge. Use when a diff touches shell2/src."
---

# Shell review

Read the diff cold. Output `BLOCKER` / `SHOULD` / `NOTE`; you advise, the user judges.
`shell2/src/main.rs`'s `//!` header and `docs/architecture.md` are the context.

1. **One running instance per item.** A tab is a *reference* into `apps`, never a container
   that owns a `LuaApp`; opening an already-open item focuses its entry. A second VM for one
   item is a BLOCKER.
2. **Id namespacing.** The retained store is keyed by `Id` alone — every id an app mints is
   prefixed `tab:<item_id>:` at the walk. Two open apps sharing a field (same caret, same
   scroll) presents as a runtime bug; a change that drops the prefix is a BLOCKER.
3. **Threading + wakeup.** The loop is on-demand; `ControlFlow::Wait` means anything that
   finishes off-frame (Argon2 worker, doc subscriber, future bridge) must arrive as a
   message over the `EventLoopProxy` or the window never repaints. A silent-non-repaint
   path is a BLOCKER. Slow work stays off the UI thread; `Vault` is `Clone` (state behind
   one mutex) so workers may hold a clone.
4. **Upload contract**: folder → `*.lua`/`*.osv` read relative, `/`-separated paths; root
   `main.lua` required; name from `manifest.osv`'s `app "<name>"` else folder name; errors
   rendered, not swallowed. Source becomes a Loro `files` map — plain-bytes keys are not a
   thing.
5. **DocChanged stays payload-free** — any user event repaints; `view()` learns staleness
   from the doc counters, not from the message.
6. **When the bridge lands** (status item 1): the bridge thread is *pure transport* — every
   request executes on the UI thread inside `update` via `Msg::Rpc(req, reply_tx)`. Vault
   mutation on the bridge thread is the sthalam pattern and a BLOCKER.
7. Screens are `runtime::El<Msg>` descriptions — no direct winit/wgpu calls here, no
   `custom()` scene hacks where a prop exists.
