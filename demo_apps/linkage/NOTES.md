# Linkage — friction log

Written while finishing `demo_apps/linkage` (a kinematic arm: nested `gfx.group` rotations,
hover readout in shape-local coordinates, `on_drag` swinging a joint by `atan2(sy, sx)`).
Everything below is a place the runtime or `docs/lua-apps.md` cost me time. The app works; this
is the bill.

Sources used: `docs/lua-apps.md` and `docs/CONVENTIONS.md`. I looked outside them three times,
all recorded inline: `demo_apps/pie/pie.lua` (one `grep` for `atan`, to confirm `math.atan2`
exists in Luau rather than Lua 5.3's `math.atan(y, x)`), `runtime/src/frame.rs` and
`runtime/src/lib.rs` (twice, for §1 and §2 — both times because the guide's wording admitted two
readings and the app is wrong under one of them).

---

## 1. `on_drag`'s `sx, sy` are frozen at the press, and the guide does not say so

This is the single fact the app's math hangs on, and the guide's phrasing points the wrong way:

> `sx, sy` keep reporting in that shape's coordinates even once the pointer leaves it — which is
> what holding something means.

Read plainly, "that shape's coordinates" is the shape's coordinates *now* — and for an arm whose
joint is being turned by the drag, "now" moves every frame. Under that reading the only correct
control law is a feedback correction (`angle += measured − grabbed`), which oscillates whenever
two move events arrive between repaints, since both are measured against the same stale pose.

The truth is the opposite, and much better: `Grabbed` in `runtime/src/lib.rs` captures
`into: Affine` at the press and applies *that* to every later point. The space is the shape's
space **as it was when you grabbed it** — a fixed frame. So the swing is an absolute mapping,
`angle = angle_at_press + (pointer angle now − pointer angle at press)`, exact and with nothing
to converge. That is `swing()` in `main.lua`.

I could not have got this from the guide. Suggested wording: *"`sx, sy` are the pointer in the
shape's coordinate space **as it was at the press** — the space is captured with the grab and does
not move if what you grabbed moves."* The current sentence is only unambiguous for a drag that
does not move the thing being dragged, which is the case this feature exists to serve.

Corollary worth documenting too: the grab is taken at pointer-down, before the 5pt slop
(`Capture::Pending` calls `Grabbed::take` with the press point), so grabbing a thin segment and
immediately sliding off it still reports that segment. The guide says this for `x, y` and not for
`shape`.

## 2. Does a named *group* have its own transform undone?

> `sx, sy` are the point in *that shape's* own coordinates, with its `group` and `instance`
> transforms undone.

For a `fill` this is clear. For a **named group**, which is itself a `group` with a transform, it
is exactly 50/50: "its group transforms" could mean the groups *above* it or those *including*
it. The app's hand is a named group with `transform = {1,0,0,1,hand_gap,0}`, and the answer
decides where the wrist's pivot sits in hand-local coordinates: `(-hand_gap, 0)` if its own
transform is undone, `(0, 0)` if it is not. Get it wrong and the hand swings about the wrong
point — a bug that looks like slightly-off physics, not like an error.

`frame.rs::find` settles it: the named-group arm returns `local: q`, the point *after*
`child(*transform, p, into)`, so a named group's own transform **is** undone and its coordinates
are the ones its children are drawn in. That is what `arm.grips["seg:hand"]` assumes. One clause
in the guide — "a named group reports in its own space, the one its children are drawn in" —
removes the coin flip.

## 3. Nothing can fire a handler headlessly

`cargo run -p app_host --example open` is the whole sanctioned loop, and it builds exactly one
view. It cannot deliver a click, a hover or a drag. For this app that means **the entire point of
the app is outside the loop**: `open` proved the tree had `frame#arm [on_drag on_hover]` and said
nothing about whether a grab on the forearm swings the elbow the right way, or at all.

What I did instead: bolted a temporary module-scope block onto `main.lua` that calls
`swing("start"/"move"/"end", shape, sx, sy)` with synthetic shape-local coordinates and `assert`s
the resulting joint angles, ran `open`, and deleted the block. It works only because a
module-scope `error` is the one thing that reaches the console (see §4), and it is not something
anyone should have to invent twice.

It also earned its keep: it caught a real bug. `atan2` jumps a full turn at its branch cut, so
swinging a segment past straight-back snapped it round. The unwrap in `main.lua` is there because
a synthetic move from `(-50, -1)` to `(-50, 1)` failed. No GUI session would have found that
reliably — you have to sweep through exactly that direction.

The ask, roughly in order of value:

- `open --hover "arm@120,80"`, `--click`, `--drag "arm@120,80..200,40"` — deliver the event
  through the real dispatch (so `shape`/`sx`/`sy` come from the real hit walk, which is the part
  an app cannot check) and re-print the tree afterwards.
- Failing that, anything that lets an app assert about itself and report without abusing `error`.

## 4. There is no `print`

The sandbox inventory — "`doc`, `ui`, `require`, `now()` (unix seconds), `uuid()`" — does not
mention `print`, and the console only carries errors (`LuaApp::log` is fed by view builds,
handler runs and reloads). So the only way to get a number out of a headless app is
`error(tostring(x))`. Two consequences worth a line in the guide: you get one observation per
run, and Luau truncates a long error string at ~512 characters, so batching observations into one
`error` silently cuts the tail off mid-word.

## 5. Missing required fields give raw mlua errors that name neither the field nor the call

The *unknown*-field path is excellent. The *missing*-field path is not. Both, verbatim:

| what I wrote | what I got |
|---|---|
| `gfx.fill({ path = p, color = "#fff" })` | `runtime error: fill: unknown field color` |
| `gfx.fill({ path = p, brush = b, "stray" })` | `runtime error: fill: positional fields are not allowed` |
| `gfx.group({ transform = { 1, 0, 0, 1, 0 }, … })` | `runtime error: transform needs six coefficients` |
| `gfx.group({ transform = { xx = 1, … } })` | `runtime error: transform: named or sparse fields are not allowed` |
| `gfx.path({ { "move", 0 } })` | `runtime error: path command 1 (move) needs 2 values, got 1` |
| `gfx.path({ { "move", x = 0, y = 0 } })` | `runtime error: path command 1: named or sparse fields are not allowed` |
| `ui.frame({ id = "unknown-prop", visual = v, rotate = 3 })` | `col#probe:[6] > [2] > frame#unknown-prop:[9] > unknown prop rotate` |
| **`gfx.frame({ height = 10 })`** | `error converting Lua nil to f64 (expected number or string coercible to number)` |
| **`gfx.stroke({ path = p, brush = b })`** | `error converting Lua nil to f64 (expected number or string coercible to number)` |
| **`gfx.fill({ brush = b })`** | `error converting Lua nil to userdata` |
| **`ui.frame({ id = "no-visual" })`** | `col#probe:[6] > [3] > frame#no-visual:[10] > error converting Lua nil to userdata` |

The last four are the ones you hit constantly while drafting, and they are the only four that
don't tell you what is wrong. Note that a missing `gfx.frame` `width` and a missing `gfx.stroke`
`width` produce **byte-identical** text — in a file that builds a dozen strokes and one frame,
that message locates nothing. `frame: missing field width` would match the quality of the rest.

The element-level breadcrumb (`col#probe:[6] > [2] > frame#unknown-prop:[9] > …`, with the
source line of each ancestor) is the best error in the system — worth saying in the guide that it
exists, so people know to read it.

## 6. `ui.*` constructors validate at render, not at call — so `pcall` catches nothing

`pcall(function() return ui.frame({ visual = v, on_drag = function() end }) end)` **succeeds**.
The `on_drag needs an id` error only appears when the tree is rendered. `gfx.*` items are
half-lazy in the same way: `gfx.fill({ path = p, color = "#fff" })` on its own is accepted and
only raises `fill: unknown field color` once it is placed in a `gfx.frame`.

Neither is wrong, but the guide's "**Anything not in this list is an error**, not a warning" reads
as if the constructor rejects it, and anyone writing a self-test will waste a round discovering
otherwise. One sentence: *"validation happens when the description is rendered, not when the
constructor returns."*

## 7. Guessed and unverifiable: a non-string shape `id`

`gfx.fill({ path = p, brush = b, id = 7 })` is **accepted** — it builds and renders. The guide
only ever shows string ids (`"slice:" .. i`), and never says whether a number is coerced to
`"7"` or arrives at the handler as `7`. I could not check, because checking requires firing a
handler (§3), so I kept every id a string. Either document the coercion or reject non-strings —
`id = i` inside a loop is the obvious thing to write and the comparison in the handler then
silently never matches.

Also accepted and probably shouldn't be: an **empty named group**,
`gfx.group({ id = "g", transform = {1,0,0,1,0,0} })`. It compiles and counts toward `hittable`,
but `touches()` needs real geometry inside, so it can never answer a hit. Given the guide
advertises "An id'd item with no brush is an invisible hit region", an id'd *group* with no
contents looks like it ought to be one too. It isn't; a hit region has to be a path.

## 8. The naming rule bites hardest exactly where the guide doesn't show it

> A named container answers as one shape and the names inside it stop being reachable — that is
> how you choose the granularity.

The rule is stated clearly and the consequence for a *nested* chain is never shown, which is where
it decides your whole structure. I wanted, and the obvious reading of the brief wants, this:

```lua
gfx.group({ id = "seg:upper", transform = shoulder, …fore… })   -- swallows everything below
```

and it cannot exist. Naming the upper-arm group makes `seg:fore`, `joint:elbow`, `joint:wrist`
and `seg:hand` — the entire rest of the arm — unreachable, because they are inside it. So in this
app the groups that rotate are **anonymous**, the ids live on the `fill`s they contain, and the
only named group is `seg:hand`, the leaf, which has nothing below it to swallow. It reads oddly
until you know why; `arm.lua` carries a comment saying so.

A four-line worked example in the guide — "a chain of rotations: name the leaves, not the links"
— is the single highest-value addition I can suggest, because the wrong choice here is not an
error. It compiles, it draws identically, and the readout just quietly names the wrong thing.

## 9. No way to ask a path for its bounds

The readout says "62% along the forearm", and to compute that it needs each shape's local extent.
Nothing exposes it, so `arm.spans` in `arm.lua` restates by hand what `shapes.lua` already drew:
the upper arm runs `0 … upper_len` along local x, a joint circle runs `−r … +r`, and so on. Two
copies of the same geometry, drifting apart the moment someone edits a path — which is precisely
the duplication `docs/design/frame-hits.md` §1 opens by arguing against ("even when it works it is
**two sources of truth**").

`gfx.path` is compiled and immutable, so its bounds are already known at build time. A
`gfx.bounds(path) -> {x0, y0, x1, y1}` would let the percentage derive itself. The guide's "not
exposed to Lua yet" list mentions reading back *layout*; it doesn't mention shape bounds at all,
so I spent a while looking for it before concluding it wasn't there.

## 10. Smaller things

- **`on_drag` has eleven parameters**, and a frame-based drag needs the last three. Every such app
  will write `function(phase, x, y, dx, dy, scale, origin_x, origin_y, shape, sx, sy)` and use
  four of them. A table-shaped event, or `shape, sx, sy` earlier in the list, would spare that.
- **An app can't ask whether a drag is in progress**, so `on_hover` has to be told to shut up by
  the app's own flag (`if S.drag then return end` in `main.lua`) — otherwise the readout follows
  whatever slides under the pointer while a segment is being held, which is the opposite of what
  the grab means. Fine as it stands, but every drag-on-a-frame app will hit it.
- **`on_hover` with `shape = nil` vs `phase == "leave"`** are different events that the app almost
  always wants to treat the same (the pointer is inside the frame but over bare canvas). The guide
  documents both correctly; it's just always three lines instead of one.
- **The guide never says which Lua.** "Your code runs sandboxed … Available beyond plain Lua" —
  but `plain Lua` is five incompatible languages where trigonometry is concerned. Luau keeps
  `math.atan2`; Lua 5.3 deleted it for `math.atan(y, x)`. This app is nothing but `atan2`, so the
  first line I wrote was the one I could not verify from the guide, and I settled it by grepping
  `demo_apps/` rather than by reading docs. `CONVENTIONS.md` says "Apps are Luau" — say it in
  `lua-apps.md` too, where an app author is actually looking.
- **`manifest.osv`'s grammar is never shown.** The guide says only that it "is optional and only
  supplies the display name". The file that was already here reads `app "Linkage" { }`, so I left
  it alone — but nothing in the docs would have let me write that from scratch, and the empty
  braces suggest there is more that could go in them.
