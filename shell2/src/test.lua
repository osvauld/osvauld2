-- MODEL
local todos = {} -- list of { id, text, done }
local draft = "" -- the text being typed
local next_id = 1
local drag = nil
local placement = nil

local function commit()
	local source = nil
	if not (drag and placement) then
		return
	end
	if drag.id == placement.id then
		return
	end
	for i = 1, #todos do
		if drag and drag.id and todos[i].id == drag.id then
			source = { value = todos[i], idx = i }
			table.remove(todos, i)
			break
		end
	end
	for i = 1, #todos do
		if placement and placement.id and todos[i].id == placement.id and source then
			source.value.done = todos[i].done
			if placement.before then
				table.insert(todos, i, source.value)
			else
				table.insert(todos, i + 1, source.value)
			end
			break
		end
	end
end

local function dump(tag)
	local out = {}
	for i, t in ipairs(todos) do
		out[i] = i .. ":" .. t.text .. (t.done and "*" or "")
	end
	print(tag, table.concat(out, " | "))
end
-- UPDATE
local function update(msg)
	if msg.kind == "edit" then
		draft = msg.text
	elseif msg.kind == "add" or msg.kind == "enter" then
		if draft ~= "" then
			todos[#todos + 1] = { id = next_id, text = draft, done = false }
			next_id = next_id + 1
			draft = ""
		end
	elseif msg.kind == "delete" then
		for i = 1, #todos do
			if todos[i].id == msg.id then
				table.remove(todos, i)
				break
			end
		end
	elseif msg.kind == "toggle" then
		print("TOGGLE", msg.id)
		for i = 1, #todos do
			if todos[i].id == msg.id then
				todos[i].done = not todos[i].done
				break
			end
		end
	elseif msg.kind == "drag" then
		if msg.phase == "start" then
			drag = { id = msg.id }
		elseif msg.phase == "move" then
			placement = nil
			drag.x, drag.y = msg.x, msg.y
		elseif msg.phase == "end" then
			drag, placement = nil, nil
		end
	elseif msg.kind == "drop" then
		local before = msg.y < 0.5
		if msg.phase == "over" then
			placement = { id = msg.id, before = before }
		else
			commit()
		end
	elseif msg.kind == "drop_col" then
		if msg.phase == "over" then
			placement = { col = msg.col }
			return
		end
		local done = msg.col == "done"
		for i = 1, #todos do
			if drag and todos[i].id == drag.id then
				todos[i].done = done
			end
		end
	end
	if msg.phase ~= "move" and msg.phase ~= "over" then
		dump(msg.kind .. " " .. tostring(msg.phase))
	end
end

local function guide()
	return ui.col({ h = 2, stretch = true, fill = "#3b82f6" })
end
local function shallow_copy(t)
	local c = {}
	for k, v in pairs(t) do
		c[k] = v
	end
	return c
end

return function()
	-- header: the input + the add button

	local todo_col = ui.col({
		id = "col:todo",
		w = 300,
		h = 600,
		gap = 8,
		pad = 12,
		fill = "#1a1a1a",
		scroll = "y",
		on_drop = function(phase, x, y)
			update({ kind = "drop_col", col = "todo", x = x, y = y, phase = phase })
		end,
	})
	local done_col = ui.col({
		id = "col:done",
		w = 300,
		h = 600,
		gap = 8,
		pad = 12,
		fill = "#1a1a1a",

		on_drop = function(phase, x, y)
			update({ kind = "drop_col", col = "done", x = x, y = y, phase = phase })
		end,
	})
	local list = ui.col({
		pad = 20,
		gap = 8,
		ui.row({ gap = 16, todo_col, done_col }),
		ui.row({
			gap = 8,
			ui.input({
				value = draft,
				id = "draft",
				w = 220,
				h = 40,
				fill = "#222",
				color = "white",
				on_enter = function()
					update({ kind = "enter" })
				end,
				on_input = function(s)
					update({ kind = "edit", text = s })
				end,
			}),
			ui.button({
				h = 40,
				w = 100,
				pad = 12,
				center = true,
				radius = 24,
				fill = "#3b82f6",
				ui.text({ "add3", color = "white" }),
				on_click = function()
					update({ kind = "add" })
				end,
			}),
		}),
	})

	-- one row per todo, appended AFTER the header
	local dragged = nil
	for i = 1, #todos do
		local t = todos[i]

		local card = ui.row({
			id = "card:" .. t.id,
			gap = 8,
			center = true,

			on_drag = function(phase, x, y)
				update({ kind = "drag", id = t.id, phase = phase, x = x, y = y })
			end,

			on_drop = function(phase, x, y)
				update({ kind = "drop", col = "todo", x = x, y = y, phase = phase, id = t.id })
			end,
			ui.button({
				w = 24,
				h = 24,
				radius = 4,
				fill = t.done and "#3b82f6" or "#444",
				on_click = function()
					update({ kind = "toggle", id = t.id })
				end,
			}),
			ui.text({ t.text, color = "white" }),
			ui.button({
				h = 40,
				pad = 12,
				center = true,
				fill = "#3b82f6",
				ui.text({ "delete", color = "white" }),
				on_click = function()
					update({ kind = "delete", id = t.id })
				end,
			}),
		})

		if drag and drag.id == t.id then
			dragged = shallow_copy(card)
			dragged.id, dragged.on_drop, dragged.on_drag = nil, nil, nil
		end

		local col = t.done and done_col or todo_col
		local at = drag ~= nil and placement ~= nil and placement.id == t.id and drag.id ~= t.id
		col[#col + 1] = {
			at and placement and placement.before and guide(),
			card,
			at and placement and not placement.before and guide(),
		}
	end
	if dragged ~= nil then
		local ghost = ui.col({ absolute = true, left = drag.x, top = drag.y, opacity = 0.9, w = 276, dragged })
		return ui.col({ list, ghost })
	end

	return ui.col({ list })
end
