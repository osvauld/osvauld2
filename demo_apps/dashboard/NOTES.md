# Dashboard demo — build notes (docs-only experiment)

Started. Source of truth: docs/lua-apps.md. Appending friction as it happens.

## Friction as it happened

1. **The guide's canonical example is source I can't read.** `docs/lua-apps.md` opens with
   "the reference app is **kanban** (in the shell's source under `shell2/src/kanban/`...)" and
   the whole "Patterns from kanban" section is a table of *pointers into that source* rather
   than code. Also "demo_apps/tally is the smallest complete app". For a docs-only build both
   are unreadable, so every pattern listed there (typed drags, drag ghost, drop guides, modal,
   composer) exists only as a one-line description. Only `resize` is actually quoted in full.
   Tag: docs-gap.

2. **No enumerated `ui.*` / `gfx.*` API surface.** There is no list of constructors or of which
   props each one accepts. I harvested from prose: `ui.col`, `ui.row`, `ui.text`, `ui.button`,
   `ui.input`, `ui.text_area`, `ui.overlay`, `ui.frame`, `ui.state`, plus `gfx.path/solid/
   linear_gradient/frame/fill/stroke/group/instance`. Combined with "Unknown props are
   **errors**", guessing is punished, so I restricted myself to the props table verbatim.
   Tag: docs-gap.

3. **No text inside `gfx`, and `absolute` is viewport-relative.** For a chart I need axis
   labels near a vector plot. `gfx.frame` has no text item, and the only escape from flow
   layout is `absolute` whose "top/left/right/bottom position it in viewport coordinates"
   (Layout > Overlay positioning) -- i.e. there is no parent-relative absolute. So labels have
   to be flow-laid-out siblings of the plot with hand-matched fixed sizes, and everything
   inside the plot rect (grid, cursor line, selection band) has to be drawn into the gfx frame
   instead. Recorded before writing code. Tag: missing-feature.

