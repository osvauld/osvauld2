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
            h = 34, radius = 8, center = true, fill = C.accent,
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
children keep their natural size there. **Needs an `id`.**

**Overlay positioning** — `absolute` takes the element out of the flow; `top`/`left`/
`right`/`bottom` position it in viewport coordinates. This is how kanban draws its drag ghost
and its modal scrim.

**Text** — `ui.text({ "label", … })` needs its label as child 1. It wraps like a paragraph by
default; `no_wrap` makes a label. Measure happens for you.

**When the window resizes** — nothing to handle: relayout is automatic. An app is responsive
exactly to the extent its tree uses `full`, `grow`, `stretch` and scroll instead of fixed
sizes — kanban fills the viewport (`full = true`) and lets the board row grow, so window
drags just work.

**Buttons have no defaults** — `ui.button` is a `ui.row` and nothing more; the fill, hover
states, padding and centering are all yours to declare.

**Inputs** — `ui.input` and `ui.text_area` require `value`, `id` and `on_input = function(v)`
(three mandatory props, no children — put the button *beside* it). `on_enter`, `on_esc` and
`autofocus` are extras. The pattern is a draft in `ui.state`, shown in
[Patterns](#patterns-from-kanban).

**Colors** are any CSS color string: `"#0d1117"`, `"#rrggbbaa"`, `"rgba(13,17,23,0.72)"`,
`"hsl(212,92%,58%)"`, named colors.

## Props reference

Unknown props are **errors**, not warnings. When a shorthand and a longhand are both set,
the shorthand applies first (`pad` before `px`/`py`, `full` before `w`/`h`).

| group | props |
|---|---|
| box | `full` · `w_full` · `h_full` · `size = {w, h}` · `w` · `h` · `min_w` `max_w` `min_h` `max_h` · `grow` (bool or share) · `no_shrink` · `wrap` |
| spacing | `pad` · `px` · `py` · `gap` · `mt` · `mb` |
| alignment | `center` · `align_center` · `stretch` |
| positioning | `absolute` · `top` `left` `right` `bottom` · `offset = {x, y}` |
| paint | `fill` · `color` (text) · `radius` · `stroke = {width, color}` · `stroke_dash = {width, color, dash, gap}` · `opacity` · `font_size` · `no_wrap` |
| hover | `hover_fill` · `hover_stroke = {width, color}` · `tint` |
| animation | `fade_in = ms` · `fade = {target, ms}` · `slide_in = {dx, dy, ms}` |
| scroll | `scroll_x` · `scroll_y` (need `id`) |
| input | `autofocus` · `value` |

## Handlers

- `on_click`, `on_enter`, `on_esc`, `on_faded_out` — plain callbacks.
- `on_input = function(v)` — an input's new text.
- `on_drag = function(phase, x, y)` — phases `"start"` / `"move"` / `"end"`; `x, y` are
  relative to where the grab started. **Needs an `id`.**
- `on_drop = function(phase, x, y)` — phases `"over"` (while hovering) / `"release"`; `x, y`
  are normalized to the drop target (0–1), so `msg.y < 0.5` means "above the midline".
  **Needs an `id`.**

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

Two kinds, both declarative:

- **Hover feedback animates itself.** `hover_fill`, `hover_stroke` and `tint` are transitions
  bound to hover state — declare the color, the fade is automatic.
- **Value animations go to a declared target.** `fade = {target, ms}` animates opacity,
  `fade_in = ms` fades in on first appearance, `slide_in = {dx, dy, ms}` slides in from an
  offset. `opacity` is the static version with no tween.

Progress is kept per element **`id`** — which is why an animated element needs a stable one.
Retargeting mid-flight reverses smoothly instead of jumping: kanban's drop guides are
`fade = { on and 1 or 0, 140 }` on an `id` that never changes, so the line fades in and out
as the pointer moves. `on_faded_out` fires when a fade completes — the hook for removing an
element after it fades away.

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
		S.resize = { id = msg.id, from = w, x0 = msg.x, w = w }
		return
	end
	if not (S.resize and S.resize.id == msg.id) then return end   -- a stale drag
	if msg.phase == "move" then
		local w = S.resize.from + msg.x - S.resize.x0
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
- A view that throws keeps the **last good frame** with an error banner naming file and line.
- Nothing you write in a handler can take the app down for good; fix the file and it reloads.

The fastest authoring loop is: upload, look at the banner, fix, upload again. Keep files small
enough that a reported line number means one obvious thing.
