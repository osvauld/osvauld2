-- MODEL
local todos = {} -- list of { id, text, done }
local draft = "" -- the text being typed
local next_id = 1
local drag = nil

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
		for i = 1, #todos do
			if todos[i].id == msg.id then
				todos[i].done = not todos[i].done
				break
			end
		end
	elseif msg.kind == "drag" then
		drag = { id = msg.id }
	elseif msg.kind == "drop" then
		local target = msg.col == "done"
		if drag then
			for i = 1, #todos do
				if todos[i].id == drag.id then
					todos[i].done = target
				end
			end
			drag = nil
		end
	end
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
		on_drop = function()
			update({ kind = "drop", col = "todo" })
		end,
	})
	local done_col = ui.col({
		id = "col:done",
		w = 300,
		h = 600,
		gap = 8,
		pad = 12,
		fill = "#1a1a1a",

		on_drop = function()
			update({ kind = "drop", col = "done" })
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
	for i = 1, #todos do
		local t = todos[i]

		local card = ui.row({
			id = "card:" .. t.id,
			gap = 8,
			center = true,

			on_drag = function(x, y)
				update({ kind = "drag", id = t.id })
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
		if t.done then
			done_col[#done_col + 1] = card
		else
			todo_col[#todo_col + 1] = card
		end
	end

	return list
end
