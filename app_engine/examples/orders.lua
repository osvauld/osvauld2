-- orders.lua — the table primitive: a typed grid over a CRDT list.
--
-- `ui.table` reads `doc:list("orders")` (a MovableList of row maps), so a peer or an agent over
-- MCP edits the same rows this UI shows. The query is declarative and runs in the engine:
-- `where` filters by equality, `order_by` sorts — display order only, the list never reorders.
-- The status pills swap the `where` by writing a CRDT map, so the filter itself syncs too.

local S = {
  page  = { padding = 26, gap = 14, background = "#14161a", width = "100%", min_height = "100%" },
  h1    = { font = 22, color = "#e6e6ea" },
  hint  = { font = 13, color = "#9aa0ab", width = 560 },
  bar   = { direction = "row", gap = 6 },
  pill  = { font = 13, color = "#9aa0ab", padding = "5 12", corner = 99, background = "#1a1d23" },
  pillon = { color = "white", background = "#2f6f4f" },
  pillhi = { background = "#242a33" },
  table = { color = "#dde1e8", font = 14 },
  add   = { font = 14, color = "white", padding = "8 12", corner = 8, background = "#2f6f4f" },
  addhi = { background = "#3a8a62" },
}

local orders = doc:list("orders")
local view = doc:map("view")

if #orders == 0 then
  orders:add{ ref = "OSV-1041", item = "Walnut desk",    qty = 2,  price = 1240, status = "packing",   paid = true }
  orders:add{ ref = "OSV-1042", item = "Oak shelf",      qty = 6,  price = 380,  status = "shipped",   paid = true }
  orders:add{ ref = "OSV-1043", item = "Pine bench",     qty = 1,  price = 95,   status = "packing",   paid = false }
  orders:add{ ref = "OSV-1044", item = "Teak side table",qty = 3,  price = 510,  status = "delivered", paid = true }
  orders:add{ ref = "OSV-1045", item = "Birch stool",    qty = 12, price = 45,   status = "shipped",   paid = false }
  orders:add{ ref = "OSV-1046", item = "Ash headboard",  qty = 1,  price = 720,  status = "packing",   paid = false }
end

local STATUSES = { "all", "packing", "shipped", "delivered" }

return function()
  local active = view.status or "all"
  local where = nil
  if active ~= "all" then where = { status = active } end

  local pills = { style = S.bar }
  for _, st in ipairs(STATUSES) do
    local style = st == active and { font = S.pill.font, padding = S.pill.padding, corner = S.pill.corner,
                                     color = S.pillon.color, background = S.pillon.background }
                                or S.pill
    pills[#pills + 1] = ui.button{ st, style = style, hover = S.pillhi,
      on_click = function() view.status = st end }
  end

  return ui.col{ style = S.page,
    ui.text{ "Orders", style = S.h1 },
    ui.text{ "A typed grid over doc:list — the pills set a declarative `where`, `order_by` sorts by price. Click an item name to edit it in place (rows are addressed by id, so editing under sort hits the right row); Ref is locked.", style = S.hint },
    ui.row(pills),
    ui.table{
      rows = orders,
      where = where,
      order_by = { "price", desc = true },
      style = S.table,
      columns = {
        { key = "ref",    label = "Ref",    width = 110, locked = true },
        { key = "item",   label = "Item" },
        { key = "qty",    label = "Qty",    type = "number", width = 70 },
        { key = "price",  label = "Price",  type = "number", width = 90 },
        { key = "status", label = "Status", type = "select", width = 120 },
        { key = "paid",   label = "Paid",   type = "check",  width = 70 },
      },
    },
    ui.button{ "+ add order", style = S.add, hover = S.addhi,
      on_click = function()
        orders:add{ ref = "OSV-" .. (1046 + #orders - 5), item = "Cedar crate", qty = 4,
                    price = math.random(40, 900), status = "packing", paid = false }
      end },
  }
end
