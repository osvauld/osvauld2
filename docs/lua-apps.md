# Writing Lua apps — a guide

How to write an app for the osvauld shell. For humans and agents alike; the reference app is
**kanban** (in the shell's source under `shell2/src/kanban/` — six files that use everything
in this guide), and `demo_apps/tally` is the smallest complete app.

## Shape

An app is a folder of `.lua` files with a root `main.lua` that **returns the view function**.
`require("theme")` loads `theme.lua` from your own folder (`ui/widgets.lua` ↔
`require("ui/widgets")`); required modules run once per app open and are cached — so anything
that must happen once (seeding a document) goes at module scope, **guarded**, because module
scope runs again on every open. `manifest.osv` is optional and only supplies the display name.

```lua
local C = require("theme")
local t = doc:open("tally")                 -- this app's document, by name

if not t.count then                         -- guarded seed
    t:set({ "count" }, doc.map({ n = 0 }))
end

return function()
    local n = t.count and t.count.n or 0
    return ui.col({
        id = "tally",
        grow = true,
        center = true,
        fill = C.bg,
        ui.text({ tostring(n), color = C.text, font_size = 72, no_wrap = true }),
        ui.button({
            id = "inc", h = 34, radius = 8, center = true, fill = C.accent,
            ui.text({ "+1", color = "#ffffff", font_size = 13 }),
            on_click = function() t:set({ "count", "n" }, n + 1) end,
        }),
    })
end
```

## The model — Elm, in Lua

The architecture is **Elm's**: state, view, update.

- **view** is your returned function. It takes nothing and returns a *description* of the
  screen built from state. It is pure: no side effects, no input handling — just a description.
- **update** is your handlers (`on_click` and friends). They run *after* the frame, as
  messages; they change state, and the next view reflects it. Kanban routes every handler
  through one `update(msg)` that dispatches on `msg.kind` to an `actions` table — that
  discipline is recommended, not required.
