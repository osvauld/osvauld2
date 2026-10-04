# App threads — one thread per app, the main thread is the chrome

Status: **plan, nothing built (2026-10-04).** A long-horizon, test-first plan: §0 lists the
end-to-end tests that prove it done; they are written first, then §6's steps make them pass.
Activates `runtime-rebuild-plan.md`'s "app-per-thread actors when multiple simultaneous apps
… demand it" — tiling and background sync are that demand.

## 0. Done means these pass

Written before the code they test. Each names the step (§6) that turns it green.

**End-to-end (`scripts/e2e_app_threads.py`, offscreen shells, real socket, real Lua VMs,
real vaults).** Wall-clock assertions measure the *bridge round trip* and
`OSVAULD_FRAME_TIMINGS` phases, never the virtual clock.

| # | scenario | proves | step |
|---|---|---|---|
| T1 | App A's `view` busy-waits 300 ms every frame (under the kill budget). Chrome tab switch, a click in app B, and `Rects` each answer in < 50 ms | a slow app costs only its own tile | 3 |
| T2 | App A's handler spins until killed. Its tile keeps its last frame, then shows the budget error card; chrome and B stay live throughout; shell process survives | wall-time budget, per tile | 1, 3 |
| T3 | App A allocates without bound. A's tile shows a memory error; B and chrome unaffected; shell process survives | memory cap, per tile | 1 |
| T4 | App A blocks forever in native code (test-only `__stall` binding). After N s the chrome badges A "not responding"; closing A's tab works; B unaffected | watchdog + stuck thread containment | 8 |
| T5 | Two shells + node. Chat is open in B's shell but **not focused**; A sends. B's chat tile, read via `DumpTree` without focusing, already shows the message | background apps process pushes | 6, 8 |
| T6 | An idle open app receives no wakeups for 5 s (wake counter via `ReadConsole`/dump) | event-driven: idle = zero CPU | 8 |
| T7 | Two tiles side by side, each with a ~40 ms view. One frame of both finishes in well under 80 ms wall | apps use separate cores | 9 |
| T8 | `edit_file` + `ReloadItem` on a running app still hot-reloads, keeps `ui.state`, and a bad edit shows the banner — through the threaded path | reload over the boundary | 5 |
| T9 | `search.query` from an app returns hits for docs written while it was in the background | index lives with the app | 7 |
| T10 | An app asks for its own window; the chrome opens it, the app renders there with no code change; closing the window docks it back as a tile | tile vs window invisible to apps | 10 |
| T11 | The chrome VM given a hot loop is killed by the strict chrome budget; the Rust fallback chrome appears | the one shared VM is guarded hardest | 11 |

**Unchanged:** every script in `scripts/smoke.py`'s `SMOKES` passes without edits after
every step. They are the proof that driving, rects and pixels still mean the same thing.

**Rust (`cargo test`):** each step lands unit tests in the crate it touches (`runtime` tile
compositing and input translation, `app_host` budget/memory, `shell2` routing), named in the step.

## 1. Why

Today every `LuaApp` lives in `shell2`'s one `App`, on the winit thread (`OpenApp` in
`shell2/src/main.rs`). Only the focused tab's `view()` runs, so:

- one slow or stuck app freezes the chrome and every other app;
- a background app's docs receive node pushes, but its Lua never runs until focused, and a
  closed app's push is dropped;
- tiling would put several apps on one core.

