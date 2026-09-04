-- KANBAN
--
-- The entry point: it wires the parts together and owns the view. `require` resolves against
-- this app's own source doc, so these three names are `theme.lua`, `model.lua` and
-- `ui/widgets.lua` sitting next to this file in the uploaded folder.
--
-- The split runs along one line: **anything that touches pointer state stays here.** `drag`,
-- `placement` and `col_modal` are per-viewer UI state that handlers write and the view reads,
-- so every piece that reads them (`drop_slot`, `card_of`, `column_of`) lives in this file where
-- the read and the write are visible together. `ui/widgets.lua` holds the pieces that are pure
-- functions of their arguments, and `model.lua` holds the doc.
local C = require("theme")
local W = require("ui/widgets")
local model = require("model")

local board, update = model.board, model.update
-- One shared table, not a copy: `S.drag` is written by handlers inside model.lua and read here.
-- A `local drag` on either side could not do this — a local is file-scoped, so the assignment
-- would be invisible across the boundary.
local S = model.state

-- VIEW PIECES

-- Which gap the indicator belongs in: 1 = above the first card, #list+1 = below the last.
-- A column-background drop ("into") appends, which is the trailing gap — same answer, one path.
local function drop_slot(c, list)
	if not (S.drag and S.placement) then
		return nil
	end
	if S.placement.kind == "into" then
		return S.placement.col == c.id and #list + 1 or nil
	end
	if S.placement.kind ~= "card" or S.drag.id == S.placement.id then
		return nil
	end
	for i = 1, #list do
		if list[i].id == S.placement.id then
			return S.placement.before and i or i + 1
		end
	end
end

-- `ghost` builds the floating copy: same look, but no id and no handlers, because a duplicate
-- id would register a second drag target and collide in the state store.
local function card_of(c, ghost)
	local dragging = S.drag ~= nil and S.drag.kind == "card" and S.drag.id == c.id
	local t = ui.row({
		gap = 8,
		px = 10,
		py = 9,
		radius = 8,
		fill = ghost and C.card_hi or C.card,
		stroke = { 1, ghost and C.accent or C.line_soft },
		align_center = true,
		ui.col({ grow = true, ui.text({ c.text, color = C.text, font_size = 13 }) }),
	})
	if ghost then
		return t
	end
	t.id = c.id
	t.hover_fill = C.card_hi
	t.hover_stroke = { 1, C.line }
	t.opacity = dragging and 0.3 or 1.0
	t.on_drag = function(phase, x, y)
		update({ kind = "drag", what = "card", id = c.id, phase = phase, x = x, y = y })
	end
	t.on_drop = function(phase, x, y)
		update({ kind = "drop", id = c.id, phase = phase, x = x, y = y })
	end
	t[#t + 1] = W.icon_button("x", function()
		update({ kind = "delete", id = c.id })
	end, C.danger)
	return t
end

local function composer(c)
	local s = ui.state("draft:" .. c.id, { text = "" })
	local send = function()
		update({ kind = "add", col = c.id })
	end
	return ui.col({
		gap = 8,
		px = 10,
		py = 10,
		ui.input({
			value = s.text,
			id = "draft:" .. c.id,
			w_full = true,
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
			on_enter = send,
		}),
		ui.button({
			w_full = true,
			h = 32,
			radius = 6,
			center = true,
			fill = C.accent,
			hover_fill = C.accent_hi,
			ui.text({ "Add card", color = "#ffffff", font_size = 13 }),
			on_click = send,
		}),
	})
end

