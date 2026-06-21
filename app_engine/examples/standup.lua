-- standup.lua — editable cells: the table as a shared form.
--
-- Text cells are editors: click one, type, and the keystroke lands in that row's CRDT field
-- (addressed by stable row id, so it stays correct under sort/filter — and a peer or an agent
-- over MCP sees it live). `on_edit(row, key)` fires when a cell commits (Enter, or clicking
-- away after typing): here it appends an audit row to a second list, rendered as its own
-- read-only table below. `locked = true` keeps the name column display-only.
--
-- Column types: `select` is a combobox (click → dropdown, type → filter, Enter → first match);
-- `transitions` makes it a state machine — from each value, only the listed successors are
-- offered. `date` cells normalize loose input to YYYY-MM-DD on commit; `number` cells parse.

local S = {
  page  = { padding = 26, gap = 14, background = "#14161a", width = "100%", min_height = "100%" },
  h1    = { font = 22, color = "#e6e6ea" },
  h2    = { font = 15, color = "#9aa0ab" },
  hint  = { font = 13, color = "#9aa0ab", width = 600 },
  table = { color = "#dde1e8", font = 14 },
  log   = { color = "#9aa0ab", font = 13, height = 150 },
}

local team = doc:list("team")
local log = doc:list("log")
local day = doc:map("day")

if #team == 0 then
  team:add{ name = "Asha",  today = "wire the cell editor",   status = "doing", due = "2026-06-15", done = false }
  team:add{ name = "Dev",   today = "permit revocation spec", status = "open",  due = "2026-06-20", done = false }
  team:add{ name = "Meera", today = "PDF header parity",      status = "done",  due = "2026-06-12", done = true }
  team:add{ name = "Tom",   today = "",                       status = "open",  due = "",           done = false }
end
if not day.started then day.started = now() end

return function()
  local open = 0
  for i = 1, #team do
    if not team:get(i).done then open = open + 1 end
  end

  return ui.col{ style = S.page,
    ui.text{ "Standup", style = S.h1 },
    ui.text{ ("Click a cell and type — your row is yours, the grid is everyone's. %d of %d still open."):format(open, #team),
             style = S.hint },
    ui.table{
      rows = team,
      style = S.table,
      row_height = 36, -- fixed row height; columns size via per-column `width`
      columns = {
        { key = "name",   label = "Who",    width = 100, locked = true },
        { key = "today",  label = "Today" },
        { key = "status", label = "Status", type = "select", width = 120,
          options = { "open", "doing", "review", "done" },
          -- a state machine: each value lists where it may go next
          transitions = {
            open   = { "doing" },
            doing  = { "review", "open" },
            review = { "done", "doing" },
            done   = {},
          } },
        { key = "due",  label = "Due",  type = "date", width = 110 },
        { key = "done", label = "Done", type = "check", width = 60 },
      },
      on_edit = function(row, key)
        -- The commit hook: every cell commit leaves an audit row (who/what/when), data a
        -- reviewer — or an agent — reads back from the same doc.
        log:add{ at = math.floor(now() - (day.started or now())), row = row, field = key }
      end,
    },
    ui.text{ "Edit log", style = S.h2 },
    ui.table{
      rows = log,
      order_by = { "at", desc = true },
      style = S.log,
      columns = {
        { key = "at",    label = "T+s",   type = "number", width = 80, locked = true },
        { key = "row",   label = "Row",   width = 200, locked = true },
        { key = "field", label = "Field", locked = true },
      },
    },
  }
end
