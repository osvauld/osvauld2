# Orbit field — friction log

Written while building `demo_apps/orbit` against `docs/lua-apps.md` alone, apart from the two
excursions flagged below (`docs/design/frame-hits.md` once, and `runtime/src/lib.rs` once — each
because the guide left a question the app could not answer by itself). Ordered worst first.
Everything here is a gap in the *contract*, not a complaint about the app.

## 1. ~~A drag also fires the click~~ — wrong, and worth keeping as a lesson

**Corrected 2026-09-19, after the fact, by running it.** The claim below is false. A press that
travels past the 5pt slop clears the armed click at that moment (`runtime/src/lib.rs`, in the
drag-start branch), so a drag fires no click at all. The guard this app carried has been removed,
along with the `quiet_click` clock and the `g.moved` flag that fed it.

What I actually saw was the *other* case: a press that travels **less** than 5pt stays a click
and reports where it was **released**, not where it was pressed. Since no hand is perfectly
still, that is the ordinary case, and it looks exactly like "a drag fired a click" from inside
Lua. Both behaviours are now tests in `runtime/src/tests.rs`.

The reasoning error is the instructive part. I read `on_cursor_release` carefully and quoted it
correctly — the release path really does fire the armed click unconditionally — but a gesture is
not one function, and I never looked at what the *move* path does to `self.pressed` on the way
there. Reading one end of a state machine and generalising is how a careful read produces a
confident wrong answer. The fix is not to read harder; it is to be able to run the thing, which
is friction #5 and was the real blocker behind this entry.

The original entry follows, unedited, because the mistake is the point:

> Undocumented, and it silently breaks the most obvious pairing in the guide. This app puts
> `on_click` (pin a sprite) and `on_drag` (throw a sprite) on the same `ui.frame` [...] Throwing
> a sprite also pinned one. So: the press arms a click, the drag does not disarm it, and the only
> thing that can stop it is releasing outside the element. The app has to defend itself.

## 2. Handler arity is unchecked, so a wrong signature is a silent no-op

`on_drag` takes eleven positional parameters. To reach the three that this app is built around —
`shape, sx, sy` — you must spell out `scale, origin_x, origin_y` first, which this app never uses
and which the guide itself says "only a root-level ghost placing itself in screen space needs".

If you drop them:

```lua
on_drag = function(phase, x, y, dx, dy, shape, sx, sy)   -- three short
```

...then `shape` is silently bound to `scale` (a number), `F.by_id[shape]` is `nil`, and nothing
can ever be grabbed. There is no error. I ran exactly this through the harness:

```
frame#field  [on_click on_drag on_hover]

5 elements, 4 handlers
console: clean
```

Exit code 0. Every check the runtime and the harness can make says the app is correct. This is
the single easiest way to write a broken app in this system, and it is a trap an agent walks into
at speed, because eleven positional parameters in a documented order is precisely the kind of
thing that gets truncated when you are copying from memory instead of from the table.

Two things would help, in order of value:

- **Pass handlers a table**, the way Rust models these internally (`DragEvent { phase, at, delta,
  shape, … }`). `function(e) … e.shape … end` cannot be mis-ordered, cannot be truncated, and
  survives a new field being added to the event. The guide's own kanban examples already route
  everything through `msg.phase` / `msg.dx` tables one layer up, so the destination shape is
  familiar.
- Failing that, **check the declared parameter count** and refuse a handler that declares fewer
  than the event supplies. Luau can see the arity; a red box saying `on_drag takes 11 arguments,
  this function declares 8` would convert a silent misbehaviour into a one-second fix.

## 3. The headless harness can't reach any code behind a pointer

`cargo run -q -p app_host --example open -- demo_apps/orbit` builds exactly one frame, with no
pointer anywhere. So `console: clean` certifies the idle view and nothing else — and, per #1 and
#2, it certifies it while the app's central gesture is broken.

Everything in this app that matters is behind the pointer: `grab_at` / `hold_to` / `release`, the
fling integrator, the three highlight branches, and the whole readout line. None of it runs in a
clean pass. I verified it by copying the app to `/tmp` and appending a driver at module scope —

```lua
M.grab_at(M.sprites[9], 5.5, 6.25)
M.step(0.016)
M.hold_to(120, -95)
M.step(0.016)
M.release()
for _ = 1, 400 do M.step(0.016) end
```

