-- team-board: complex-scenario test.
-- Exercises: nested flex, space-between, align_self, max_width centering, absolute overlay
-- (count badges), hover/active states, CRDT list + editor + scroll, min_height.
local T = require("lib.theme")

local tasks = doc:list("tasks") -- rows: { text = string, done = boolean }
local draft = doc:text("draft")

local function add_task()
  local text = draft:get()
  if text ~= "" then
    tasks:add({ text = text, done = false })
    draft:set("")
  end
end

local function pill(label, color)
  return ui.col{
    style = { background = color, padding = "3 10", border_radius = 999 },
    ui.text{ label, style = { font_size = 11, color = "white" } },
  }
end

local function task_row(t)
  local done = t.done
  return ui.row{
    style = {
      background = T.card, padding = "10 14", gap = 10, border_radius = 10,
      border = "1 " .. T.line, align_items = "center",
      justify_content = "space-between",
    },
    hover  = { background = "#232936", border = "1 #3a4760" },
    active = { background = "#2a3142" },
    on_click = function() t.done = not done end,
    ui.text{ done and "✓ " or "○ ",
      { t.text or "", strike = done },
      style = { font_size = 14, color = done and T.muted or T.fg } },
    pill(done and "done" or "open", done and T.good or T.warn),
  }
end

local function column(title, want_done)
  local items, n = {}, 0
  for i = 1, tasks:len() do
    local t = tasks:get(i)
    if t and t.done == want_done then
      n = n + 1
      items[#items + 1] = task_row(t)
    end
  end
  return ui.col{
    style = {
      background = T.panel, border_radius = 14, border = "1 " .. T.line,
      padding = 14, gap = 10, flex_grow = 1, min_height = 200,
    },
    -- column header with an absolute count badge in the corner
    ui.col{
      ui.text{ title, style = { font_size = 15, color = T.fg } },
      ui.col{
        style = { position = "absolute", top = 0, right = 0,
                  background = T.accent, padding = "2 8", border_radius = 999,
                  opacity = n == 0 and 0.35 or 1.0 },
        ui.text{ tostring(n), style = { font_size = 11, color = "white" } },
      },
    },
    ui.col{ style = { gap = 8, flex_grow = 1 }, scroll = title,
      table.unpack(#items > 0 and items
        or { ui.text{ "nothing here", style = { font_size = 13, color = T.muted } } }) },
  }
end

return function()
  return ui.col{
    style = { background = T.bg, width = "100%", height = "100%",
              padding = 20, align_items = "center" },
    -- centered page column, capped like a web layout
    ui.col{ style = { max_width = 760, width = "100%", gap = 14, flex_grow = 1 },

      -- header band: title left, legend right (space-between)
      ui.row{ style = { justify_content = "space-between", align_items = "center" },
        ui.text{ "Team board", style = { font_size = 24, color = T.fg } },
        ui.row{ style = { gap = 8, align_items = "center" },
          pill("open", T.warn), pill("done", T.good) },
      },

      -- composer: editor grows, button hugs
      ui.row{ style = { gap = 10, align_items = "center" },
        ui.editor{ value = draft, id = "draft", on_submit = add_task,
          style = { background = T.panel, padding = "10 14", border_radius = 10,
                    border = "1 " .. T.line, flex_grow = 1, font_size = 14, color = T.fg } },
        ui.button{ "Add",
          style = { background = T.accent, padding = "10 18", border_radius = 10,
                    font_size = 14, color = "white" },
          hover  = { background = "#629af7" },
          active = { background = "#3b78e0" },
          on_click = add_task },
      },

      -- two live columns
      ui.row{ style = { gap = 14, flex_grow = 1, align_items = "stretch" },
        column("Open", false),
        column("Done", true),
      },
    },
  }
end
