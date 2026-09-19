# Voronoi — friction log

Built from `docs/lua-apps.md` alone, apart from the two detours recorded below. Checked with
`cargo run -q -p app_host --example open -- demo_apps/voronoi` (clean, 12 elements, 5 handlers)
and with a plain-Lua harness that runs `voronoi.lua` and `model.lua` under `lua5.4`: cell areas
sum to the rectangle to 1.7e-16 relative, 13741 sample points each land in exactly one cell and
always in their nearest site's, and 400 simulated drag moves hold the grab offset exactly.

The two detours, and what sent me there:

- **`runtime/src/lib.rs`** — for §1 and §6 below. Both are questions the guide raises and does not
  answer, and both change the code you write.
- **`docs/design/frame-hits.md`** — looking for the answer to §1 before resorting to source. It has
  the answer to §5 (the transform is accumulated on the way down) but not §1.

No other demo app was read.

---

## 1. `on_drag`'s `sx, sy`: measured against *which* transform?

The guide says:

> `sx, sy` keep reporting in that shape's coordinates even once the pointer leaves it — which is
> what holding something means.

For a shape that is moving *because you are dragging it*, "that shape's coordinates" has two
readings, and they are not close:

- **live transform** → `sx, sy` stays the grab offset forever, and `x - sx` is where to put the
  shape. Write the obvious thing and it works.
- **transform frozen at the press** → `sx` grows by exactly the distance dragged, so `x - sx` is
  the shape's position *at the press* and never changes. Write the obvious thing and the shape
  sticks to where you grabbed it, silently, with no error.

It is the second one (`Grabbed { into: Affine }` in `runtime/src/lib.rs`, captured by
`Grabbed::take` at press). I only know that from the source. This is the first bug every author
writing a drag against this API will ship, because the failure looks like "drag doesn't work"
rather than like a coordinate-space mistake.

`model.lua` works around it by saving `sx, sy` at `"start"` and never reading them again. One
sentence in the guide — *"the transform is the one the shape had when it was grabbed"* — removes
the whole problem. The design doc's revision note already says exactly this ("the shape's
transform outlives the hit"); it just never reached the authoring guide.

## 2. An item's error is blamed on the `gfx.frame` line, and does not name the field

A `gfx.stroke` on line 125 with no `width`:

```
View error: error converting Lua nil to f64 (expected number or string coercible to number)
stack traceback:
	[C]: in ?
	[string "main.lua"]:166: in ?
```

Line 166 is `visual = gfx.frame(items)`. The stroke is 41 lines away, the message names neither
`width` nor `stroke`, and the same text comes out of a `gfx.frame` missing its `width` — two
different mistakes, one indistinguishable message. Omitting `height` on the frame gives it too.

`gfx.path` does not have this problem; it validates eagerly and reports precisely:

```
View error: runtime error: path command 2 (line) needs 2 values, got 1
	[string "main.lua"]:13: in function 'poly_path'
	[string "main.lua"]:113: in ?
```

So do the structural checks on items — `fill: positional fields are not allowed`,
`transform needs six coefficients` — except that those, too, are reported at the `gfx.frame` line
rather than at the call that was wrong. Anything that names the offending call would be an
improvement; a *missing required field* should say which field, on which call, the way the prop
checker does.

The prop and handler errors are the good ones and worth copying:

```
col#voronoi:[178] > [2] > frame#canvas:[208] > unknown prop cursor
col#voronoi:[177] > [2] > frame:[207] > on_drag needs an id
```

## 3. `on_hover` fires only when the pointer moves, and nothing can re-ask

Documented — "It fires only when the pointer moves" — and the single biggest constraint on an
app whose geometry moves under a still pointer, which is precisely what this app is for. The
shell's answer to "what am I on" goes stale the moment the pointer stops, and there is no way to
ask again.

So `model.lua` carries `M.under(x, y)`: a second hit test, in Lua, that has to mirror the draw
order (dots over cells) by hand. That is the exact thing `docs/design/frame-hits.md` §1 says frame
hits exist to abolish:

> And even when it works it is **two sources of truth** — the draw math and the pick math drift
> apart with nothing to catch it.

Either would fix it: re-dispatch hover after each `on_frame` when the pointer is inside the
element, or expose the hit test as a query (`ui.hit(id, x, y)` → `shape, sx, sy`). The second is
more generally useful — a view cannot read the pointer position either, so the app also has to
cache that from the last hover event.

## 4. A pointer app cannot be tested without a GUI