— which is why the physics lives in `field.lua` behind three named functions rather than inside
the `on_drag` closure: **the shape of the app was decided by what the test harness can reach.**
That is a real pull on architecture coming from a gap in tooling, and worth knowing about.

It also still leaves the premise of the demo unverified. That the runtime hit-tests a *moving*
instance, returns shape-local coordinates with the instance transform undone, and holds the
grabbed shape across a gesture, is exactly what `open` cannot check. A synthetic pointer would
close it:

```
cargo run -p app_host --example open -- demo_apps/orbit --hover 400,300 --press 400,300 --move 480,240 --release
```

...printing the `shape, sx, sy` the runtime actually delivered at each step before rebuilding the
view. That one flag would have caught both #1 and #2 in seconds.

(This was also the first of my two excursions outside the guide: I read
`docs/design/frame-hits.md` to confirm the "an id'd instance is one target; ids **inside** a
reused visual are not reported" rule before committing a 200-instance app to it. The guide does
state it. I wanted the design doc's `landed 2026-09-18` status line, because the guide carries no
such marker on an experimental feature.)

## 4. There is no clock, so a throw cannot be measured without the frame tick

`now()` is listed in the sandbox section as "unix seconds", and that is exact: it returns a whole
number. In the app:

```lua
print("now() resolution:", now(), now() - math.floor(now()))
-->  now() resolution:  1789748438  0
```

One-second granularity is the only clock an app has. `on_frame`'s `elapsed` is monotonic and
sub-second, but it is an argument, readable only inside that one handler — you cannot time a
region of your own code, and you cannot time anything at all from inside a pointer handler.

That bites immediately here, because **`on_drag` carries no time and no velocity.** It gives
cumulative `dx, dy` since the press, and a throw needs a rate. Several `"move"` events can arrive
between two frames and there is nothing on the event to tell them apart. What the app ends up
doing is differencing position on the `on_frame` tick, since `dt` is the only duration in the
system:

```lua
-- field.lua, in step()
local g = M.grab
if g and dt > 0 then
	g.vx = (s.cx - g.px) / dt
	g.vy = (s.cy - g.py) / dt
	g.px, g.py = s.cx, s.cy
end
```

It works, and it is arguably the right place for it, but it means **no app can implement a fling
without also declaring `on_frame`** — an unstated dependency between two features the guide
presents as unrelated. It also means a double-click, a press-and-hold, a "dismiss after 3s" toast
and a debounce are all currently unwritable. A monotonic `now_ms()` in the sandbox list would
unlock all of them; velocity on the drag event would be the targeted fix.

## 5. The frame item budget is undocumented, and the error doesn't carry the numbers

`docs/lua-apps.md` documents the path-command ceiling in the `gfx.path` row ("up to 65536") and
says nothing anywhere about a limit on items in a frame. Pushing `count` up found one:

```
View error: Frame expands to more than 4096 items
stack traceback:
	[C]: in ?
	[string "main.lua"]:124: in ?
```

The line number is right (`visual = gfx.frame(items)`), so the traceback earns its keep. The
message does not. It doesn't say how many items this frame *did* expand to, it doesn't say what an
"expanded item" is, and — the part that actually matters here — it doesn't say that **an instance
costs `1 + (items in the visual it places)`**. My sprite visual is two fills, so each placement
costs 3, and I had to binary-search the ceiling to learn that:

| sprites | result |
|---|---|
| 1365 | `console: clean` |
| 1366 | `Frame expands to more than 4096 items` |

1365 x 3 = 4095. That confirms the accounting, but an author sizing a field of sprites should not
have to discover the rule by bisection — it is exactly the number you want *before* you write the
app. Two fixes, both cheap:

- put the cap in the `gfx.frame` row of the constructor table, with the instance accounting rule
  next to the "placed by its local origin" note in the `gfx.instance` row;
- make the error read the way a budget error should, e.g.
  `Frame expands to 4098 items (max 4096): 1366 instances of a 2-item visual`.

At 200 sprites this app sits at 600/4096, so the requested count was never in danger — I only know
that because I went looking.

## 6. A frame doesn't clip, and the pointer stops at the element's edge

The guide says "Frame dimensions are intrinsic layout claims, not an implicit clip or scale", and
separately, about hover, "An element is hovered while the pointer is inside it". Put those two
together and there is a consequence neither sentence states: **anything your visual draws outside
its own `width`/`height` is painted over the neighbours and is unreachable by the pointer** —
outside the element rect, no event is delivered at all, no matter what shape is under the cursor.