- **state** lives in three places, each with a job — see [State](#state).

Two consequences to internalize:

- **The description is rebuilt constantly** — every message, every pointer move. Keep `view`
  cheap: derive, don't compute the world. Nothing is mutated in place; you never "update the
  UI", you change state and describe it again.
- **`id`s are the continuity mechanism.** The shell keeps per-element state (scroll offsets,
  animation progress, input carets) keyed by `id`. An element without one is anonymous — which
  is fine for static things, and exactly why `scroll_*`, `on_drag`, `on_drop`, `fade` and every
  `ui.input` need one.

## Elements and children

Three kinds of table exist in an app, and only the first draws:

| kind | example | reaches the screen |
|---|---|---|
| **element** | `ui.col({ … })` | yes |
| **splice group** | `local body = {}` … `ui.col({ …, body })` | its contents, spliced in |
| **data** | `doc.map({ … })`, `stroke = { 1, C.line }` | no |

Only the `ui.*` constructors make elements — never hand-write `tag = "col"`. A bare table
nested as a child splices its contents into the parent (kanban builds `body` imperatively and
nests it whole).

**Children are positional entries and their order is meaning** — it is the child list.
A bare string child becomes a text element; `false` drops out (`ghost or false` is the idiom
for conditional children).

## Frame visuals (experimental foundation)

Anything you can describe with coordinates, you can draw. Paths and brushes are compiled once
into immutable resources, assembled into a **visual**, and published as a normal leaf with
`ui.frame({ visual = v, …normal props… })`. Frame dimensions are intrinsic layout claims, not an
implicit clip or scale.

| call | fields | notes |
|---|---|---|
| `gfx.path(commands)` | positional list of commands | up to 65536; drawing before `move` is an error |
| `gfx.solid(color)` | a CSS color string | |
| `gfx.linear_gradient({…})` | `from = {x, y}` · `to = {x, y}` · `stops = {{offset, color}, …}` · `extend` | 2–64 stops, offsets 0–1; `extend` is `pad` (default), `repeat`, `reflect` |
| `gfx.frame({…items})` | `width` · `height` · (`baseline`) + items as positional children | `width`/`height` required |
| `gfx.fill({…})` | `path` · `brush` · (`rule`) | `rule` is `nonzero` (default) or `evenodd` |
| `gfx.stroke({…})` | `path` · `brush` · `width` · (`cap` · `join` · `miter_limit` · `dashes` · `dash_offset`) | `cap`: `butt` (default) · `square` · `round`. `join`: `miter` (default) · `bevel` · `round`. `miter_limit` 4, `dashes` `{}` (max 64), `dash_offset` 0 |
| `gfx.group({…items})` | `transform = {xx, yx, xy, yy, dx, dy}` + items | |
| `gfx.instance({…})` | `visual` (another frame) · `transform` | placed by its local origin — account for a centered shape's radius |

`fill`, `stroke`, `group` and `instance` take an optional **`id`** — and that is what makes the
drawing touchable. Put the pointer handler on the `ui.frame` itself, and `on_click`/`on_hover`
report which named shape the pointer is on and where on it:

```lua
ui.frame({
    id = "pie",
    visual = v,
    on_hover = function(e) hot = e.phase ~= "leave" and e.shape or nil end,
})
```

Unnamed shapes are paint: the pointer falls through them to whatever is underneath. Where two
named shapes overlap the later one wins, because that is the one you see. A named container
answers as one shape and the names inside it stop being reachable — that is how you choose the
granularity, a whole dial or each of its ticks — and the names inside an instanced visual are
never reachable, since every instance would answer to the same ones. An id'd item with no brush
is an invisible hit region. A stroke is hit within half its width of the line; caps, joins and
dashes aren't modelled, so a dashed line is one line to the pointer.

Path commands are positional with exact arity: `{"move", x, y}`, `{"line", x, y}`,
`{"quad", cx, cy, x, y}`, `{"cubic", c1x, c1y, c2x, c2y, x, y}`, `{"close"}`. Everywhere else in
`gfx`, fields are named — a stray positional entry in a `fill` or a named key in a path is an
error, not an ignored extra.

There is no text inside a frame, no arcs, and no internal clips yet; labels are `ui.text`
siblings positioned by layout, which is what the demo charts' axes do.

## Layout

Flexbox: `ui.col` stacks children vertically, `ui.row` lays them out horizontally. Children
that should share the leftover space use `grow` (a bool = equal share, a number = weighted
share); children that should fill the other axis sit in a `stretch` parent.

**Sizing** — `w`/`h` fixed sizes · `min_w`/`max_w`/`min_h`/`max_h` bounds (a `grow` child
without a floor can be squeezed to nothing) · `w_full`/`h_full`/`full` · `no_shrink` (refuse
to be squeezed: labels, badges).

**Alignment** — `center` centers on both axes · `align_center` centers on the cross axis only
(cards under a title) · `stretch` makes children fill the cross axis.

**Spacing** — `gap` between children · `pad` on all sides, `px`/`py` per axis · `mt`/`mb`
margins.

**Scrolling** — `scroll_x`/`scroll_y` on a container makes it scrollable on that axis;
children keep their natural size there. A scrolling container with `grow` uses the remaining
main-axis space instead of letting its content enlarge its viewport. **Needs an `id`.**

**Overlay positioning** — `absolute` takes the element out of the flow; `top`/`left`/
`right`/`bottom` position it in viewport coordinates. This is how kanban draws its drag ghost
and its modal scrim. A portal popover uses `ui.overlay`: exactly two positional children, the
anchor then the root-painted panel. `side` is `bottom` (default), `top`, `left`, or `right`;
`align` is `start` (default), `center`, or `end`; `on_dismiss` adds a click-away catcher and,
like every handler, needs an `id`:

```lua
ui.overlay({
    id = "add-menu",
    side = "top",
    align = "start",
    on_dismiss = function() open = false end,
    ui.button({ "Add" }),
    ui.col({ ui.text({ "Popover" }) }),
})
```

The panel escapes ancestor clips and stays screen-sized. Its element anchor follows stable
placement through zoom, pan, authored scale, and offset, but ignores transient press-scale so an
opening popover does not wobble while its button settles. A point anchor supplied by Rust is
already screen-space.

**Text** — `ui.text({ "label", … })` needs its label as child 1. It wraps like a paragraph by
default; `no_wrap` makes a label. Measure happens for you.

**When the window resizes** — nothing to handle: relayout is automatic. An app is responsive
exactly to the extent its tree uses `full`, `grow`, `stretch` and scroll instead of fixed
sizes — kanban fills the viewport, gives its board row a definite `h_full`, and lets only the
cards region scroll, so window drags and added cards do not enlarge the board.

**Buttons have no defaults** — `ui.button` is a `ui.row` and nothing more; the fill, hover
states, padding and centering are all yours to declare.

**Inputs** — `ui.input` and `ui.text_area` require `value`, `id` and `on_input = function(v)`
(three mandatory props, no children — put the button *beside* it). `on_enter`, `on_esc` and
`autofocus` are extras. The pattern is a draft in `ui.state`, shown in
[Patterns](#patterns-from-kanban).

**Colors** are any CSS color string: `"#0d1117"`, `"#rrggbbaa"`, `"rgba(13,17,23,0.72)"`,
`"hsl(212,92%,58%)"`, named colors.

## Reference

### Constructors

Eight, and no others. Anything else is `unknown tag`.

| constructor | children | required | notes |
|---|---|---|---|
| `ui.col({…})` | any | — | vertical flex |
| `ui.row({…})` | any | — | horizontal flex |
| `ui.button({…})` | any | — | a `ui.row` and nothing more — no fill, padding or centering of its own |
| `ui.text({ "label", … })` | **the label is child 1** | — | wraps like a paragraph; `no_wrap` makes it a label |
| `ui.input({…})` | none | `value` · `id` · `on_input` | extras: `on_enter` · `on_esc` · `autofocus` |
| `ui.text_area({…})` | none | `value` · `id` · `on_input` | same, multi-line |
| `ui.frame({ visual = v, … })` | none | `visual` (a `gfx.frame`) | the frame's `width`/`height` are its layout claim |
| `ui.overlay({ anchor, panel, … })` | **exactly two** | — | takes *only* `id` · `side` · `align` · `on_dismiss` — no box or paint props; style the panel child instead |

`ui.state(id, init)` is not an element — it is per-viewer scratch, see [State](#state).

### Props

Every prop below works on every element (`ui.overlay` excepted, above). **Anything not in this
list is an error**, not a warning — there is no silent ignore, so a typo shows up as a red box
in place rather than as a missing effect. Types are checked too (`expected a number got string`).

Shorthands are applied before the longhands that override them, whatever order the Lua table
happens to be in: `full` before `w`/`h`, `size` before both, `pad` before `px`/`py`, `fade_in`
before `fade`.

| group | prop | value |
|---|---|---|
| box | `full` · `w_full` · `h_full` | bool — fill the parent on both axes / one |
| | `size` | `{w, h}` |
| | `w` · `h` | number |
| | `min_w` · `max_w` · `min_h` · `max_h` | number (a `grow` child with no floor can be squeezed to nothing) |
| | `grow` | bool, or a number for a weighted share — `grow = 2` beside `grow = true` is 2:1 |
| | `no_shrink` | bool — refuse to be squeezed (labels, badges) |
| | `wrap` | bool — flex children onto more lines |
| spacing | `pad` · `px` · `py` · `gap` · `mt` · `mb` | number |
| alignment | `center` · `align_center` · `stretch` | bool — both axes / cross axis only / children fill the cross axis |
| positioning | `absolute` | bool — out of the flow |
| | `top` · `left` · `right` · `bottom` | number, **viewport** coordinates |
| | `offset` | `{dx, dy}` — shifts paint, not layout |
| | `scale` | number — scales this subtree, layout unchanged |
| paint | `fill` · `color` | a CSS color string (`color` is the text one) |
| | `radius` · `opacity` · `font_size` | number |
| | `stroke` | `{width, color}` |
| | `stroke_dash` | `{width, color, dash, gap}` |
| | `no_wrap` | bool |
| hover / press | `hover_fill` · `press_fill` | color — the transition to it is automatic |
| | `hover_stroke` · `press_stroke` | `{width, color}` |
| | `tint` | **milliseconds** — the fade time for a hover brightening, not a color |
| | `press_scale` | number, e.g. `0.96` — **needs `id` and `on_click`** |
| animation | `fade_in` | ms |
| | `fade` | `{target_opacity, ms}` |
| | `slide_in` | `{{dx, dy}, ms}` — a **nested** pair, then the duration |
| viewport | `zoomable` · `zoom_x` | bool — Ctrl+wheel zooms children around the pointer, both axes or x only. **Needs `id`.** |
| scroll | `scroll_x` · `scroll_y` | bool. **Needs `id`.** |
| input | `value` · `autofocus` | string · bool |

Four props need an `id` because the shell keys state by it: `scroll_*` (offset), `zoomable` /
`zoom_x` (camera), `press_scale` (spring), and every handler (dispatch). Asking for one without
an `id` is an error naming the prop.

Not exposed to Lua yet, so don't go looking: right-click, text measurement or alignment inside a
`gfx` frame, hit-testing individual Frame shapes, and reading back layout.

## Handlers

**Every handler needs an `id`**, and no two elements may share an id and a handler. A handler is
found by id + name when its event is delivered, not when the view was built, so a click still
reaches its element when a peer edit lands between press and release — and is dropped if the
element is gone.

A handler that carries more than one value is called with **one table**, not positional
arguments — `function(e)`, and every value is a named field on `e`. The two that carry nothing or
a single value keep their plain form: `on_enter`, `on_esc`, `on_faded_out` take nothing, and
`on_input = function(v)` takes the new text.

This is why: positionally, a short or mis-ordered signature binds the wrong values *and keeps
running*. Writing `function(phase, x, y, dx, dy, shape, sx, sy)` for `on_drag` puts `scale` into
`shape`, so `shape` is the number 1, looks like a shape id, and fails every lookup in silence. A
wrong key is `nil`, which is loud the moment you index it, and a field added later can never
shift the meaning of one already there.

```lua
on_drag = function(e)
	if e.phase == "start" then grab(e.shape, e.sx, e.sy) end
end
```

- `on_click(e)` — `e.x, e.y` are where the click landed, from the element's top-left corner in
  its own units, zoom and scroll undone: a click on a 1400×900 canvas reports canvas numbers
  whatever the camera is doing. Fired from the bridge, with no layout, they are `0, 0`. Inside a
  `zoomable`, a press that travels past 5pt pans instead.
- `on_hover(e)` — `e.phase` is `"enter"` / `"move"` / `"leave"`, `e.x, e.y` as `on_click` (outside
  the element on `"leave"`). An element is hovered while the pointer is inside it, like
  `hover_fill`: a parent stays hovered over its children, and an element painted on top doesn't
  hide the one below — check your own geometry if that matters. It is sampled every frame as well
  as on every pointer move, so geometry that drifts under a still pointer reports it: an element
  that slides under one enters where it arrives, and `"move"` fires when the shape beneath the
  pointer changes *or* slides, without the pointer having moved at all. What it will not do is
  repeat itself — a still pointer over still geometry says nothing, so `"move"` always means
  something actually changed.
- `e.shape, e.sx, e.sy` on both of those name the shape inside a `ui.frame`'s visual that the
  pointer is on — see [Frame visuals](#frame-visuals-experimental-foundation). `e.shape` is the
  `id` you gave the shape, and `e.sx, e.sy` are the point in *that shape's* own coordinates, with
  its `group` and `instance` transforms undone. All three are absent on an element that draws no
  frame, or when the pointer is on none of its named shapes.
- `on_enter`, `on_esc`, `on_faded_out` — plain callbacks, no argument.
- `on_input = function(v)` — an input's new text.
- `on_drag(e)` — `e.phase` is `"start"` / `"move"` / `"end"`. `e.x, e.y` are the pointer in the
  element's own units, as `on_click` reports them (at `"start"`, where the press landed, not where
  the 5pt slop ended). `e.dx, e.dy` are movement since the press, `e.scale` lets a root ghost
  match zoomed content, and `e.origin_x, e.origin_y` are the dragged element's screen-space
  origin — only a root-level ghost placing itself in screen space needs those. `e.shape, e.sx,
  e.sy` are the shape the press **grabbed**: the same one for the whole gesture, whatever the
  pointer has since slid over, and reported even once the pointer leaves it — which is what
  holding something means. **Needs an `id`.**

  A press that travels past 5pt is a drag and fires **no** click. A press that travels less is a
  click, reported where it was *released*. No hand is perfectly still, so the second is ordinary.

  The coordinate space is **frozen at the press**, not recomputed each move: `e.sx, e.sy` are
  measured in the space the shape had when you grabbed it, even if the shape has rotated or moved
  since. That is what makes them useful — they are a fixed grab offset for the whole gesture, so
  "where should this go now" is `e.x - e.sx`, computed the same way on every move. Read them as
  live coordinates instead and you write a feedback correction that fights itself, which looks
  like a broken drag rather than a coordinate-space mistake.
- `on_drop(e)` — `e.phase` is `"over"` (while hovering) / `"release"`; `e.x, e.y` are normalized
  to the drop target (0–1), so `e.y < 0.5` means "above the midline". **Needs an `id`.**
- `on_frame(e)` — an **experimental visual/prototyping loop**. `e.elapsed` is monotonic Runner
  time and `e.dt` is clamped to 0.1 seconds after stalls; only Runner's first frame is guaranteed
  zero. Presence keeps repainting, so omit it to stop that request. Custom screenshots currently
  dispatch it too; it is not a fixed-step world scheduler. **Needs an `id`.**

Handlers run after the frame. A gesture should accumulate in your own state during
`"start"`/`"move"` and commit to the document once at the end — a drag is **one write**, not
sixty.

## State

Three homes, pick by lifetime and audience:

| | locals | `ui.state(id, init)` | `doc:open(name)` |
|---|---|---|---|
| audience | this viewer | this viewer | everyone, forever |
| survives reopen | no | yes | yes |
| survives hot reload | no | **yes** | yes |
| cleaned up | never | **when its `id` leaves a frame** | never |

- **Plain locals** (a module-scope table like kanban's `S = { drag = nil, … }`) are right for
  gesture and interaction state: drags in flight, modal-open flags, resize widths mid-drag.
  They die with the app instance — usually what you want for pointer state.
- **`ui.state(id, init)`** is right for state *keyed to an element that may come and go* —
  `"draft:" .. col_id` is garbage-collected when its column disappears — and for anything that
  should survive a hot reload, like half-written drafts.
- **Documents** are right for anything that is a claim about the work: a column's name, its
  width, a card's text. The next person to open this app sees the board somebody arranged.

Kanban uses all three at once — that split is the example to copy.

## Documents

`local board = doc:open("board")` opens (or creates) a named document and returns the
**mirror**: reads are plain table indexes — `board.columns[1].name`, `#board.cards` — free
and instant. Writes are explicit:

```lua
board:set({ "cards", id, "col" }, "c-doing")           -- path segments: names or ids
board:insert({ "cards" }, doc.map({ id = uuid(), col = "c-todo", text = "…" }))
board:delete({ "cards", id })
board:move({ "cards", id }, to)                        -- `to` is the index after removal
```

Write values are built with `doc.map{…}` / `doc.list{…}` / `doc.text("…")`; lists take
positional entries only — a stray named key is an error, not a silent extra field.

The rules that bite, once each:

- **The mirror is a frame behind your own write.** A write lands immediately; reads see it on
  the next view. Read what you need *first*, then write — never read back what you just wrote.
- **Address by stable id, never by position.** Stamp `id = uuid()` at birth. The mirror is
  positional; a concurrent insert shifts it.
- **`:move` removes then reinserts** — dragging downward lands one slot short unless you
  nudge the target (see `target_index` in kanban's `model.lua`).
- **Deleting while iterating** collects the doomed ids first, deletes second — the list you
  are walking does not shrink until the next frame.
- **An unchanged write is a no-op.** Don't guard against writing a value that might already
  be there; the document skips it.

## Animation

Presentation animation is declarative:

- **Hover feedback animates itself.** `hover_fill`, `hover_stroke` and `tint` are transitions
  bound to hover state — declare the color, the fade is automatic. `tint` is the odd one: its
  number is the fade duration in milliseconds, not a color or an amount.
- **Press feedback is declarative for clickable elements.** `press_fill` / `press_stroke` apply
  while the pointer is down; `press_scale = 0.96` scales an `on_click` element subtree around
  its centre. `press_scale` needs an `id` and does not change layout.
- **Value animations go to a declared target.** `fade = {target, ms}` animates opacity,
  `fade_in = ms` fades in on first appearance, `slide_in = {{dx, dy}, ms}` slides in from an
  offset. `opacity` is the static version with no tween.

Progress is kept per element **`id`** — which is why an animated element needs a stable one.
Retargeting mid-flight reverses smoothly instead of jumping: kanban's drop guides are
`fade = { on and 1 or 0, 140 }` on an `id` that never changes, so the line fades in and out
as the pointer moves. `on_faded_out` fires when a fade completes — the hook for removing an
element after it fades away.

Authored simulations are the deliberate exception: `on_frame(dt, elapsed)` runs Lua after each
presented frame and schedules the next while declared. Keep transient positions and velocities in
module locals, perform no document writes per tick, avoid allocations in inner numeric loops, and
remove the handler when settled. Routine UI effects must use the declarative transitions above.

## Patterns from kanban

The kanban app is the canonical example — no other app exercises all of this. Pattern →
where to look:

| pattern | file |
|---|---|
| typed drags — `on_drag` updates `S`, `on_drop` computes placement, one commit at `"release"`; one drop target, two meanings (card *into* / column *beside*) | `main.lua` + `model.lua` |
| the drag ghost — rebuilt at the root, `absolute` + `top`/`left` in viewport coords, no `id` and no handlers (a duplicate id would collide), the original dims via `opacity` | `main.lua` |
| drop guides always rendered — the space is reserved, only the alpha animates, nothing shifts when a target appears | `ui/widgets.lua` |
| resize — a grip in the gutter, live width from viewer state, one write on release — **the full pattern below** | `model.lua` + `main.lua` |
| modal — a root-level `absolute` scrim with `rgba` fill and `on_click` to dismiss; the panel swallows clicks with `on_click = function() end` and appears with `fade_in` | `main.lua` |
| composer — `ui.input` + button, draft in `ui.state("draft:" .. col_id)`, cleared on send | `main.lua` |
| spacer — `ui.col({ grow = true })` pushes what follows to the far end | throughout |

### Resize, in full

Column resizing is achieved and shipping in kanban, and it's the most instructive pattern in
the corpus: all three state homes in a few lines.

```lua
-- model.lua — the drag lives in viewer state, the width lives in the document
function actions.resize(msg)
	if msg.phase == "start" then
		local c = find(board.columns, msg.id)
		local w = c and c.w or C.col_w
		S.resize = { id = msg.id, from = w, w = w }
		return
	end
	if not (S.resize and S.resize.id == msg.id) then return end   -- a stale drag
	if msg.phase == "move" then
		local w = S.resize.from + msg.dx
		S.resize.w = math.max(C.col_w_min, math.min(C.col_w_max, w))
	else -- "end"
		board:set({ "columns", msg.id, "w" }, S.resize.w)
		S.resize = nil
	end
end

-- main.lua — this frame's width: the drag's live value, else the stored one
local function width_of(c)
	if S.resize and S.resize.id == c.id then return S.resize.w end
	return c.w or C.col_w
end
```

What to steal from it:

- **Clamp on every move**, never only at the end — an end-only clamp lets the pointer walk
  past the limit and the column sits dead until it comes back.
- **During the drag, layout reads viewer state; the document is untouched.** A drag is sixty
  frames of `S.resize.w` and **one `:set` at `"end"`** — one document write per finished
  gesture, which is also what a future peer receives: the commit, not the drag.
- **The write needs no guard** — an unchanged value is a no-op in the document.
- **`min_w`/`max_w` on the element are the layout guard — a second, separate job from the
  drag clamp.** A width can arrive that never passed the clamp: from a peer whose theme
  differs, from a hand-edited document. The drag clamp is policy; the bounds are physics.
- **The grip is a sibling in the gutter, not a child of the column** — it must sit *between*
  two columns, and a child is clipped by its parent's rounded panel.

## App structure

The kanban split, worth copying at any size:

- `theme.lua` — palette and geometry constants; depends on nothing.
- `model.lua` — the document, guarded seeds, and the `actions` table (the update half of Elm);
  shared interaction state is a field on the exported table — a `local` rebind is invisible
  across `require`, a table field is not.
- `main.lua` — the view; owns everything that reads or writes pointer state, so the read and
  the write stay in one file.
- `ui/widgets.lua` — pure functions of their arguments; handlers are passed in, never reached
  for.

## The sandbox, and what happens when you err

Your code runs sandboxed: no `io`, no filesystem, no network, no `os` — and `require` can
only see your own folder. Available beyond plain Lua: `doc`, `ui`, `require`, `now()` (unix
seconds), `uuid()`. A runaway loop is killed, with the line number.

Errors are for reading, not for fearing:

- An unknown prop or a value of the wrong type is an error at that element — a red box in
  place, **siblings stay alive**.
- A `gfx` call that is missing a required field names it: `frame needs width`, `fill needs
  brush`, `stroke needs width`. Passing the wrong handle — a brush where a path goes — says so
  too: `fill.path must be a gfx.path`.
- A view that throws keeps the **last good frame** with an error banner naming file and line.
- Nothing you write in a handler can take the app down for good; fix the file and it reloads.

The fastest authoring loop is: upload, look at the banner, fix, upload again. Keep files small
enough that a reported line number means one obvious thing.

## Checking it without a window

`open` loads a folder, builds a view, and prints the element tree with each element's id and
handlers, then the console. It exits non-zero if anything reached the console, so it drops
straight into a loop.

```
cargo run -p app_host --example open -- demo_apps/pie
cargo run -p app_host --example open -- demo_apps/pie --hover 180,128 --tree
cargo run -p app_host --example open -- demo_apps/voronoi --drag 300,300:380,360
cargo run -p app_host --example open -- demo_apps/voronoi --hover 400,250 --frames 200 --tree
```

`--hover X,Y`, `--click X,Y`, `--drag X0,Y0:X1,Y1[:steps]` and `--frames N` run in the order given,
and after each one it reports new console lines and whether the view changed. These are not
simulated: the
same layout, the same hit regions, the same handler call the window makes. The only thing supplied
by hand is the pointer coordinate — which is also the limit, since scaling, event timing and
painting all live below that line. A view that passes here can still look wrong.

Coordinates are logical points from the top-left of a 1200×800 viewport (`--size WxH` to change
it), so they are the same units an element's rect is in. `print()` from a handler goes to stdout.

`--frames N` is time passing with the pointer left where it was — the only way to watch an
animating app move, and the only way to see `on_hover` follow geometry that drifts under a still
pointer. Offscreen frames run back to back, so `N` is a count of frames, not of seconds.

Two behaviours are easier to see here than to reason about: a press that travels more than 5pt is
a drag and fires **no** click, and a press that travels less is a click reported at the point it
was *released*. Since no hand is perfectly still, the second is the ordinary case.
