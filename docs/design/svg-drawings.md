# SVG drawings — art from a real editor, as Lua data

Status: **steps 1–4 built 2026-11-05, not landed** (see docs/status.md). Plan agreed the same day. Option B: the
runtime parses, Lua gets data. Worktree `.claude/worktrees/hillclimb`, branch `hillclimb`.

Why: every look in Hillclimb is path coordinates written by hand in `art.lua`. An artist (or an
agent) should draw the car in Inkscape, name its parts, and have Lua animate them. Grows out of
[2D world](2d-world.md) §2 (the drawing format) and [Assets](assets.md) (drawings as files).

**The one idea:** `gfx.svg(name)` reads an SVG from the app's own folder and returns a
**drawing module** — the same plain table `gfx.drawing` already takes (`size`, `parts`).
No new handle type, no new render path. Rust supplies the vocabulary; Lua does the content.

```lua
local spec, report = gfx.svg("car.svg")   -- plain Lua data, parsed in Rust (usvg)
spec.parts[3].shapes[1].fill = "#c0392b"  -- recolour: it's a table
local car = gfx.drawing(spec)
local frame = car:pose({ wheel_front = { rot = spin } })  -- <g id="wheel_front"> in Inkscape
```

## 0. End-to-end tests (written first, red until their step lands)

`L` = a real sandboxed Luau VM through `app_host` (fixture SVGs as strings in the `files` map),
`S` = bridge smoke over an offscreen shell (`scripts/smoke_svg.py`, into `SMOKES` once green).

### Reading the file

| id | proof | step |
|---|---|---|
| L-V1 | `gfx.svg("art/car.svg")` reads `files["art/car.svg"]` and returns a table that `gfx.drawing` accepts unchanged | 1 |
| L-V2 | the sandbox holds: `"../x.svg"`, `"/etc/x.svg"`, a `.lua` key and a missing name are errors; the missing-name error lists the app's `.svg` files | 1 |
| L-V3 | a malformed SVG, one over 4 MiB, one declaring `<!ENTITY` (expansion bombs), or one past the 200 000 path-command cap is an error naming the file — never a half table. External files are never read: an `<image href="/etc/passwd">` is only reported | 1 |
| S-V1 | `client.py` uploads `.svg` beside `.lua`, and the shell's `app_src` keeps it (both filters say `svg`); the file reaches the VM | 1 |

### The mapping

| id | proof | step |
|---|---|---|
| L-V4 | `size` is the SVG's rendered size (`width`/`height` in CSS px, else the viewBox's), and coordinates are where a browser at 100 % draws them: the viewBox (offset, scale, `preserveAspectRatio`) is applied. A `viewBox="10 10 50 50" width="100"` square at 10,10 lands at 0,0 with size 100 | 2 |
| L-V5 | every `<g id>` (and `<use id>`) is a part with that id; a nested one has its enclosing part as `parent`; document order is draw order. Shapes outside any id'd group go to a part `root`. Shapes of a part that come *after* one of its child parts go to a continuation part `<id>~2`, `~3`… (parent `<id>`, same pivot), so posing the part still moves them. Ids inside a `<use>` clone, and a repeated id, fold into the enclosing part | 2 |
| L-V6 | transforms (group, element, `<use>`) are baked into the points: a rotated, scaled `<rect>` inside a translated group lands where a browser draws it (checked against hand-computed corners, ±0.01) | 2 |
| L-V7 | `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon` and `path` (arcs included) become `move/line/quad/cubic/close` commands | 2 |
| L-V8 | a part's `pivot` is `data-pivot="x y"` on the group (in drawing coordinates) if present, else the centre of the group's bounding box; `root`'s is the drawing's centre. A bad `data-pivot` is an error naming the group | 2 |
| L-V9 | fills: solid colour, `fill-opacity`/`opacity` folded into alpha; linear and radial gradients become `{ linear = … }`/`{ radial = … }` with stops, `extend` and points in drawing space (a skewed linear gradient stays exact; a radial one under non-uniform scale is reported); `fill-rule: evenodd` becomes `rule = "evenodd"`, which drawing shapes gain (Inkscape writes it by default) | 3 |
| L-V10 | strokes: width (scaled by the transform), colour with `stroke-opacity`, `cap`, `join`, `miter_limit`, `dashes` and `dash_offset` carry over — drawing strokes gain those as optional named fields after `{ w, colour }` (unnamed stays round, as now). A gradient or pattern stroke is reported in `report.dropped` and not drawn | 3 |
| L-V11 | anything the format can't carry — `filter`, `mask`, `clip-path`, `pattern`, `image`, blend modes, `<text>`, stroke-under-fill paint order — is dropped and listed in `report.dropped` as `{ feature, id }`. `opacity` on a group is folded into each of its shapes, and listed when the group has more than one (overlaps then show through). Nothing changes silently | 3 |