For a field of moving things that is not a footnote, it is a design constraint on the geometry.
The first cut of this app seeded orbit centres across 10–90% of the field with radii up to 226pt,
which put a good fraction of the sprites outside the panel for part of every lap — drawn over the
header text, and dead to the hover that the whole demo is about. The fix is in `field.lua`: a
centre is placed no closer to an edge than its own orbit radius, and the drag clamps to the same
box.

```lua
local function keep_in(s)
	local m = C.margin + s.a
	s.cx = clamp(s.cx, m, C.field_w - m)
	s.cy = clamp(s.cy, m, C.field_h - m)
end
```

I never saw either behaviour — no window — so this is the guide taken at its word rather than an
observation. One sentence in the frame-visuals section would settle it for good: *a visual may
paint outside its declared box; hits are only reported inside the element's rect, so keep your
geometry inside it or clip it yourself.*

## 7. A compiled visual can't survive a frame when only its transforms change

This app is the poster case for the resource model: one immutable sprite, 200 placements. The
sprite path and brushes really are compiled once, at module scope, and that part is a pleasure.
But the *container* — `gfx.frame(items)` holding the 200 `gfx.instance` tables — is rebuilt and
recompiled on every single view, because the transforms live inside it and the transforms are the
only thing that changed.

The guide's framing ("Paths and brushes are compiled once into immutable resources") reads like
the compile cost is a one-time thing, and for a static chart it is. For anything that moves it is
a per-view cost proportional to N, and the guide never mentions that assembly has a cost at all.
The animation section says "avoid allocations in inner numeric loops" in the same breath as
`on_frame`, which is good advice that this API makes impossible to follow: 200 sprites is 200
`gfx.instance` tables plus 200 six-element transform tables per frame, minimum, and there is no
way to write it otherwise.

What I wanted and could not find: a visual that takes a list of transforms (`gfx.instances({
visual = v, transforms = t })`, one resource, one Lua table reused across frames), or any
documented way to hand back a frame I built last tick.

**Measured, so the number is on the record.** I cannot profile the real thing — no window, and
`open` builds one view per process — so I timed `F.step(dt) + view()` in a loop inside a copied
app, `--release`, best of five process runs, minus the same run with a zero-iteration loop:

| sprites | ms per step + view |
|---|---|
| 50 | 0.095 |
| 200 | **0.366** |
| 600 | 1.090 |
| 1365 | 2.505 |

Dead linear, about 1.8 µs per sprite per frame. **At the requested 200 that is 0.37 ms, roughly
2% of a 60 fps budget** — the Lua and assembly side is simply not the problem, and an earlier
guess in this file that extrapolated 3–4 ms was wrong because it was measured on a debug build.
What this does *not* measure is painting and hit-testing, which is where 200 instances might
actually cost something; that still needs somebody with a window.

