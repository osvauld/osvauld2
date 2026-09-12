-- SCRATCH
--
-- The smallest app that uses the whole loop: a note list whose state lives in the CRDT doc
-- (so it survives a restart and, later, syncs), a composer whose draft lives in ui.state
-- (per-viewer scratch), and nothing clever anywhere.
--
-- The one rule that shapes every handler below: the mirror is a frame behind your own write.
-- Read what you need first, then write; never read back what you just wrote.

local C = {
	bg = "#0d1117",
	card = "#1c2128",
	card_hi = "#22282f",
	sunken = "#0d1117",
	line = "#30363d",
	line_soft = "#21262d",
	text = "#e6edf3",
	muted = "#8b949e",
	accent = "#2f81f7",
	accent_hi = "#4493f8",
	danger = "#f85149",
}

local t = doc:open("scratch")

-- Module scope runs on every open, so the seed is guarded: without it, reopening the app
-- would wipe the notes it just loaded.
if not t.notes then
	t:set({ "notes" }, doc.list({
		doc.map({ id = uuid(), text = "write a small app in lua", ts = now() }),
		doc.map({ id = uuid(), text = "render it, type into it, restart it", ts = now() }),
	}))
end

-- A relative timestamp is a pure derivation of `now()`, so it is computed per frame and never
-- stored: a stored "2m ago" is wrong a minute later, on every peer.
local function ago(ts)
	local d = now() - ts
	if d < 60 then
		return d .. "s"
	end
	if d < 3600 then
		return math.floor(d / 60) .. "m"
	end
	return math.floor(d / 3600) .. "h"
end

local function add()
	local s = ui.state("draft", { text = "" })
	-- `""` is truthy in Lua, so the guard needs both checks.
	if not s.text or s.text == "" then
		return
	end
	-- The append form of :insert, not a counted position: `#t.notes` is a frame behind once
	-- anything has been written this frame.
	t:insert({ "notes" }, doc.map({ id = uuid(), text = s.text, ts = now() }))
	s.text = ""
end

local function row_of(n)
	return ui.row({
		id = n.id,
		gap = 8,
		px = 12,
		py = 9,
		radius = 8,
		align_center = true,
		fill = C.card,
		hover_fill = C.card_hi,
		fade_in = 140,
		ui.col({ grow = true, ui.text({ n.text, color = C.text, font_size = 13 }) }),
		ui.text({ ago(n.ts), no_wrap = true, color = C.muted, font_size = 11 }),
		ui.button({
			w = 22,
			h = 22,
			radius = 6,
			center = true,
			no_shrink = true,
			hover_fill = C.danger,
			ui.text({ "x", no_wrap = true, color = C.muted, font_size = 13 }),
			on_click = function()
				t:delete({ "notes", n.id })
			end,
		}),
	})
end

return function()
	local notes = t.notes
	local rows = {}
	for i = 1, #notes do
		rows[#rows + 1] = row_of(notes[i])
	end
	if #notes == 0 then
		rows[#rows + 1] = ui.col({
			h = 64,
			center = true,
			radius = 8,
			stroke = { 1, C.line_soft },
			ui.text({ "nothing scratched yet", no_wrap = true, color = C.muted, font_size = 12 }),
		})
	end

	local s = ui.state("draft", { text = "" })
	return ui.col({
		id = "scratch",
		full = true,
		fill = C.bg,
		-- top bar
		ui.row({
			gap = 10,
			px = 24,
			py = 14,
			align_center = true,
			ui.text({ "Scratch", no_wrap = true, color = C.text, font_size = 18 }),
			ui.row({
				px = 7,
				py = 1,
				radius = 10,
				fill = C.line_soft,
				center = true,
				no_shrink = true,
				ui.text({ tostring(#notes), no_wrap = true, color = C.muted, font_size = 11 }),
			}),
			ui.col({ grow = true }),
		}),
		ui.col({ h = 1, w_full = true, fill = C.line_soft }),
		-- the list. Scroll offsets are retained per id, so the scroller needs one.
		ui.col({
			id = "list",
			scroll_y = true,
			grow = true,
			px = 16,
			py = 14,
			gap = 8,
			rows,
		}),
		-- composer
		ui.row({
			gap = 8,
			px = 16,
			py = 14,
			ui.input({
				value = s.text,
				id = "draft",
				grow = true,
				h = 34,
				px = 10,
				radius = 6,
				fill = C.sunken,
				stroke = { 1, C.line },
				color = C.text,
				font_size = 13,
				on_input = function(v)
					s.text = v
				end,
				on_enter = add,
			}),
			ui.button({
				w = 64,
				h = 34,
				radius = 6,
				center = true,
				no_shrink = true,
				fill = C.accent,
				hover_fill = C.accent_hi,
				ui.text({ "add", no_wrap = true, color = "#ffffff", font_size = 13 }),
				on_click = add,
			}),
		}),
	})
end
