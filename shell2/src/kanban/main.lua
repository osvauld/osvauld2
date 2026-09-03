-- PALETTE
local C = {
	bg = "#0d1117",
	panel = "#161b22",
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

-- MODEL
--
-- Columns and cards live in the CRDT, not in Lua tables: `board.columns` and `board.cards` are
-- the mirror, refreshed from Loro before every frame, and every mutation goes back through the
-- doc. That is what makes the board survive a restart, and what will make a second peer's drag
-- show up here.
--
-- `drag`, `placement` and `col_modal` deliberately stay Lua locals. They are per-viewer UI
-- state -- what *this* pointer is doing right now -- and putting them in the doc would
-- broadcast one person's half-finished drag to everyone.
--
-- One rule to keep in mind while reading the actions below: **the mirror is a frame behind your
-- own write.** `:set` and friends go straight to Loro, and `board.cards` is only repatched in
-- view(). So read what you need first, then write; never read back what you just wrote.
local board = doc:open("board")

-- Module scope runs on every open, so seeding has to be guarded or reopening the app would
-- wipe the board it just loaded.
if not board.columns then
	board:set({ "columns" }, doc.list({
		doc.map({ id = "c-todo", name = "Todo" }),
		doc.map({ id = "c-doing", name = "In Progress" }),
		doc.map({ id = "c-done", name = "Done" }),
	}))
end
if not board.cards then
	board:set({ "cards" }, doc.list({
		doc.map({ id = "k1", col = "c-todo", text = "Wire the MCP bridge" }),
		doc.map({ id = "k2", col = "c-todo", text = "Font weight in TextSpec" }),
		doc.map({ id = "k3", col = "c-doing", text = "Error boundaries in walk" }),
		doc.map({ id = "k4", col = "c-done", text = "ui.state + sweep" }),
	}))
end

local drag, placement = nil, nil
local col_modal = false
local actions = {}

-- UPDATE
local function index_of(list, id)
	for i = 1, #list do
		if list[i].id == id then
			return i
		end
	end
end

-- Loro's move removes then reinserts, so `to` is the index the element ends up at *after* its
-- own removal. Dragging downwards therefore lands one slot short unless the target is nudged.
local function target_index(list, from, anchor_id, before)
	local at = index_of(list, anchor_id)
	if not at then
		return #list
	end
	local to = before and at or at + 1
	if from < to then
		to = to - 1
	end
	return to
end

local function commit_card()
	local cards = board.cards
	local from = index_of(cards, drag.id)
	if not from then
		return
	end
	-- Dropped on a column rather than on a card: leave the ordering alone and just restamp the
	-- column. The card list is flat, so `col` is the only thing that has to change.
	if placement.kind == "into" then
		board:set({ "cards", drag.id, "col" }, placement.col)
		return
	end
	if drag.id == placement.id then
		return
	end
	local at = index_of(cards, placement.id)
	local anchor = at and cards[at]
	-- Move first, then restamp: :move retargets the element's `pos` register and :set writes a
	-- field inside it, and those are independent registers. Deleting and reinserting would mint
	-- a new element, which is how a concurrent edit turns into a duplicated card.
	board:move({ "cards", drag.id }, target_index(cards, from, placement.id, placement.before))
	if anchor and anchor.col ~= cards[from].col then
		board:set({ "cards", drag.id, "col" }, anchor.col)
	end
end

local function commit_col()
	if drag.id == placement.id then
		return
	end
	local columns = board.columns
	local from = index_of(columns, drag.id)
	if not from then
		return
	end
	board:move({ "columns", drag.id }, target_index(columns, from, placement.id, placement.before))
end

local function commit()
	if not (drag and placement) then
		return
	end
	if drag.kind == "col" then
		commit_col()
	else
		commit_card()
	end
end

function actions.add(msg)
	local s = ui.state("draft:" .. msg.col)
	if s.text and s.text ~= "" then
		board:insert({ "cards" }, doc.map({ id = uuid(), col = msg.col, text = s.text }))
		s.text = ""
	end
end

function actions.delete(msg)
	board:delete({ "cards", msg.id })
end

function actions.drag(msg)
	if msg.phase == "start" then
		drag = { kind = msg.what, id = msg.id, x = msg.x, y = msg.y }
	elseif msg.phase == "move" then
		if drag then
			drag.x, drag.y = msg.x, msg.y
		end
		placement = nil
	elseif msg.phase == "end" then
		drag, placement = nil, nil
	end
end

function actions.drop(msg)
	if not (drag and drag.kind == "card") then
		return
	end
	if msg.phase == "over" then
		placement = { kind = "card", id = msg.id, before = msg.y < 0.5 }
	else
		commit()
	end
end

-- One target, two meanings: a card lands *inside* the column, a column lands *beside* it.
function actions.drop_col(msg)
	if not drag then
		return
	end
	if msg.phase == "over" then
		if drag.kind == "col" then
			placement = { kind = "col", id = msg.col, before = msg.x < 0.5 }
		else
			placement = { kind = "into", col = msg.col }
		end
	else
		commit()
	end
end

function actions.open_col()
	col_modal = true
end

function actions.close_col()
	col_modal = false
	-- Clear the draft too, or the next open shows the abandoned text.
	ui.state("col_modal", { name = "" }).name = ""
end

function actions.add_col()
	local m = ui.state("col_modal")
	if not m.name or m.name == "" then
		return
	end
	board:insert({ "columns" }, doc.map({ id = uuid(), name = m.name }))
	m.name = ""
	col_modal = false
end

function actions.delete_col(msg)
	-- Collect first, delete second. Each :delete lands in the doc immediately but the mirror is
	-- only repatched in view(), so iterating `board.cards` while deleting from it would be
	-- walking a stale list.
	local doomed = {}
	for _, c in ipairs(board.cards) do
		if c.col == msg.id then
			doomed[#doomed + 1] = c.id
		end
	end
	for _, id in ipairs(doomed) do
		board:delete({ "cards", id })
	end
	board:delete({ "columns", msg.id })
end

local function update(msg)
	local f = actions[msg.kind]
	if not f then
		print("unknown action:", msg.kind)
		return
	end
	f(msg)
end

-- HELPERS
local function by_column()
	local out = {}
	for _, c in ipairs(board.columns) do
		out[c.id] = {}
	end
	for _, card in ipairs(board.cards) do
		local list = out[card.col]
		if list then
			list[#list + 1] = card
		end
	end
	return out
end

-- Guides are rendered on *every* gap, always, so the space is permanently reserved and nothing
-- shifts when a drop target appears — only the line's alpha changes. `fade` is keyed by id:
-- that's where the runtime parks the tween, so retargeting mid-flight reverses smoothly.
local function guide(id, on)
	return ui.col({
		py = 3,
		w_full = true,
		ui.col({ id = id, h = 2, radius = 1, fill = C.accent, fade = { on and 1 or 0, 140 } }),
	})
end

-- Vertical twin, for column reordering. No height: the board row is `stretch`, so it fills.
local function guide_v(id, on)
	return ui.col({
		px = 7,
		ui.col({
			id = id,
			w = 2,
			grow = true,
			radius = 1,
			fill = C.accent,
			fade = { on and 1 or 0, 140 },
		}),
	})
end

-- Which gap the indicator belongs in: 1 = above the first card, #list+1 = below the last.
-- A column-background drop ("into") appends, which is the trailing gap — same answer, one path.
local function drop_slot(c, list)
	if not (drag and placement) then
		return nil
	end
	if placement.kind == "into" then
		return placement.col == c.id and #list + 1 or nil
	end
	if placement.kind ~= "card" or drag.id == placement.id then
		return nil
	end
	for i = 1, #list do
		if list[i].id == placement.id then
			return placement.before and i or i + 1
		end
	end
end

local function badge(n)
	return ui.row({
		px = 7,
		py = 1,
		radius = 10,
		fill = C.line_soft,
		center = true,
		ui.text({ tostring(n), color = C.muted, font_size = 11 }),
	})
end

local function icon_button(glyph, on_press, hover)
	return ui.button({
		w = 22,
		h = 22,
		radius = 6,
		center = true,
		hover_fill = hover or C.line,
		ui.text({ glyph, color = C.muted, font_size = 13 }),
		on_click = on_press,
	})
end

-- VIEW PIECES
-- `ghost` builds the floating copy: same look, but no id and no handlers, because a duplicate
-- id would register a second drag target and collide in the state store.
local function card_of(c, ghost)
	local dragging = drag ~= nil and drag.kind == "card" and drag.id == c.id
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
	t[#t + 1] = icon_button("x", function()
		update({ kind = "delete", id = c.id })
	end, C.danger)
	return t
end

local function empty_slot(on)
	return ui.col({
		h = 64,
		center = true,
		radius = 8,
		stroke = { 1, on and C.accent or C.line_soft },
		ui.text({ "drop a card here", color = on and C.accent or C.muted, font_size = 12 }),
	})
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
		body[#body + 1] = guide("g:" .. c.id .. ":" .. i, slot == i)
		body[#body + 1] = card_of(list[i])
	end
	body[#body + 1] = guide("g:" .. c.id .. ":tail", slot == #list + 1)
	if #list == 0 then
		body[#body + 1] = empty_slot(slot ~= nil)
	end

	local lifting = drag ~= nil and drag.kind == "col" and drag.id == c.id
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
			badge(#list),
			ui.col({ grow = true }),
			icon_button("x", function()
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
	local grouped = by_column()
	local col_list = {}
	local flying = nil

	local col_slot = nil
	if drag and placement and placement.kind == "col" and drag.id ~= placement.id then
		for i = 1, #columns do
			if columns[i].id == placement.id then
				col_slot = placement.before and i or i + 1
				break
			end
		end
	end

	for i = 1, #columns do
		local c = columns[i]
		col_list[#col_list + 1] = guide_v("gv:" .. i, col_slot == i)
		col_list[#col_list + 1] = column_of(c, grouped[c.id])
		if drag and drag.kind == "col" and drag.id == c.id then
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
	col_list[#col_list + 1] = guide_v("gv:tail", col_slot == #columns + 1)

	if drag and drag.kind == "card" then
		for i = 1, #cards do
			if cards[i].id == drag.id then
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
			left = drag.x,
			top = drag.y,
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
			badge(#cards),
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
		col_modal and modal(),
	})
end
