local C = require("theme")
local slides = require("content")

-- The current slide lives in the document, keyed by slide id, so a synced screen follows
-- the presenter and a reordered deck keeps its place.
local deck = doc:open("deck")
if not deck.pos then
	deck:set({ "pos" }, doc.map({ id = slides[1].id }))
end
if not deck.tally then
	deck:set({ "tally" }, doc.map({ n = 0 }))
end

local function index_of(id)
	for i, s in ipairs(slides) do
		if s.id == id then return i end
	end
	return 1
end

local function go(i)
	i = math.max(1, math.min(#slides, i))
	deck:set({ "pos", "id" }, slides[i].id)
end

local NEXT = { ArrowRight = 1, ArrowDown = 1, Space = 1, PageDown = 1 }
local PREV = { ArrowLeft = true, ArrowUp = true, PageUp = true }

local function rect(w, h)
	return gfx.path({ { "move", 0, 0 }, { "line", w, 0 }, { "line", w, h }, { "line", 0, h }, { "close" } })
end

local function gradient(w, h, stops, to)
	return gfx.frame({
		width = w, height = h,
		gfx.fill({
			path = rect(w, h),
			brush = gfx.linear_gradient({ from = { 0, 0 }, to = to or { w, 0 }, stops = stops }),
		}),
	})
end

-- Frames have a fixed size and Lua can't read the window's, so the wash is drawn oversized
-- and its gradient spans a 1920×1080 screen; `pad` extends the end colours beyond that.
local WASH = gradient(3840, 2160, {
	{ 0, C.wash_from }, { 0.45, "rgba(0,0,0,0)" }, { 1, C.wash_to },
}, { 1920, 1080 })
local ACCENT = gradient(120, 5, { { 0, C.acid }, { 1, C.blue } })
local ACCENT_WIDE = gradient(280, 8, { { 0, C.acid }, { 1, C.blue } })

local function txt(s, size, color)
	return ui.text({ s, font_size = size, color = color or C.ink })
end

local function heading(s, size)
	return ui.col({
		gap = 22,
		s.eyebrow and txt(s.eyebrow, C.eyebrow, C.acid) or false,
		txt(s.title, size or C.h2),
		ui.frame({ visual = size and ACCENT_WIDE or ACCENT }),
	})
end

local render = {}

function render.title(s)
	return ui.col({
		grow = true, gap = 28,
		ui.col({ grow = true }),
		heading(s, C.h1),
		s.sub and txt(s.sub, C.body, C.muted) or false,
		ui.col({ grow = true }),
		s.foot and txt(s.foot, C.small, C.muted) or false,
	})
end

function render.bullets(s)
	local points = {}
	for _, p in ipairs(s.points) do
		points[#points + 1] = ui.row({
			-- stretch + a one-line-tall dash box pins the dash to a wrapped point's first line
			gap = 20, stretch = true,
			ui.col({ h = math.floor(C.body * 1.35), center = true, txt("—", C.body, C.acid) }),
			ui.col({ grow = true, txt(p, C.body) }),
		})
	end
	return ui.col({ gap = 56, heading(s), ui.col({ gap = 26, points }) })
end

function render.table(s)
	local rows = {}
	for i, r in ipairs(s.rows) do
		local head = i == 1
		local size = head and C.eyebrow or C.body
		if not head then rows[#rows + 1] = ui.col({ h = 1, fill = C.line }) end
		rows[#rows + 1] = ui.row({
			py = head and 8 or 18, gap = 32,
			ui.col({ w = 380, no_shrink = true, txt(r[1], size, head and C.muted or C.ink) }),
			ui.col({ grow = true, txt(r[2], size, head and C.muted or C.acid) }),
		})
	end
	return ui.col({ gap = 48, heading(s), ui.col({ rows }) })
end

function render.cards(s)
	local function card(c)
		return ui.col({
			-- w = 0: grow shares only leftover space, so equal cards need an equal starting width.
			grow = true, w = 0, pad = 36, gap = 16, radius = 16,
			fill = C.panel, stroke = { 1, C.line },
			txt(c[1], C.body, C.acid),
			txt(c[2], C.small, C.ink),
		})
	end
	local c = s.cards
	return ui.col({
		gap = 48, heading(s),
		ui.col({
			gap = 24,
			ui.row({ gap = 24, stretch = true, card(c[1]), card(c[2]) }),
			ui.row({ gap = 24, stretch = true, card(c[3]), card(c[4]) }),
		}),
	})
end

-- The code slide's live half: the same col/row tree as the snippet, with each box outlined
-- and named so the audience can match description to picture.
local function boxed(tag, color, el)
	return ui.col({
		gap = 10, pad = 20, radius = 12, stroke_dash = { 2, color, 8, 6 },
		txt(tag, C.eyebrow, color), el,
	})
end

local function tally_preview()
	local n = deck.tally and deck.tally.n or 0
	local function btn(id, label, d)
		return ui.button({
			id = id, w = 72, h = 60, radius = 12, center = true,
			fill = C.panel, stroke = { 1, C.line }, hover_fill = "#1a2216", press_scale = 0.94,
			txt(label, C.body, C.ink),
			on_click = function() deck:set({ "tally", "n" }, n + d) end,
		})
	end
	return boxed("ui.col", C.acid, ui.col({
		gap = 16, align_center = true,
		ui.text({ tostring(n), font_size = 96, color = C.ink, no_wrap = true }),
		boxed("ui.row", C.blue, ui.row({ gap = 16, btn("demo:minus", "−", -1), btn("demo:plus", "+", 1) })),
	}))
end

function render.code(s)
	local lines = {}
	for line in (s.code .. "\n"):gmatch("(.-)\n") do
		lines[#lines + 1] = ui.text({ line == "" and " " or line, no_wrap = true,
			font_size = C.code, color = C.ink })
	end
	return ui.col({
		gap = 40, heading(s),
		ui.row({
			gap = 48,
			ui.col({ grow = 3, w = 0, pad = 32, gap = 4, radius = 14, fill = C.panel,
				stroke = { 1, C.line }, lines }),
			ui.col({ grow = 2, w = 0, gap = 24, align_center = true,
				tally_preview(), txt(s.note, C.small, C.muted) }),
		}),
	})
