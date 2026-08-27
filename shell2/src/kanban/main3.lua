local columns = {}
local cards = {}
local drag, placement = nil, nil
local drafts = {}
local actions = {}
local col_modal = false
local col_name_draft = nil

local function commit()
	local source = nil
	if not (drag and placement) then
		return
	end
	if drag.id == placement.id then
		return
	end
	for i = 1, #cards do
		if drag.id and cards[i].id == drag.id then
			source = table.remove(cards, i)
			break
		end
	end
	if placement and placement.col and source then
		source.col = placement.col
		cards[#cards + 1] = source
		return
	end

	for i = 1, #cards do
		if placement and placement.id and cards[i].id == placement.id and source then
			source.col = cards[i].col
			if placement.before then
				table.insert(cards, i, source)
			else
				table.insert(cards, i + 1, source)
			end
			break
		end
	end
end

function actions.edit(msg)
	drafts[msg.col] = msg.text
end

function actions.edit_col_name(msg)
	col_name_draft = msg.text
end

function actions.add(msg)
	local s = ui.state("col" .. msg.col)
	if s.draft and s.draft ~= "" then
		local card = { id = uuid(), col = msg.col, text = s.draft }
		cards[#cards + 1] = card
		s.draft = ""
	end
end

function actions.enter(msg)
	actions.add(msg)
end

function actions.drag(msg)
	if msg.phase == "start" then
		drag = { id = msg.id, x = msg.x, y = msg.y }
	elseif msg.phase == "move" then
		placement = nil
		drag.x, drag.y = msg.x, msg.y
	elseif msg.phase == "end" then
		drag, placement = nil, nil
	end
end

function actions.drop_col(msg)
	if msg.phase == "over" then
		placement = { col = msg.col }
	elseif msg.phase == "release" then
		commit()
	end
end

function actions.drop(msg)
	local before = msg.y < 0.5
	if msg.phase == "over" then
		placement = { id = msg.id, before = before }
		return
	else
		commit()
	end
end

function actions.open_col()
	col_modal = true
end

function actions.add_col(msg)
	if col_name_draft == "" or col_name_draft == nil then
		return
	end
	columns[#columns + 1] = { id = uuid(), name = col_name_draft }
	col_name_draft = nil
	col_modal = false
end

function actions.close_col()
	col_modal = false
	col_name_draft = nil
end

function actions.delete_col(msg)
	local delete = true
	local col_idx = nil

	for i = 1, #columns do
		if columns[i].id == msg.id then
			col_idx = i
		end
	end
	if col_idx == nil then
		return
	end

	for i = 1, #cards do
		if cards[i].col == columns[col_idx].id then
			delete = false
			break
		end
	end
	if delete then
		table.remove(columns, col_idx)
	end
end

function actions.delete(msg)
	for i = 1, #cards do
		if cards[i].id == msg.id then
			table.remove(cards, i)
			break
		end
	end
end

local function update(msg)
	local f = actions[msg.kind]
	if not f then
		print("unknown action:", msg.kind)
		return
	end
	f(msg)
end
local function by_column()
	local out = {}
	for _, c in ipairs(columns) do
		out[c.id] = {}
	end
	for _, card in ipairs(cards) do
		local list = out[card.col]
		if list then
			list[#list + 1] = card
		end
	end
	return out
end

local function cardof(c)
	return ui.row({
		id = c.id,
		gap = 8,
		center = true,
		on_drag = function(phase, x, y)
			update({ kind = "drag", id = c.id, phase = phase, x = x, y = y })
		end,
		on_drop = function(phase, x, y)
			update({ kind = "drop", phase = phase, x = x, y = y, id = c.id })
		end,
		ui.text({ c.text, color = "white" }),
	})
end

local function column_of(c, list)
	local col_list = {}
	for i = 1, #list do
		col_list[#col_list + 1] = cardof(list[i])
	end
	local s = ui.state("col" .. c.id, { draft = "" })
	return ui.col({
		id = c.id,
		gap = 8,
		w = 300,
		h = 600,
		fill = "#1a1a1a",
		ui.row({
			w_full = true,
			center = true,
			ui.text({ c.name, color = "white", font_size = 18, mb = 4 }),
		}),
		col_list,
		ui.col({
			center = true,
			ui.input({
				value = s.draft,
				id = "draft" .. c.id,
				w = 220,
				h = 40,
				fill = "#222",
				color = "white",
				on_enter = function()
					update({ kind = "enter", col = c.id })
				end,
				on_input = function(v)
					s.draft = v
				end,
			}),
			ui.button({
				h = 40,
				w = 100,
				pad = 12,
				center = true,
				radius = 24,
				fill = "#3b82f6",
				ui.text({ "add", color = "white" }),
				on_click = function()
					update({ kind = "add", col = c.id })
				end,
			}),
		}),

		on_drop = function(phase, x, y)
			update({ kind = "drop_col", col = c.id, phase = phase })
		end,
	})
end

return function()
	local col_list = {}
	local out_list = by_column()

	for i = 1, #columns do
		col_list[#col_list + 1] = column_of(columns[i], out_list[columns[i].id])
	end

	local panel = ui.col({
		w = 320,
		pad = 20,
		gap = 12,
		radius = 12,
		fill = "#1a1a1a",
		on_click = function() end,
		ui.text({ "New Column", color = "white" }),
		ui.input({
			value = col_name_draft or "",
			id = "col_draft",
			autofocus = true,
			w_full = true,
			h = 40,
			fill = "#222",
			color = "white",
			on_input = function(s)
				update({ kind = "edit_col_name", text = s })
			end,
			on_enter = function()
				update({ kind = "add_col" })
			end,
			on_esc = function()
				update({ kind = "close_col" })
			end,
		}),
		ui.button({
			h = 40,
			pad = 12,
			center = true,
			radius = 8,
			fill = "#3b8276",
			ui.text({ "create", color = "white" }),
			on_click = function()
				update({ kind = "add_col" })
			end,
		}),
	})

	local backdrop = ui.col({
		absolute = true,
		center = true,
		top = 0,
		left = 0,
		right = 0,
		bottom = 0,
		fill = "rgba(0,0,0,0.5)",
		on_click = function()
			update({ kind = "close_col" })
		end,
		panel,
	})

	return ui.col({
		full = true,
		ui.row({ scroll_x = true, pad = 24, gap = 24, id = "board", grow = true, col_list }),
		ui.button({
			absolute = true,
			right = 24,
			bottom = 24,
			h = 40,
			w = 100,
			pad = 12,
			center = true,
			radius = 24,
			fill = "#3b82f6",
			ui.text({ "add col", color = "white" }),
			on_click = function()
				update({ kind = "open_col" })
			end,
		}),
		col_modal and backdrop,
	})
end
