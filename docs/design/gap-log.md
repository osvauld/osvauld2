# Gap log — what apps hit that the Lua layer could not do

The deliverable of `six-apps.md`. One entry per wall, **written when it is hit, before the
workaround** — the workaround is what destroys the evidence.

Verdicts: **no door** (the runtime can do it, nothing reaches Lua) · **missing** (nothing exists)
· **wrong shape** (reachable, but the shape fights the app).

---

## App 1 — pomodoro (`demo_apps/pomodoro`, 2026-09-19)

Written from `docs/lua-apps.md` alone. Six entries; the first two shaped the whole app.

> **Correction, 2026-09-20.** Entries 1.4 and 1.5 were written without checking `scripts/`, and
> they overstate. A second driver exists — the bridge (`scripts/osvauld/client.py`,
> `scripts/drive.py`) — and it already has most of what they ask for: `rpc.click(item_id, el_id)`
> clicks **by element id**, plus `dump_tree`, `read_console`, `read_data`, `type_text`, `key`,
> real `screenshot`, `write_file` + `reload_item` for a live Lua edit, and a persistent REPL
> against one running app.
>
> It drives `shell2`, which always opens a window (`Session` spawns the binary; no offscreen flag
> exists in `shell2/src/main.rs`). So both entries change verdict from **missing** to **no door**,
> and the door is a specific one: *the good driver only exists on the windowed path, and the
> headless path has coordinates and cold starts.* See §Tooling below.

### 1.1 A click cannot read the clock — **missing**

**Wanted:** "start counting 25 minutes from now", in `on_click`.

**Had to:** the click can only record an intent (`S.pending = "start"`), and the next `on_frame`
— which is handed `e.elapsed` — turns it into a deadline. Three extra pieces of state, a branch
at the top of the tick, and a visible consequence: for one frame after pressing Start the button
still reads "Start", because the label follows `S.running` and nothing can set it until a frame
has told the app what time it is.

**What it would take:** carry the stamp the runtime already computes. `event_at` is set at
`lib.rs:660` for exactly this and already rides `DragEvent.t`; `on_click` and `on_hover` just
don't carry it. This is the cheapest entry in the log.

### 1.2 No wake at a time, and repaint is all-or-nothing — **missing**

**Wanted:** repaint once a second while counting, and be woken when the deadline passes.

**Had to:** declare `on_frame` for as long as the timer is live, which repaints at the display's
rate — sixty frames to move one digit — and drop the handler when paused, since its presence *is*
the repaint request. Two unrelated things are therefore the same switch: "do I need the clock" and
"do I need to repaint". A paused timer is correct only because those two happen to coincide here,
and they will not in the next app.

Worse than the cost: the countdown only advances while painting. A window that stops being painted
is a pomodoro that quietly stops counting.

**What it would take:** `six-apps.md` §4 — declared deadlines serviced by `ControlFlow::WaitUntil`
(`lib.rs:1452` is a bare `Wait`). This is the entry that plan is parked behind.

### 1.3 The guide contradicts itself on `on_frame` — **wrong shape** (doc)

**Wanted:** to know `on_frame`'s signature.

**Found:** §Handlers documents `on_frame(e)` with named fields. §Animation says
"`on_frame(dt, elapsed)` runs Lua after each presented frame" — the old positional form. Following
the second produces exactly the failure §Handlers warns about three paragraphs earlier: a
positional signature that binds the wrong values *and keeps running*.

**What it would take:** one line in `docs/lua-apps.md`. Logged rather than fixed in passing,
because a guide that disagrees with itself is the authorability finding this plan exists to
measure, and quietly patching it would have deleted the evidence.

### 1.4 An app cannot find out where anything landed — **missing** (tooling)

**Wanted:** to click my own Start button in a headless run.

**Had to:** sweep a grid of coordinates across ~15 separate runs until the view changed. Layout
readback is documented as not exposed, and `open --tree` prints the element tree without rects, so
there is no way to ask where an element is from either side. The first guess was wrong in a way
worth recording: the click landed in the 10pt gap *between* the two buttons, and an
"unchanged" result says nothing about whether you missed by 5pt or 200.

**What it would take:** rects in `--tree` output. The layout is right there when it prints.

### 1.5 `open` cannot advance the clock — **no door**

**Wanted:** to watch a 25-minute timer roll over into a break.

**Had to:** copy the app to a scratch folder and shrink `work_mins` to 0.02. At 1/60s per frame a
real session is 90,000 frames.

**What it would take:** `Headless::advance(secs)` already exists and was built for exactly this
("a wait an app is supposed to notice — a debounce, a toast that dismisses itself"). `open` has no
flag that reaches it. An `--advance SECS` action is about five lines.

