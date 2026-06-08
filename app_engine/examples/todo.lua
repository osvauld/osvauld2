-- todo.lua — a CRDT-backed todo list, loaded at runtime (hot-reloads on edit).
--
-- State lives in Loro: `doc:list("todos")` is a MovableList of `{text, done}` maps, and the
-- new-task input is a `doc:text("draft")` (a LoroText, edited by ui.editor). Because it's all a
-- CRDT, a peer — or an agent over MCP — can flip a task's `done`, or type into the draft,
-- exactly the way this UI does; the next view() just reflects it. The data outlives code edits.

local S = {
  page  = { padding = 26, gap = 14, background = "#14161a", width = "100%", height = "100%" },
  h1    = { font = 22, color = "#e6e6ea" },
  hint  = { font = 13, color = "#9aa0ab", width = 540 },
  row   = { direction = "row", gap = 10, padding = 8, corner = 8, background = "#1a1d23" },
  rowhi = { background = "#1f242d" },                                   -- row :hover
  box   = { font = 16, color = "#cdd3df", padding = 6, corner = 6, background = "#2d3340" },
  boxhi = { background = "#3a4150", color = "white" },                 -- checkbox :hover
  del   = { font = 13, color = "#9aa0ab", padding = 6, corner = 6 },
  delhi = { background = "#3a2a2c", color = "#f47068" },               -- delete :hover
  list  = { grow = 1, gap = 8 },   -- the scrolling area: absorbs the height between hint and input
  addrow = { direction = "row", gap = 10 },
  input = { grow = 1, font = 15, color = "#e6e6ea", padding = 9, corner = 8, background = "#0f1115" },
  add   = { font = 15, color = "white", padding = 9, corner = 8, background = "#2f6f4f" },
  addhi = { background = "#3a8a62" },                                  -- add :hover
}

local todos = doc:list("todos")
local draft = doc:text("draft")   -- the new-task input, a CRDT text field

-- Seed once, only when empty (survives hot-reload: the doc outlives the script). Enough rows to
-- overflow the list so the scroll is visible — wheel over the list to scroll it.
if #todos == 0 then
  todos:add{ text = "Wire the doc binding", done = true }
  todos:add{ text = "Click a box to toggle me", done = false }
  for i = 1, 12 do todos:add{ text = "Task number " .. i, done = false } end
  todos:add{ text = "Type below and press Enter (or click add)", done = false }
end

-- Add the draft as a task, if it isn't just whitespace, then clear the field. Shared by the
-- Add button (on_click) and the input itself (on_submit / Enter).
local function add_draft()
  local text = (draft:get() or ""):match("^%s*(.-)%s*$")  -- trim
  if text ~= "" then
    todos:add{ text = text, done = false }
    draft:set("")
  end
end

return function()
  -- The todo rows, gathered into a scrollable list.
  local rows = {}
  for i = 1, #todos do
    local todo = todos:get(i)
    local checked = todo.done
    rows[#rows + 1] = ui.row{ style = S.row, hover = S.rowhi,
      ui.button{ checked and "[x]" or "[ ]", style = S.box, hover = S.boxhi,
        on_click = function() todo.done = not todo.done end },
      ui.text{ todo.text or "",
        style = { font = 15, grow = 1, color = checked and "#6b7280" or "#e6e6ea" } },
      ui.button{ "delete", style = S.del, hover = S.delhi,
        on_click = function() todos:remove(i) end },
    }
  end

  return ui.col{ style = S.page,
    ui.text{ "Todos", style = S.h1 },
    ui.text{ "State lives in the CRDT — a peer or MCP can flip 'done', or type a task, the same way you do.", style = S.hint },
    -- The list scrolls (wheel over it); the header above and input below stay put.
    ui.col{ style = S.list, scroll = true, table.unpack(rows) },
    -- New-task input: type, press Enter or click add. Focus stays on the input after adding
    -- (clicking the button dispatches without blurring), so you can keep typing.
    ui.row{ style = S.addrow,
      ui.editor{ id = "draft", style = S.input, on_submit = add_draft },
      ui.button{ "add", style = S.add, hover = S.addhi, on_click = add_draft },
    },
  }
end