The guard today is a 1 M-interrupt count reset per `view`/`update` (`app_host/src/lib.rs`
`sandboxed_vm`). It catches a Lua loop, not a long native call, not memory, not a stack
overflow through native frames (`modules.rs`'s `require`-cycle note), and it allows a
~100 ms+ frame without tripping.

**Threads, not processes.** Browsers use processes because they run hostile native code and
need a crash boundary. Ours is sandboxed Luau; a memory cap and a wall-time budget cover what
a process would buy, without serializing every frame or splitting doc ownership across an
address space. Revisit only if apps ever load native code.

## 2. Shape

```
app thread × N (one per open app)              main thread (winit)
 Luau VM + its LoroDocs + its search index      chrome (Lua launcher / Rust strip), tiling
 Tile runtime: view → layout → hit → paint      compositor: append each tile's latest Scene
   → vello Scene ──────────────────────────▶      at its rect / into its window
 ◀── input (tile coords), size, focus, wake,    input routing, focus, IME, clipboard, cursor
     bridge requests, sync bytes                 one GPU submit per window
```

- **Hand-off is a move, not IPC.** The tile sends an owned `vello::Scene` plus its hit rects;
  the compositor does `scene.append(&tile, Some(transform))`. Nothing is serialized.
- **Latest frame wins.** The compositor never waits; a late tile shows its previous Scene.
  *(Decided 2026-10-04.)*
- **Event-driven.** An app thread blocks on its channel and wakes only for input, a node push,
  a bridge request, or a due timer/animation — the same `ControlFlow::Wait` discipline the
  runtime already has. Idle costs memory, not CPU. Hidden apps stay alive; a closed app's
  thread ends. *(Decided 2026-10-04.)*
- **Windows.** An app may *ask* for its own window; the main thread creates it (macOS requires
  windows on the main thread) and the chrome decides. Tile vs window is invisible to the app.
- **Offscreen determinism.** With a virtual clock, a driver op (`Frame`, `Advance`, pointer
  ops, `Rects`, `Screenshot`) is a barrier: the main thread forwards the clock to every tile
  and waits for their frames before answering. "Latest frame wins" applies to real windows only,
  so the smokes stay exact.

## 3. The boundary

Plain data in both directions. No `Rc`, no borrowed `LuaApp`, no closure crosses.

**Main → app (`TileIn`):** input in tile coordinates · resize · focus/blur · wake ·
clock (offscreen) · bridge request (`DumpTree`, `Rects`, `Click`, `ReadConsole`, `EditFile`,
`ReloadItem`, `RunTests`, `AppDataGet`, `Search`…) with a reply channel · sync: build hello /
import bytes / list open docs · close.

**App → main (`TileOut`):** frame (`Scene`, hit rects, overlay layer) · cursor · clipboard
write · window request · dirty docs to persist and sync (bytes) · console lines · error/budget
state · bridge reply.

What moves into the app thread (inventory from the `lua-shell` and `sync-hardening` sessions,
2026-10-04): `view`/`update`; source edit and reload; bridge senses and actions; test helpers;
doc sync/import and `open_doc_names`; `flush` (bytes go out, the vault write stays a worker
job); the search index (`Rc<RefCell<ItemIndex>>` shared with the app binding); later the
ephemeral channel (`net.send`/`net.on`), routed per item into the same queue.

**Chrome rules** (for the `lua-shell` work): launcher capabilities stay copied metadata and
queued plain-data commands (they already are); open-or-focus stays one path and becomes
"spawn or focus an app thread"; launcher senses stay separate from app senses; the chrome
never calls into an app synchronously.

## 4. What the code already gives us

- `runtime` has no global state except `timing.rs`'s env-flag `OnceLock` (thread-safe).
- `Runner` (`runtime/src/lib.rs`) already separates `render: Option<Render>` from its own
  `scene`, `text: TextEngine`, `store`, clock and hits; `Headless` runs a full `Runner` with
  no window. A tile is a `Runner` whose Scene is kept instead of dropped.
- `app_host::Wake` is `Arc<dyn Fn() + Send + Sync>` — wakes already cross threads.
- `LuaMsg` is plain data (`Call(u32)`); `App::Msg` is already `Send`.
- Frame and 3D payloads are `Arc`.

## 5. What is new

- **A tile slot in the runtime:** an element that paints an externally supplied Scene at its
  rect, routes pointer/wheel/key events inside it to a sender, and reports the tile's rects
  offset into the parent's coordinates (so `Rects` stays one combined list).
- **Per-thread text:** each tile's `TextEngine` has its own font context and glyph caches.
  Font *data* is shared; caches are not. Memory per open app goes up — measured in step 2.
- **Overlays past the tile edge:** a tile's overlay layer goes to the compositor separately
  and is painted above all tiles, clipped to the window.
- **Cross-tile drag** is a chrome-mediated protocol. Out of scope here (§7).

## 6. Steps

Each leaves `cargo test` and the smokes green.

0. **Tests.** `scripts/e2e_app_threads.py` with T1–T11 and the test-only Lua bindings they
   need (`__stall`, a wake counter), all failing for the right reason.
1. **Guards, on today's code.** Per-VM `set_memory_limit`; the interrupt checks elapsed wall
   time against a budget instead of counting. T3 green; T2's kill half green.
2. **Tile runtime.** Split `Runner` into a windowless `Tile` (layout, hit, paint, clock,
   state, text) and the windowed driver; `Headless` becomes a `Tile`. Add the tile slot
   element and input translation. Verify `vello::Scene` is `Send`. No threads yet.
3. **App thread, focused app only.** `OpenApp` becomes a handle (`TileIn` sender, latest
   frame, ids); `LuaApp` + `Tile` live on the thread. T1, T2 green.
4. **Bridge through the boundary.** Senses and actions become `TileIn` requests with reply
   channels; offscreen barrier. All smokes still green unchanged.
5. **Source edit and reload** on the app thread. T8 green.
6. **Docs and sync.** Hello/import/list/flush cross as bytes; node push routes to the app
   thread by item id; persistence stays a worker. T5's sync half green. Coordinates with
   `group-chat-sync.md` (ephemeral channel lands in the same queue).
7. **Search index** moves into the thread; the indexer runs there. T9 green.
8. **Background apps live.** Unfocused tiles keep running on events; watchdog badges a
   thread that stops answering. T4, T5, T6 green.
9. **Two tiles.** A minimal side-by-side split in the Rust chrome (the Lua chrome takes it over
   later). T7 green.
10. **Windows.** App window requests, compositor per window, dock back. T10 green.
11. **Chrome VM guards.** Strictest wall-time budget + memory cap on the launcher VM, Rust
    fallback on kill. T11 green.

## 7. Open

- **Font caches per thread** — memory cost at 10+ open apps; a shared read-only glyph atlas
  may be needed.
- **Cross-tile drag/drop** and clipboard formats beyond text.
- **IME** forwarding: composition events to the focused tile; the bridge's `Keyboard` does not
  cover native IME today.
- **Frame pacing:** whether the compositor should re-present when only one tile changed, or
  per-window damage.
- **Hung native call** (T4): a stuck thread cannot be killed safely; the plan is to abandon it
  (detach, drop its handle, leak until exit). Acceptable only because Luau has no blocking I/O.
