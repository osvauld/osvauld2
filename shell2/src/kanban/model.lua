-- MODEL
--
-- Columns and cards live in the CRDT, not in Lua tables: `board.columns` and `board.cards` are
-- the mirror, refreshed from Loro before every frame, and every mutation goes back through the
-- doc. That is what makes the board survive a restart, and what will make a second peer's drag
-- show up here.
--
-- One rule to keep in mind while reading the actions below: **the mirror is a frame behind your
-- own write.** `:set` and friends go straight to Loro, and `board.cards` is only repatched in
-- view(). So read what you need first, then write; never read back what you just wrote.
--
-- This module runs once per app instance -- `require` caches it -- so the seeding below happens
-- once no matter how many files ask for the model.
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

-- Per-viewer pointer state -- what *this* pointer is doing right now. Deliberately not in the
-- doc: broadcasting a half-finished drag would show everyone else your mouse.
--
-- It has to be a table field rather than a local now that the view lives in another file. A
-- `local drag` is file-scoped, and reassigning it here could never be seen from main.lua; a
-- field on an exported table can. That is the whole cost of the split, and it is why there is
-- exactly one of these tables rather than one per module.
local S = { drag = nil, placement = nil, col_modal = false }

local actions = {}

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
	local from = index_of(cards, S.drag.id)
	if not from then
		return
	end
	-- Dropped on a column rather than on a card: leave the ordering alone and just restamp the
	-- column. The card list is flat, so `col` is the only thing that has to change.
	if S.placement.kind == "into" then
		board:set({ "cards", S.drag.id, "col" }, S.placement.col)
		return
	end
	if S.drag.id == S.placement.id then
		return
	end
	local at = index_of(cards, S.placement.id)
	local anchor = at and cards[at]
	-- Move first, then restamp: :move retargets the element's `pos` register and :set writes a
	-- field inside it, and those are independent registers. Deleting and reinserting would mint
	-- a new element, which is how a concurrent edit turns into a duplicated card.
	board:move({ "cards", S.drag.id }, target_index(cards, from, S.placement.id, S.placement.before))
	if anchor and anchor.col ~= cards[from].col then
		board:set({ "cards", S.drag.id, "col" }, anchor.col)
	end
end

local function commit_col()
	if S.drag.id == S.placement.id then
		return
	end
	local columns = board.columns
	local from = index_of(columns, S.drag.id)
	if not from then
		return
	end
	board:move(
		{ "columns", S.drag.id },
		target_index(columns, from, S.placement.id, S.placement.before)
	)
end

local function commit()
	if not (S.drag and S.placement) then
		return
	end
	if S.drag.kind == "col" then
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
		S.drag = { kind = msg.what, id = msg.id, x = msg.x, y = msg.y }
	elseif msg.phase == "move" then
		if S.drag then
			S.drag.x, S.drag.y = msg.x, msg.y
		end
		S.placement = nil
	elseif msg.phase == "end" then
		S.drag, S.placement = nil, nil
	end
end

function actions.drop(msg)
	if not (S.drag and S.drag.kind == "card") then
		return
	end
	if msg.phase == "over" then
		S.placement = { kind = "card", id = msg.id, before = msg.y < 0.5 }
	else
		commit()
	end
end

-- One target, two meanings: a card lands *inside* the column, a column lands *beside* it.
function actions.drop_col(msg)
	if not S.drag then
		return
	end
	if msg.phase == "over" then
		if S.drag.kind == "col" then
			S.placement = { kind = "col", id = msg.col, before = msg.x < 0.5 }
		else
			S.placement = { kind = "into", col = msg.col }
		end
	else
		commit()
	end
end

function actions.open_col()
	S.col_modal = true
end

function actions.close_col()
	S.col_modal = false
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
	S.col_modal = false
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

-- Cards keyed by the column they sit in. A derivation over the mirror, so it is recomputed every
-- frame rather than cached: the tables it reads are repatched under it.
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

return {
	board = board,
	state = S,
	update = update,
	by_column = by_column,
}