4. **`on_drag` never tells you where the pointer is.** Handlers section: "`x, y` are the
   dragged element's screen-space origin (for root-level ghosts), `dx, dy` are movement in its
   local content space". A range selection needs the *press* position in element space, which
   no drag phase provides. Plan: remember the last `on_hover` x (hover "fires only when the
   pointer moves", and a press is preceded by entering the element) and reconstruct
   `x0 + dx`. Noting it now as a guess that may not hold for a press with no prior move.
   Tag: missing-feature.

5. **First full run was green on the first try** (`cargo test -p app_host dashboard`, 1 passed),
   which is worth saying: the props table, the handler signatures and the `gfx` command list are
   accurate enough to write ~350 lines of Lua blind. Everything below is friction *inside* that.

6. **`gfx.frame({width=..., height=..., items...})` — how do I build items in a loop?** I need a
   variable number of grid lines/points, and the docs only show the literal form. Guessed:
   accumulate positional entries in a table and set `items.width` / `items.height` as named keys
   on the same table, then `gfx.frame(items)`. Worked. Tag: docs-gap (a one-line "built in a
   loop" note would remove the guess). `baseline` is listed but never explained; I omitted it.

7. **Translucent brushes are undocumented.** `gfx.solid("#rrggbb")` is the only spelling given,
   while the Layout > Colors section allows `#rrggbbaa`/`rgba()` for *element* colors. A shaded
   selection band wants alpha. I did not risk an error inside the one item that has to draw, so
   the band is an opaque dark blue painted *under* the grid and the line (`theme.lua: band`).
   Tag: docs-gap.

8. **Nothing says whether a sideways drag survives a scrolling ancestor.** My root is
   `scroll_y` (Layout: "A scrolling container with `grow` uses the remaining main-axis space",
   and the resize guidance says to let only a region scroll). The plots are descendants of it.
   The docs describe gesture arbitration exactly once, for a different container: "Inside a
   `zoomable`, a press that travels past 5pt pans instead." Nothing states who wins between
   `on_drag` on a child and `scroll_y`/`scroll_x` on an ancestor, or whether a drag has a
   direction/slop threshold at all. **I cannot test this headlessly** -- `CallDrag` is injected
   below whatever does that arbitration, so my test passes either way. This is the one place the
   demo could be silently broken in the real shell. What I wanted documented: "a press on an
   element with `on_drag` claims the gesture; ancestors do not scroll", or the threshold rule if
   it is the other way. Tag: docs-gap (possible bug, unverified).

9. **`ui.frame` + handlers: I hedged.** "Publish it as a normal leaf with `ui.frame({visual=...,
   ...normal El props...})`" -- handlers are documented in their own section, not as props, so I
   couldn't tell whether `on_hover`/`on_drag` are legal on a frame. I wrapped the frame in a
   `ui.col` that carries `w`, `h`, `id` and both handlers (test asserts the shape:
   `plot:users` -> one child of kind `"frame"`). Tag: docs-gap.

10. **Headless error text has no location.** I deliberately added `bogus_prop = 7` to check my
    empty-console assert was worth anything: `app.console(100)` returned exactly
    `["unknown prop bogus_prop"]` -- no file, no line, no element id, though the docs promise
    "an error at that element ... a red box in place" and "an error banner naming file and
    line". Fine in the GUI, thin for an agent working from test output. Reverted. Tag: docs-gap.

11. **`gfx` is missing from the sandbox surface.** "Available beyond plain Lua: `doc`, `ui`,
    `require`, `now()` (unix seconds), `uuid()`" -- `gfx` has a whole section but is not in that
    list, and `string`/`math` (which I use heavily for formatting and geometry) are only implied
    by "plain Lua". Both work. Tag: docs-wrong.

12. **No axis anything.** No text measurement, no text alignment prop (`center` centers an
    element, not a glyph run inside it), no tick/"nice number" helper, no text item in `gfx`. So
    every label is a flow sibling hand-aligned to plot geometry: the y gutter is a fixed 52pt
    column with `grow` spacers (the mid label lines up only approximately, since its own text
    height shifts it), and the x ticks are left-aligned at their sample's x with the last column
    taking the remainder. My y bounds are padded 10% so the top/bottom labels are ugly numbers
    like `1921` instead of `2000`. Tag: missing-feature.

13. **`manifest.osv` syntax is not in the docs.** "`manifest.osv` is optional and only supplies
    the display name" -- no grammar, no example. The `app "Dashboard" { }` form came from the
    task, not the guide. Tag: docs-gap.

14. **Guesses that happened to work, unprompted by the docs:** building a `gfx` item list in a
    loop with `width`/`height` as named keys on it; `id` on a `ui.text` (for test-visible
    readouts); `ui.col({ w = 10 })` as a fixed-size spacer next to the documented
    `ui.col({ grow = true })` one; `dashes = { 3, 3 }` on `gfx.stroke` for the cursor line;
    `stroke = { 1, C.line }` positional; nesting a built table (`tiles`, `charts`) as a splice
    group inside a `ui.row`/`ui.col` that also has named props.

15. **What did work exactly as written, and mattered most:** `on_hover`'s "`x, y` ... from the
    element's top-left corner in its own units, zoom and scroll undone". That single sentence is
    why a shared cursor is four lines (`S.hover = Chart.index_at(x)`), and why the test can pick
    a pixel and know which month it means. Same for the resize pattern: cumulative `dx` from
    drag start, clamp every move, state in a module-scope table. I copied it verbatim for the
    range selection and it worked first try.

## Report

**Files** (all new, in `demo_apps/dashboard/`): `manifest.osv`, `main.lua` (view, cursor +
range gesture, tiles, clear), `chart.lua` (geometry, `gfx` visual, chart card), `data.lua`
(24 months x 3 series), `theme.lua`, `NOTES.md`. Plus one test appended to
`app_host/src/tests.rs`.

**Test: PASS.** `cargo test -p app_host dashboard` -> `1 passed`. It asserts: clean console on
first build; full-range tiles (`2516`, `$455500`, `1.48%`); the plot is one hover+drag surface
with a single `frame` child; hovering x=121.3 on `plot:users` marks 2024-05 in all three
readouts (`1640`, `$12100`, `1.90%`) and in the tiles; a start/move/move/end drag of +5 steps
selects 2024-05..2024-10 and every tile re-summarises (`1917`, `$85500`, `1.75%`); hovering
`plot:errors` moves the cursor in chart one while the released selection survives; `leave`
drops the cursor, not the selection; clicking `clear` restores all four texts; console still
empty. I also verified the console assert has teeth by breaking a prop on purpose (item 10).

**Friction log:** the numbered items 1-15 above, written as each one happened.

**Wished-for features, one line each**
- Pointer position in `on_drag` (`x, y` in element space at press/move), not just the element origin.
- Parent-relative `absolute` (or a `ui.layer` inside a container) so overlays can sit on a plot.
- Text in `gfx` frames -- axis labels belong in the picture, not in hand-aligned flow siblings.
- `text_align` on `ui.text`, and some way to measure a label.
- A documented rule (and a prop) for who wins a gesture between `on_drag` and an ancestor `scroll_*`.
- Translucent brushes: say whether `gfx.solid` takes `#rrggbbaa`.
- An error string that names file, line and element id, so headless agents can act on it.
- Axis/tick helpers: nice-number bounds, or a `ui.chart` primitive if charts are meant to be common.
- A props/constructor reference table for `ui.*` and `gfx.*`, given unknown props are hard errors.
- A readable reference app outside `shell2/src` -- `demo_apps/tally` in the guide is one screen, kanban is unreachable prose.

**Read beyond `docs/lua-apps.md`:** nothing. No file under `runtime/`, `shell2/`,
`app_host/src/` or `demo_apps/` was opened, and I did not run the GUI. The only extra
information I used came from my own test's output (`unknown prop bogus_prop`, and one
`eprintln!` probe that told me the `ui.frame` element's `kind` is `"frame"`, now an assert).
