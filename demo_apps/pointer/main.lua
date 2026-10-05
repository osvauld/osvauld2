-- POINTER
--
-- The showcase the demo recorder films: two counters to click, and a zoomable board of cards.
-- The cursor is `cursor.lua`, mounted on the root like any app would; what it looks like is
-- declared by what it is over — the cards say "grab", the board hands over its own drawing.

local cursor = require("cursor")

local C = {
	bg = "#0f1117",
	panel = "#171a22",
	line = "#2a2f3a",
	muted = "#8a91a0",
	accent = "#3b6ef5",
	accent_hi = "#5582ff",
	cards = { "#2e7d5b", "#7a3e9d", "#b4532a" },
}

-- The board's own look for the cursor: a magnifier, saying "this zooms". The board supplies it;
-- cursor.lua only draws it.
local MAGNIFIER = (function()
	local ring = {}
	for i = 0, 15 do
		local a = i / 16 * 2 * math.pi
		ring[#ring + 1] = { i == 0 and "move" or "line", 10 + 7 * math.cos(a), 10 + 7 * math.sin(a) }
	end
	ring[#ring + 1] = { "close" }
	local handle = { { "move", 15, 15 }, { "line", 22, 22 } }
	local items = { width = 24, height = 24 }
	for _, pass in ipairs({ { "#f5f5f5", 5 }, { "#111318", 2 } }) do
		items[#items + 1] = gfx.stroke({ path = gfx.path(ring), brush = gfx.solid(pass[1]), width = pass[2] })
		items[#items + 1] = gfx.stroke({ path = gfx.path(handle), brush = gfx.solid(pass[1]), width = pass[2] + 1, cap = "round" })
	end
	return gfx.frame(items)
end)()

local counts = { one = 0, two = 0 }
local picked = nil

local function counter(id, label)
	return ui.button({
		id = id,
		w = 150,
		h = 52,
		radius = 10,
		center = true,
		gap = 8,
		fill = C.accent,
		hover_fill = C.accent_hi,
		press_scale = 0.96,
		on_click = function()
			counts[id] = counts[id] + 1
		end,
		ui.text({ label, color = "#ffffff", font_size = 16, no_wrap = true }),
		ui.text({ tostring(counts[id]), id = id .. "-count", color = "#ffffff", font_size = 16, no_wrap = true }),
	})
end

local function card(id, title, fill)
	local on = picked == id
	return ui.col({
		id = id,
		w = 180,
		h = 110,
		pad = 14,
		gap = 6,
		radius = 12,
		fill = fill,
		stroke = on and { 3, "#ffffff" } or { 1, C.line },
		cursor = "grab",
		on_click = function()
			picked = (not on) and id or nil
		end,
		ui.text({ title, color = "#ffffff", font_size = 18, no_wrap = true }),
		ui.text({ on and "picked" or "click to pick", color = "#ffffffaa", font_size = 13, no_wrap = true }),
	})
end

return function()
	return ui.col({
		id = "root",
		grow = true,
		stretch = true,
		fill = C.bg,
		pad = 24,
		gap = 20,
		on_hover = cursor.track,
		system_cursor = false,
		ui.row({
			gap = 14,
			counter("one", "One"),
			counter("two", "Two"),
		}),
		ui.text({ "Ctrl+wheel the board to zoom", color = C.muted, font_size = 13, no_wrap = true }),
		ui.col({
			id = "board",
			grow = true,
			zoomable = true,
			cursor = MAGNIFIER,
			fill = C.panel,
			radius = 14,
			pad = 24,
			ui.row({
				gap = 18,
				card("card-a", "Plan", C.cards[1]),
				card("card-b", "Build", C.cards[2]),
				card("card-c", "Ship", C.cards[3]),
			}),
		}),
		cursor.view(),
	})
end