**On the requested count: 200 shipped, and nothing objected.** I cannot say whether it is
*visibly* heavy, because that needs the GUI. The number where it becomes impossible is 1366 (#5).

A trap to note while timing: the loop above dies past a certain length with

```
runtime error: interrupt budget exceeded
stack traceback:
	[string "app_host/src/gfx.rs:107:9"]:5: in function 'instance'
	[string "main.lua"]:80: in function 'view'
```

That is the "runaway loop is killed" guard, and it names neither the budget nor the limit nor what
it is protecting you from — it reads like an internal assertion, and for a while I mistook it for
a resource leak in `gfx.instance`. Exactly 809 of this app's view builds fit in one budget, which
also means **a single view has a hard instruction ceiling** — worth one line in the sandbox
section, since an app that builds a big enough description will hit it and the message will not
help.

## 8. Does a bare table splice into a `gfx.frame`?

Unanswered by the docs, and the two halves of the guide point opposite ways. "Elements and
children" defines splice groups — "A bare table nested as a child splices its contents into the
parent" — but scopes the whole section to `ui.*` elements. The `gfx` section says "items as
positional children" and then warns that "a stray positional entry in a `fill` or a named key in a
path is an error, not an ignored extra", which sets the expectation that `gfx` is strict where
`ui` is forgiving. It is:

```
runtime error: fill: positional fields are not allowed
```

So I did not risk `gfx.frame({ width = w, height = h, items })`. I built the list and hung the
named keys on it instead:

```lua
items.width = C.field_w
items.height = C.field_h
local v = gfx.frame(items)
```

That is fine, arguably better, and I still don't know whether the other spelling works. One
sentence in the `gfx` section — splices, or doesn't — settles it.

## 9. `fill` and `stroke` take no transform, so "draw this circle over there" needs a group

Only `group` and `instance` carry a `transform`. To ring a sprite that is placed with a scale I
had to wrap a one-item `gfx.group` around a `gfx.stroke` purely to move it, and then undo the
scale on the stroke width by hand:

```lua
gfx.stroke({ path = ring_p, brush = gfx.solid(color), width = C.ring_w / s.k })
```

Both halves are guessy. The group-as-translation is a whole extra node per highlight, and the
division is there because a scaling group multiplies stroke width — universally true in vector
renderers, but this guide is otherwise explicit about exactly this class of gotcha (it warns that
`tint` is milliseconds, that `instance` is placed by its local origin, that a dashed line is one
line to the pointer). Stroke width under a scaled group belongs on that list.

## 10. No arcs means every gfx app rewrites the kappa constant

Documented, so not a surprise — "There is no text inside a frame, no arcs". But the consequence
is that `shapes.lua` exists at all: 45 lines whose entire content is a 4-cubic circle and a
4-cubic rotated ellipse built around `0.5522847498307936`. Every app that draws a round thing will
write that file again, slightly differently, and the ellipse one is fiddly enough to get wrong
quietly (mine needed the rotation folded into the control points, since there is no transform on
a fill — see #9). A `gfx.ellipse(cx, cy, a, b, rot)` path constructor would delete the module.

Worth saying plainly because the pitch is "anything you can describe with coordinates, you can
draw", and the most ordinary shape in the world currently requires a magic number.

## 11. The transform tuple's element order is stated but never defined

`transform = {xx, yx, xy, yy, dx, dy}` is all the guide gives. I assumed a column-major 2x3 affine
— `x' = xx*x + xy*y + dx`, `y' = yx*x + yy*y + dy` — which is the vello/`kurbo::Affine` reading
and almost certainly right. My app cannot tell me: it only ever uses scale + translate, where the
two candidate readings are identical, so a wrong guess would have been invisible here and would
have bitten the first person to rotate something. The multiply-out is one line and makes the
field names checkable.

## 12. Smaller things

- **`print` exists, is undocumented, and does not count as the console.** The sandbox section
  lists what you get beyond plain Lua (`doc`, `ui`, `require`, `now()`, `uuid()`) and `print` is
  not on it, but it works, and in the harness its output lands on stdout *after* `console: clean`
  and does not affect the exit code. That is a useful debug channel and the only one an app has,
  so it should be documented — and the harness should probably say which stream is which, because
  right now a deliberate `print` and an app error look similar and are gated differently.
- **There is no way to decline a drag.** `on_drag` fires `"start"` on a press that landed on no
  named shape (`shape == nil`), and a handler cannot return "not mine" — it has to carry an empty
  grab through the whole gesture. Fine here; would matter for an app that wants a press on the
  background to pan instead.
- **Where does an element's own `fill` paint relative to its `visual`?** I put
  `fill = C.panel, radius = 10` on the `ui.frame` for a backdrop, assuming element paint goes
  under the visual. The props table says every prop works on every element; nothing says the
  z-order between the two. Obvious answer, still an assumption.
- **`on_frame` is listed among the pointer handlers**, so it reads like something that belongs on
  an interactive element. It is on the root `ui.col` here and works fine. The bullet says "Needs
  an `id`" but not "any element will do".
- **No seeded RNG.** `math.random` is there but there is no `math.randomseed` story in the
  sandbox section, and a demo that looks different on every open is a bad demo. `field.lua`
  carries a hand-rolled Lehmer generator for this. The sandbox list would be a good place to say
  what `math.random` does across opens.
- **Error messages are genuinely good** where they exist, and deserve saying so:
  `col#orbit:[91] > [4] > frame#field:[122] > unknown prop clip` gives the path, the line of each
  element on it, and the offending name; `radius: runtime error: expected a number got string`
  names the prop. Anonymous children show as `[4]`, which is one more argument for putting ids on
  things. The failures in #1 and #2 are exactly the ones that produce no message at all.
- **The console-is-the-test contract is good.** Non-zero exit when anything lands on the console
  is exactly right for an iterate-until-clean loop, and the element tree with ids and handlers
  caught more than one thing. The gap is only #3: it tests the idle frame.
