# Gap log — what apps hit that the Lua layer could not do

The deliverable of `six-apps.md`. One entry per wall, **written when it is hit, before the
workaround** — the workaround is what destroys the evidence.

Verdicts: **no door** (the runtime can do it, nothing reaches Lua) · **missing** (nothing exists)
· **wrong shape** (reachable, but the shape fights the app).

---

## App 1 — pomodoro (`demo_apps/pomodoro`, 2026-09-19)

Written from `docs/lua-apps.md` alone. Six entries; the first two shaped the whole app.

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

### Tally so far

| verdict | count |
|---|---|
| missing | 4 |
| no door | 1 |
| wrong shape | 1 |

Too early to read. The thing to watch is whether 1.1 and 1.2 recur in app 2 — two apps wanting
the same two things is the evidence `six-apps.md` §4 is waiting for, and a third would settle it.

### Not a gap

Recorded so the next author doesn't re-litigate them:

- **A zero-width child is fine.** Guarded against it on the assumption layout would reject
  `w = 0`; tested, and it renders as an empty box with no error. Guard removed.
- **Declarative hover and press did everything asked of them.** `hover_fill`, `tint` and
  `press_scale` needed no per-frame code, which is the half of the animation story that is
  genuinely finished.