### 1.6 No type definitions for the sandbox globals — **missing** (tooling)

**Wanted:** to write Lua with an editor that knows what `ui`, `doc` and `gfx` are.

**Had to:** ignore an "undefined global" warning on every single reference to them — ten in this
app, and it is a small app.

**What it would take:** a `.luarc.json` and a definitions file for the eight constructors, the
prop list and `doc`. It is also the cheapest possible authoring aid for an agent, since the prop
table in the guide is already the content.

---

### Tooling — one capability, two drivers, and the split between them

Added 2026-09-20 after the correction above. Two ways to drive an app exist, and neither is whole:

| | `open` (`app_host/examples/open.rs`) | the bridge (`scripts/`) |
|---|---|---|
| window | none | **always** — `shell2` has no offscreen mode |
| address an element | pixel coordinates | **by `id`** |
| session | one action list, then exit | **persistent REPL** (`drive.py`) |
| clock | `--frames N`, virtual and exact | real time |
| sees | element tree, console | tree, console, doc data, **real pixels** |
| edit Lua live | no | **`write_file` + `reload_item`** |

The interesting part is that the columns are almost complementary. The bridge has every affordance
worth having except a headless mode; `open` is headless and has none of them. So an agent that
must not open a window — which is the normal case for an agent — is left on the weaker driver, and
pays for it in cold starts and guessed coordinates.

Two ways to close it, and they are not equivalent:

- **Port the affordances into `open`** — `--click-id`, `--advance`, a tree diff. Perhaps 60 lines,
  no runtime change, but it makes a second driver that will drift from the first.
- **Give `shell2` an offscreen mode** so the bridge drives it without a window. Bigger, and it
  ends with *one* driver that has by-id addressing, screenshots, hot reload and a REPL — the same
  path the MCP bridge gives an agent, which is the one that should be good.

The second is the better end state and the first is what unblocks app 2 this week. Worth deciding
deliberately rather than by whichever gets written first.

> **Closed, 2026-09-20.** The second was taken. `shell2 --offscreen WxH` runs the bridge's driver
> with no window, real pixels, and a virtual clock that advances 1/60s per delivered request —
> so the `window` and `clock` rows above no longer split the two columns, and `open` has nothing
> left that the bridge lacks except the pointer pipeline, which is step 2.
>
> **1.4 and 1.5 are therefore answered, but not the way they asked.** 1.4 wanted rects in
> `--tree`; what it gets instead is that coordinates stopped being how you address anything —
> `rpc.click(item, "toggle")` uses the id the app already declared. 1.5 wanted `--advance SECS`;
> what it gets is that 120 requests *is* two seconds, exactly. Both entries stay in the log as
> written: an entry describing a wall the author actually hit is still true after the wall moves,
> and rewriting them to match the fix is how a log stops being evidence.
>
> One bound is new and worth its own line, because it is the kind of thing that is discovered at
> the worst moment otherwise: **offscreen is windowless, not headless.** `EventLoop::build()`
> fails with no `DISPLAY`/`WAYLAND_DISPLAY`, so a container or CI runner needs `xvfb-run` until
> the event loop itself is replaced. See `six-apps.md` §7, "Step 1, as built".
>
> **1.5 closed for real, 2026-09-20.** `Advance(secs)` landed as a bridge op: `rpc.advance(25*60)`
> runs a whole pomodoro session to completion in one request, asserted in
> `scripts/smoke_offscreen.py`. The entry's own suggestion — an `--advance SECS` flag on `open` —
> was deliberately not taken; see §7.
>
> **1.4 is still open, and now the reason is precise.** Rects looked like a small addition to
> `DumpTree` and are not: `DumpTree` reads `app.view().info()`, a fresh view that has never been
> laid out. Layout lives in `Runner::frame()` and its results never leave the `Runner` — the same
> wall that stops `Pointer`. One cause, two symptoms. Slice 2b.

### Tally so far

| verdict | count |
|---|---|
| missing | 2 |
| no door | 3 |
| wrong shape | 1 |

Too early to read, and the one number that moved was moved by *checking the repo instead of
reasoning* — which is the same lesson `six-apps.md` §0 records. The thing to watch is whether 1.1
and 1.2 recur in app 2; two apps wanting the same two things is the evidence §4 is waiting for,
and a third would settle it.

### Not a gap

Recorded so the next author doesn't re-litigate them:

- **A zero-width child is fine.** Guarded against it on the assumption layout would reject
  `w = 0`; tested, and it renders as an empty box with no error. Guard removed.
- **Declarative hover and press did everything asked of them.** `hover_fill`, `tint` and
  `press_scale` needed no per-frame code, which is the half of the animation story that is
  genuinely finished.
