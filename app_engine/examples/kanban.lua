-- kanban.lua — a CRDT-backed kanban board, built to stress-test the engine's UI.
--
-- It leans on almost everything the engine has: a deep layout (a row of columns, each holding
-- cards, each card holding a button row), one `doc:list("cards")` of `{ text, col }` maps
-- filtered three ways, three independent `ui.editor` inputs (focus moves between them), and a
-- handler per button. Moving a card across columns is just setting its `col` — no list-to-list
-- move needed. All state is Loro, so a peer or MCP rearranges the board the same way you do.

local COLS = { "To do", "Doing", "Done" }

local S = {
  page   = { direction = "row", gap = 14, padding = 16, background = "#14161a", width = "100%", height = "100%" },
  col    = { width = 240, gap = 10, padding = 12, corner = 10, background = "#1a1d23" },
  head   = { font = 16, color = "#e6e6ea", padding = 2 },
  card   = { gap = 8, padding = 10, corner = 8, background = "#23272f" },
  cardhi = { background = "#2b3039" },
  ctext  = { font = 14, color = "#dfe3ea", width = "100%" },
  bar    = { direction = "row", gap = 6 },
  move   = { font = 13, color = "#cdd3df", padding = 5, corner = 6, background = "#2d3340" },
  movehi = { background = "#3a4150", color = "white" },
  del    = { font = 13, color = "#9aa0ab", padding = 5, corner = 6 },
  delhi  = { background = "#3a2a2c", color = "#f47068" },
  input  = { font = 14, color = "#e6e6ea", padding = 8, corner = 8, background = "#0f1115", width = "100%" },
}

local cards = doc:list("cards")

-- Seed once (survives hot-reload + restart; the doc outlives the script).
if #cards == 0 then
  cards:add{ text = "Render spine (Taffy + egui)", col = 3 }
  cards:add{ text = "Lua + the doc binding",       col = 3 }
  cards:add{ text = "ui.editor + keyboard focus",  col = 2 }
  cards:add{ text = "Drag and drop",               col = 1 }
  cards:add{ text = "Sync over iroh",              col = 1 }
end

-- One draft text per column, so each column's input is independent.
local function add_to(c)
  local d = doc:text("draft" .. c)
  local text = (d:get() or ""):match("^%s*(.-)%s*$")
  if text ~= "" then
    cards:add{ text = text, col = c }
    d:set("")
  end
end

return function()
  local columns = {}
  for c = 1, 3 do
    local kids = { ui.text{ COLS[c], style = S.head } }

    -- Cards whose `col` is this column.
    for i = 1, #cards do
      local card = cards:get(i)
      if (card.col or 1) == c then
        local bar = {}
        if c > 1 then
          bar[#bar + 1] = ui.button{ "<", style = S.move, hover = S.movehi,
            on_click = function() card.col = c - 1 end }
        end
        if c < 3 then
          bar[#bar + 1] = ui.button{ ">", style = S.move, hover = S.movehi,
            on_click = function() card.col = c + 1 end }
        end
        bar[#bar + 1] = ui.button{ "x", style = S.del, hover = S.delhi,
          on_click = function() cards:remove(i) end }

        kids[#kids + 1] = ui.col{ style = S.card, hover = S.cardhi,
          ui.text{ card.text or "", style = S.ctext },
          ui.row{ style = S.bar, table.unpack(bar) },
        }
      end
    end

    -- This column's add-a-card input.
    kids[#kids + 1] = ui.editor{ id = "draft" .. c, style = S.input,
      on_submit = function() add_to(c) end }

    columns[#columns + 1] = ui.col{ style = S.col, table.unpack(kids) }
  end

  return ui.row{ style = S.page, table.unpack(columns) }
end