end

-- A whole app on a slide: its main.lua returns its view function, so the deck just calls it.
-- Demos are mounted under demos/<name>/ at upload (require resolves from the app root, so
-- their requires are rewritten there); without them the slide says so instead of erroring.
-- Required at open, not mid-view: a demo seeds its documents at module scope, and a seed made
-- inside a frame isn't readable until the next one (the mirror is a frame behind the write).
-- Per-app keyboard zoom (Ctrl +/-/0), for presenting without a mouse. Viewer state, not doc:
-- the presenter's zoom is not a claim about the deck.
local ZOOM = {}  -- app -> { z, tx, ty }: screen-local = z * local + t
local HOVER = {} -- app -> the pointer's last position, in the app's own (unscaled) units
local ZOOM_KEYS = { Equal = 1.25, NumpadAdd = 1.25, Minus = 0.8, NumpadSubtract = 0.8 }

-- `scale` grows from the element's top-left and `offset` shifts it, so zooming around the
-- pointer is: keep the point under it fixed, z*u + t == z'*u + t'.
local function zoom_by(app, f)
	local Z = ZOOM[app] or { z = 1, tx = 0, ty = 0 }
	local nz = math.max(0.5, math.min(4, Z.z * f))
	local h = HOVER[app] or { x = 0, y = 0 }
	ZOOM[app] = { z = nz, tx = Z.tx + (Z.z - nz) * h.x, ty = Z.ty + (Z.z - nz) * h.y }
end

local VIEWS, LOAD_ERR = {}, {}
for _, s in ipairs(slides) do
	if s.kind == "app" then
		local ok, v = pcall(require, "demos/" .. s.app .. "/main")
		if ok then VIEWS[s.app] = v else LOAD_ERR[s.app] = tostring(v) end
	end
end

function render.app(s)
	local view = VIEWS[s.app]
	-- A one-line header: the app is the slide, so the words stay small.
	return ui.col({
		grow = true, gap = 16,
		ui.row({
			gap = 14, align_center = true,
			ui.col({ w = 6, h = 28, radius = 3, fill = C.acid }),
			txt(s.title, C.small, C.ink),
		}),
		-- Ctrl+wheel zooms the app around the pointer; an app's own drags still win over the pan.
		-- scroll_y (on an inner box — not verified alongside zoomable on one element) makes the
		-- frame take the remaining height instead of growing to the app's.
		ui.col({
			id = "zoom:" .. s.app, grow = true, zoomable = true, radius = 16, stroke = { 1, C.line },
			ui.col({
				id = "app:" .. s.app, grow = true, scroll_y = true,
				-- scale grows from the top-left and hit-testing follows it, so buttons still work
				ui.col({
					id = "scaled:" .. s.app, grow = true,
					scale = (ZOOM[s.app] or {}).z or 1,
					offset = { (ZOOM[s.app] or {}).tx or 0, (ZOOM[s.app] or {}).ty or 0 },
					on_hover = function(e)
						if e.phase ~= "leave" then HOVER[s.app] = { x = e.x, y = e.y } end
					end,
					view and view() or txt(LOAD_ERR[s.app] or ("demos/" .. s.app .. " is not mounted"), C.small, C.muted),
				}),
			}),
		}),
	})
end

local function footer(i)
	local n = #slides
	local function nav(id, label, to)
		return ui.button({
			id = id, w = 40, h = 32, radius = 8, center = true,
			hover_fill = C.panel, press_scale = 0.94,
			txt(label, C.small, C.muted),
			on_click = function() go(to) end,
		})
	end
	return ui.row({
		align_center = true, gap = 20,
		ui.row({
			grow = true, h = 3, radius = 2,
			ui.col({ grow = i, fill = C.acid }),
			i < n and ui.col({ grow = n - i, fill = C.line }) or false,
		}),
		txt(string.format("%d / %d", i, n), C.small, C.muted),
		nav("prev", "‹", i - 1),
		nav("next", "›", i + 1),
	})
end

return function()
	local i = index_of(deck.pos and deck.pos.id)
	local s = slides[i]
	local body = render[s.kind](s)
	if s.kind ~= "title" and s.kind ~= "app" then
		body = ui.col({ grow = true, ui.col({ grow = true }), body, ui.col({ grow = 2 }) })
	end
	return ui.col({
		id = "deck", full = true, fill = C.bg, px = C.pad_x, py = C.pad_y, gap = 32,
		ui.frame({ visual = WASH, absolute = true, top = 0, left = 0 }),
		on_key = function(e)
			if not e.down or e.cancelled then return end
			if e.ctrl and s.kind == "app" then
				if ZOOM_KEYS[e.code] then
					zoom_by(s.app, ZOOM_KEYS[e.code])
				elseif e.code == "Digit0" or e.code == "Numpad0" then
					ZOOM[s.app] = nil
				end
				return
			end
			if NEXT[e.code] then go(i + 1)
			elseif PREV[e.code] then go(i - 1)
			elseif e.code == "Home" then go(1)
			elseif e.code == "End" then go(#slides) end
		end,
		ui.col({
			id = "slide:" .. s.id, grow = true, fade_in = 220, slide_in = { { 24, 0 }, 220 },
			body,
		}),
		footer(i),
	})
end
