--- @meta
--- OSVAULD SANDBOX GLOBALS — GENERATED, DO NOT EDIT.
---
--- Source of truth is the running sandbox and `app_host/src/props.rs`. Regenerate with
--- `BLESS=1 cargo test -p app_host generated_defs_are_current`; the same test fails when this
--- file drifts, which is the point of generating it (gap-log 1.6).
---
--- This teaches an editor the *names*. `docs/lua-apps.md` is still the contract for what they
--- mean, and unknown props are a hard error at runtime, not a warning here.

--- @class El
El = {}

--- @class GfxResource
GfxResource = {}

--- @class Doc
Doc = {}

--- The element constructors.
--- @class ui
ui = {}

--- Element. Children are positional entries; every other key is a prop.
--- @param spec table
--- @return El
function ui.button(spec) end

--- Element. Children are positional entries; every other key is a prop.
--- @param spec table
--- @return El
function ui.col(spec) end

--- Element. Children are positional entries; every other key is a prop.
--- @param spec table
--- @return El
function ui.frame(spec) end

--- Element. Children are positional entries; every other key is a prop.
--- @param spec table
--- @return El
function ui.input(spec) end

--- Element. Children are positional entries; every other key is a prop.
--- @param spec table
--- @return El
function ui.overlay(spec) end

--- Element. Children are positional entries; every other key is a prop.
--- @param spec table
--- @return El
function ui.row(spec) end

--- Element. Children are positional entries; every other key is a prop.
--- @param spec table
--- @return El
function ui.text(spec) end

--- Element. Children are positional entries; every other key is a prop.
--- @param spec table
--- @return El
function ui.text_area(spec) end

--- Per-viewer scratch state, keyed by id and carried across a hot reload. Not in the document:
--- nothing here syncs to a peer.
--- @param id string
--- @param init table|nil
--- @return table
function ui.state(id, init) end

--- Drawing resources for `ui.frame({ visual = ... })`.
--- @class gfx
gfx = {}

--- @param ... any
--- @return GfxResource
function gfx.fill(...) end

--- @param ... any
--- @return GfxResource
function gfx.frame(...) end

--- @param ... any
--- @return GfxResource
function gfx.group(...) end

--- @param ... any
--- @return GfxResource
function gfx.instance(...) end

--- @param ... any
--- @return GfxResource
function gfx.linear_gradient(...) end

--- @param ... any
--- @return GfxResource
function gfx.path(...) end

--- @param ... any
--- @return GfxResource
function gfx.solid(...) end

--- @param ... any
--- @return GfxResource
function gfx.stroke(...) end

--- The document API — CRDT-backed, shared, persisted.
--- @class doc
doc = {}

--- @param ... any
--- @return any
function doc.list(...) end

--- @param ... any
--- @return any
function doc.map(...) end

--- @param ... any
--- @return any
function doc.text(...) end

--- This app's document, by name.
--- @param name string
--- @return Doc
function doc.open(name) end

--- Seconds since the Unix epoch. Wall clock: it can step backwards, so never measure a
--- duration with it. For elapsed time use `e.elapsed` in `on_frame` or `e.t` on a pointer event.
--- @return number
function now() end

--- @return string
function uuid() end

-- Props any element accepts (unknown ones are errors):
--   absolute, align_center, autofocus, bottom, center, color
--   fade, fade_in, fill, font_size, full, gap
--   grow, h, h_full, hover_fill, hover_stroke, id
--   left, line, max_h, max_w, mb, min_h
--   min_w, mt, no_shrink, no_wrap, offset, on_input
--   opacity, pad, press_fill, press_scale, press_stroke, px
--   py, radius, right, scale, scroll_x, scroll_y
--   size, slide_in, stretch, stroke, stroke_dash, tag
--   tint, top, value, w, w_full, wrap
--   zoom_x, zoomable

-- Handler props:
--   on_click, on_drag, on_drop, on_enter, on_esc, on_faded_out
--   on_frame, on_hover
