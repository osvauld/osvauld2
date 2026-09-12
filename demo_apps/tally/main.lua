-- TALLY
--
-- The smallest real app: a counter that lives in the CRDT doc, so the number survives a
-- restart and, later, syncs to a peer. One read (the mirror), one write (the doc), and
-- nothing clever anywhere.

local t = doc:open("tally")

-- Module scope runs on every open, so the seed is guarded: without the guard, reopening
-- the app would zero the count it just loaded.
if not t.count then
	t:set({ "count" }, doc.map({ n = 0 }))
end

local C = {
	bg = "#0d1117",
	card = "#1c2128",
	card_hi = "#22282f",
	sunken = "#0d1117",
	line = "#30363d",
	text = "#e6edf3",
	muted = "#8b949e",
	accent = "#2f81f7",
	accent_hi = "#4493f8",
}

-- Read the mirror first, then write: `:set` lands in Loro immediately, but the mirror is
-- only repatched on the next frame — reading back your own write returns yesterday's news.
local function bump(d)
	local n = t.count and t.count.n or 0
	t:set({ "count", "n" }, math.max(0, n + d))
end

local function pill(label, delta, fill, hover)
	return ui.button({
		w = 44,
		h = 44,
		radius = 10,
		center = true,
		fill = fill,
		hover_fill = hover,
		ui.text({ label, color = "#ffffff", font_size = 20, no_wrap = true }),
		on_click = function()
			bump(delta)
		end,
	})
end

return function()
	local n = t.count and t.count.n or 0

	return ui.col({
		id = "tally",
		grow = true,
		stretch = true,
		center = true,
		fill = C.bg,
		gap = 18,
		ui.text({ "TALLY", color = C.muted, font_size = 12, no_wrap = true }),
		ui.col({
			w = 220,
			h = 180,
			radius = 16,
			center = true,
			fill = C.card,
			stroke = { 1, C.line },
			ui.text({ tostring(n), color = C.text, font_size = 72, no_wrap = true }),
		}),
		ui.row({
			gap = 12,
			align_center = true,
			pill("−", -1, C.sunken, C.card_hi),
			pill("+", 1, C.accent, C.accent_hi),
		}),
		ui.button({
			h = 30,
			px = 14,
			radius = 8,
			center = true,
			fill = C.card,
			hover_fill = C.card_hi,
			ui.text({ "reset", color = C.muted, font_size = 12, no_wrap = true }),
			on_click = function()
				t:set({ "count", "n" }, 0)
			end,
		}),
	})
end