### Through the real path

| id | proof | step |
|---|---|---|
| L-V12 | the hover on a `ui.frame` of an SVG drawing reports the `<g id>` under the pointer as `e.shape` | 4 |
| S-V2 | Hillclimb's wheels and helmet are `art/wheel.svg` / `art/helmet.svg` (*revised 2026-11-05: the wheels are their own bodies, so the physics turns them; the chassis stays Lua because its shape follows the tuned mounts*): screenshot shows them, console clean | 4 |
| S-V3 | **hot edit**: `edit_file` on `wheel.svg` changing one fill (bridge, `read_file_versioned` + exact replace) reloads the app and the pixel under the body changes colour | 4 |
| S-6′ | the Hillclimb smokes stay green (art changes, physics doesn't) | all |

## 1. Decisions to make before step 1

1. **Return shape.** Two values, `spec, report`, so `gfx.drawing`'s strict fields stay
   strict. *Proposed.*
2. **Which ids make parts.** Inkscape gives every group an auto id (`g1234`, `layer1`). Those
   become parts too — harmless, a few extra groups. Alternative: only ids not matching
   `^g%d+$`/`^layer%d+$`. *Proposed: all group ids; revisit if dumps get noisy.*
3. **Pivot.** `data-pivot` (set in Inkscape's XML editor) or bbox centre. Inkscape's own
   rotation centre (`inkscape:transform-center-x/y`) is an offset only Inkscape writes; reading it
   is a later nicety.
4. **Text.** usvg outlines text only with fonts loaded. *Proposed: drop and report; "Object to
   Path" in Inkscape fixes it.*

*All four agreed 2026-11-05.* Found while reading usvg 0.48: it keeps every `<g>`/`<use>` with
its id and bakes the viewBox into transforms, so size is the rendered size (L-V4) rather than the
viewBox; text needs its `text` feature, which stays off (no font loading, no filesystem).
5. **Caching.** Parsed on every call, like `gfx.drawing`; call at module scope. No cache.

## 2. Where the code goes

- `app_host/src/gfx/svg.rs` (new): `usvg` parse → walk the tree → Lua tables. Reads through the
  same `files` map as `modules.rs` (shared path resolver, `.svg` only). Parsing runs inside the
  call's budget; the 4 MiB cap keeps it well under.
- `runtime/` untouched: the output is the existing drawing, so the render path is the existing one.
- `shell2/src/app_src.rs` `KEEP` and `scripts/osvauld/client.py` gain `svg`.
- Docs: `lua-apps.md` (Drawings section), `status.md`.

## 3. Steps

1. **Read** (L-V1–V3, S-V1): `gfx.svg` reaches the file, refuses what it should; the table it
   returns is a single empty-safe part.
2. **Geometry** (L-V4–V8): shapes, transforms, parts and pivots.
3. **Paint** (L-V9–V11): fills, gradients, strokes, the dropped report.
4. **Hillclimb uses it** (L-V12, S-V2, S-V3): the car body and wheels move to `car.svg`, drawn
   by us; screenshots to the user.

Not in scope: raster images (`gfx.image`, its own plan), group opacity and blend modes (a
small runtime addition, if art needs them), the Tiled stage importer (its own plan).