`--example open` builds one frame and prints the tree, which proves the description compiles and
nothing more. Every behaviour this app exists to demonstrate — the grabbed shape held across a
gesture, the cell under a still pointer changing as the diagram drifts, the selection surviving
a drag — is unreachable from the headless checker. I verified the *arithmetic* by running
`voronoi.lua` and `model.lua` under stock `lua5.4` (they are pure), and the *runtime semantics*
by reading Rust. Neither is a test.

`open --hover x,y`, `--click x,y` and `--drag x0,y0 x1,y1`, dispatched through the same path the
shell uses and printing what each handler saw, would make interaction apps testable for the first
time. It is also the only way an agent can check its own work here.

## 5. A shape that isn't inside a `group` has no coordinates of its own

`sx, sy` are "the point in *that shape's* own coordinates, with its `group` and `instance`
transforms undone". Nothing says what they are when there are no such transforms: they are the
frame's, so a top-level `gfx.fill` reports `sx, sy == x, y` and a readout prints the same two
numbers twice. The guide's phrasing reads as a caveat about transforms, not as *the mechanism for
giving a shape a local frame*.

That mechanism is worth a line in the guide, because it is how you make `sx, sy` mean something:
wrap the item in a named `gfx.group` whose transform is the translation, and build the path
relative to that origin. Both this app's cells and its site dots do it, and it is why hovering a
cell can report "37 right, 12 below your site" instead of repeating the frame coordinates.

## 6. `on_click` and `on_drag` on the same element — do both fire?

The guide lists the handlers independently and never says how a press that becomes a drag is
resolved. I assumed both would fire on release and started writing a swallow-the-next-click flag,
which would have been a stale-state bug of its own. The source says otherwise: `self.pressed =
None` when the press passes the 5pt slop, so **a real drag cancels the click**, and no guard is
needed.

The other half of the same rule is worth stating: **a press that never passes 5pt fires no
`on_drag` at all** — not even `"start"` and `"end"` — so `on_drag` cannot be used to observe a
press. Two sentences in the handler section would cover both.

## 7. No arcs, so every app re-derives the same circle

`main.lua` carries `circle_path` — four cubics and `KAPPA = 0.5522847498307933`. Any app that
draws a dot, a node, a pie or a knob writes the same ten lines. `gfx.circle(cx, cy, r)` (or an
`{"arc", …}` path command) would pay for itself immediately; failing that, the guide should just
print the snippet where it says "no arcs".

## 8. A computed region cannot be labelled

I wanted the cell index drawn at each cell's centroid. There is no text inside a frame (documented)
and the suggested fallback — "labels are `ui.text` siblings positioned by layout" — only reaches
points that layout can express. For an arbitrary interior point the only tool is `absolute` with
`top`/`left` in **viewport** coordinates, and the frame's own viewport origin is not knowable to
Lua ("reading back layout" is explicitly not exposed). So a shape whose position is an algorithm's
output can be hovered and named but cannot be labelled in place. That gap is worth recording
against the "describable = touchable" direction: it is touchable, and it is mute.

## 9. A named group's hit region includes its children's stroke width

Each cell is a fill plus a 1.5pt edge stroke inside one named group, so every cell claims 0.75pt
beyond its own polygon and adjacent cells overlap in a 1.5pt band where the later index wins. The
guide gives the rule for a stroke *item* — "a stroke is hit within half its width of the line" —
but not that it composes into the region of a named ancestor. Harmless at these sizes; for a
hairline grid with a thick separator stroke, every cell would quietly steal a band from its
neighbour and nothing would explain why.

## 10. Smaller things

- **`manifest.osv` has no documented syntax.** The guide says it "is optional and only supplies
  the display name" and never shows one. `app "Voronoi" { }` is a guess that happens to work;
  there is nothing to check it against, and `--example open` does not read the file at all.
- **Shape ids go out as strings and come back as strings.** Any app with per-item shapes writes
  `tonumber(shape:match(":(%d+)$"))` to get its index back (`M.index_of`). Every future frame app
  will write that same function.
- **A conditional handler prop has no documented idiom.** `false` drops a *child*
  (`ghost or false`), but there is no equivalent shown for a prop; this app builds the props table
  and assigns `canvas.on_frame = M.step` afterwards to stop the repaint request while paused. That
  works, and `on_frame = (not paused) and M.step or nil` in the literal works too, but neither is
  in the guide next to the sentence that tells you to omit the handler.
- **The prescribed app split assumes a document.** "`model.lua` — the document, guarded seeds, and
  the `actions` table". This app has no document at all: drifting sites and a drag in flight are
  exactly the "gesture and interaction state" the State section says should be locals. The
  structure section could say that a `model.lua` with no `doc:open` is normal.