local function column_of(c, list)
	local body = {}
	local slot = drop_slot(c, list)
	for i = 1, #list do
		body[#body + 1] = W.guide("g:" .. c.id .. ":" .. i, slot == i)
		body[#body + 1] = card_of(list[i])
	end
	body[#body + 1] = W.guide("g:" .. c.id .. ":tail", slot == #list + 1)
	if #list == 0 then
		body[#body + 1] = W.empty_slot(slot ~= nil)
	end

	local lifting = S.drag ~= nil and S.drag.kind == "col" and S.drag.id == c.id
	return ui.col({
		id = c.id,
		w = 300,
		radius = 12,
		fill = C.panel,
		stroke = { 1, C.line_soft },
		opacity = lifting and 0.35 or 1.0,
		on_drop = function(phase, x, y)
			update({ kind = "drop_col", col = c.id, phase = phase, x = x })
		end,
		-- header doubles as the column's drag handle
		ui.row({
			id = "colhead:" .. c.id,
			gap = 8,
			px = 12,
			py = 11,
			align_center = true,
			hover_fill = C.line_soft,
			on_drag = function(phase, x, y)
				update({ kind = "drag", what = "col", id = c.id, phase = phase, x = x, y = y })
			end,
			ui.text({ c.name, color = C.text, font_size = 14 }),
			W.badge(#list),
			ui.col({ grow = true }),
			W.icon_button("x", function()
				update({ kind = "delete_col", id = c.id })
			end, C.danger),
		}),
		ui.col({ h = 1, w_full = true, fill = C.line_soft }),
		-- cards
		ui.col({
			id = "cards:" .. c.id,
			scroll_y = true,
			grow = true,
			px = 10,
			py = 7,
			body,
		}),
		composer(c),
	})
end

local function modal()
	local m = ui.state("col_modal", { name = "" })
	local create = function()
		update({ kind = "add_col" })
	end
	local panel = ui.col({
		w = 360,
		gap = 14,
		pad = 20,
		radius = 14,
		fill = C.panel,
		stroke = { 1, C.line },
		fade_in = 120,
		on_click = function() end,
		ui.text({ "New column", color = C.text, font_size = 16 }),
		ui.input({
			value = m.name,
			id = "col_name",
			autofocus = true,
			w_full = true,
			h = 36,
			px = 10,
			radius = 6,
			fill = C.sunken,
			stroke = { 1, C.line },
			color = C.text,
			font_size = 13,
			on_input = function(v)
				m.name = v
			end,
			on_enter = create,
			on_esc = function()
				update({ kind = "close_col" })
			end,
		}),
		ui.row({
			gap = 8,
			ui.col({ grow = true }),
			ui.button({
				h = 34,
				px = 14,
				radius = 6,
				center = true,
				hover_fill = C.line_soft,
				ui.text({ "Cancel", color = C.muted, font_size = 13 }),
				on_click = function()
					update({ kind = "close_col" })
				end,
			}),
			ui.button({
				h = 34,
				px = 16,
				radius = 6,
				center = true,
				fill = C.accent,
				hover_fill = C.accent_hi,
				ui.text({ "Create", color = "#ffffff", font_size = 13 }),
				on_click = create,
			}),
		}),
	})
	return ui.col({
		absolute = true,
		top = 0,
		left = 0,
		right = 0,
		bottom = 0,
		center = true,
		fill = "rgba(1,4,9,0.72)",
		on_click = function()
			update({ kind = "close_col" })
		end,
		panel,
	})
end

-- ROOT
return function()
	-- Bind the mirror once per frame. `view()` repatches it before calling this, so these two
	-- are current for the whole frame — and reading them once keeps every count and index below
	-- consistent with each other.
	local columns, cards = board.columns, board.cards
	local grouped = model.by_column()
	local col_list = {}
	local flying = nil

	local col_slot = nil
	if S.drag and S.placement and S.placement.kind == "col" and S.drag.id ~= S.placement.id then
		for i = 1, #columns do
			if columns[i].id == S.placement.id then
				col_slot = S.placement.before and i or i + 1
				break
			end
		end
	end

	for i = 1, #columns do
		local c = columns[i]
		col_list[#col_list + 1] = W.guide_v("gv:" .. i, col_slot == i)
		col_list[#col_list + 1] = column_of(c, grouped[c.id])
		if S.drag and S.drag.kind == "col" and S.drag.id == c.id then
			flying = ui.col({
				w = 300,
				radius = 12,
				pad = 12,
				fill = C.panel,
				stroke = { 1, C.accent },
				ui.text({ c.name, color = C.text, font_size = 14 }),
			})
		end
	end
	col_list[#col_list + 1] = W.guide_v("gv:tail", col_slot == #columns + 1)

	if S.drag and S.drag.kind == "card" then
		for i = 1, #cards do
			if cards[i].id == S.drag.id then
				flying = card_of(cards[i], true)
				break
			end
		end
	end

	-- The ghost sits at the root, outside every scroll clip, in viewport coords: `x`/`y` are
	-- `pos - grab`, i.e. where the dragged element's top-left belongs right now.
	local ghost = flying
		and ui.col({
			absolute = true,
			left = S.drag.x,
			top = S.drag.y,
			w = 300,
			opacity = 0.9,
			flying,
		})

	return ui.col({
		full = true,
		fill = C.bg,
		-- top bar
		ui.row({
			gap = 10,
			px = 24,
			py = 14,
			align_center = true,
			ui.text({ "Board", color = C.text, font_size = 18 }),
			W.badge(#cards),
			ui.col({ grow = true }),
			ui.button({
				h = 32,
				px = 14,
				radius = 8,
				center = true,
				fill = C.accent,
				hover_fill = C.accent_hi,
				ui.text({ "+  Column", color = "#ffffff", font_size = 13 }),
				on_click = function()
					update({ kind = "open_col" })
				end,
			}),
		}),
		ui.col({ h = 1, w_full = true, fill = C.line_soft }),
		-- board
		ui.row({
			id = "board",
			scroll_x = true,
			grow = true,
			stretch = true,
			px = 13,
			py = 20,
			col_list,
		}),
		ghost or false,
		S.col_modal and modal(),
	})
end
