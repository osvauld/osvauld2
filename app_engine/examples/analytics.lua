-- analytics.lua — a World-B dashboard over IMPORTED tables.
--
-- Unlike dashboard.lua (static boxes) and orders.lua (a grid over this app's OWN doc:list), this
-- app reads other `.table` ITEMS in the workspace through the `data` binding. The heavy lifting
-- (Polars SQL, joins, pivots) runs HOST-SIDE; only results (a window of rows) and tiny KPI scalars
-- cross into Lua. Lua just declares the queries and lays out the view.
--
-- Prerequisites: a `.table` item named "orders" in this workspace (e.g. created by an Excel import
-- or the table primitive), with columns like: segment (text), cust (text), sales (number),
-- status (text/select). An optional "customers" `.table` (cust, region) powers the JOIN panel —
-- absent, that panel just shows nothing. Use the MCP `table_sql` tool to profile the real columns
-- before adapting this file.

local S = {
  page  = { padding = 24, gap = 16, background = "#0d1016", width = "100%", min_height = "100%" },
  h1    = { font = 22, color = "#e8eaf0" },
  hint  = { font = 12.5, color = "#8a91a0", width = 680 },
  kpis  = { direction = "row", gap = 12 },
  card  = { padding = 16, gap = 6, corner = 10, background = "#151a23", width = 200 },
  klabel= { font = 12, color = "#8a91a0" },
  kval  = { font = 26, color = "#e8eaf0" },
  panel = { padding = 14, gap = 10, corner = 10, background = "#151a23", width = "100%" },
  h2    = { font = 14, color = "#cdd2dc" },
  chart = { height = 240, width = "100%" },
  grid  = { color = "#dde1e8", font = 13, height = 220 },
  row   = { direction = "row", gap = 16, width = "100%" },
  col   = { direction = "column", gap = 16, grow = 1 },
  field = { font = 13, color = "#e8eaf0", background = "#1c2230", corner = 6, padding = "6 10", width = 160 },
  add   = { font = 13, color = "white", padding = "8 14", corner = 8, background = "#2f6f4f" },
  addhi = { background = "#3a8a62" },
}

-- NOTE: the `data` binding is attached after this setup chunk runs, so all `data.*` calls live
-- INSIDE the returned view function (which runs per-frame, with `data` present), never at module
-- top level. Registration is lazy + idempotent, so calling data.use every frame is free.

local function kpi(label, value)
  return ui.col{ style = S.card,
    ui.text{ label, style = S.klabel },
    ui.text{ value, style = S.kval },
  }
end

local function money(n)
  return "$" .. string.format("%.0f", n or 0)
end

return function()
  -- Register the sources (lazy — the host loads each on first touch, cached by version).
  data.use("orders")
  data.use("customers")

  -- KPIs: single-aggregate queries; :value pulls the one scalar across the boundary.
  local totals = data.sql([[
    SELECT SUM(sales) AS revenue, COUNT(*) AS n, AVG(sales) AS avg FROM orders
  ]])

  -- A computed grouping: revenue + order count per segment. Rendered as a chart AND a grid, both
  -- from the same host-side result (no rows in Lua).
  local by_segment = data.sql([[
    SELECT segment, SUM(sales) AS revenue, COUNT(*) AS orders
    FROM orders GROUP BY segment ORDER BY revenue DESC
  ]])

  -- A JOIN across two sources — falls out of SQL JOIN; empty if "customers" doesn't exist.
  local by_region = data.sql([[
    SELECT c.region, SUM(o.sales) AS revenue
    FROM orders o JOIN customers c ON o.cust = c.cust
    GROUP BY c.region ORDER BY revenue DESC
  ]])

  -- A pivot (cross-tab) — the one named op SQL can't express: segments become columns, status rows.
  local cross = data.sql([[ SELECT segment, status, sales FROM orders ]])
                  :pivot{ on = "segment", index = "status", values = "sales" }

  return ui.col{ style = S.page,
    ui.text{ "Analytics", style = S.h1 },
    ui.text{ "A dashboard over imported `.table` items. Queries run host-side with Polars; charts and grids render natively. Edit the form below and ‘add order’ to write a row back to the orders table — every panel recomputes.", style = S.hint },

    -- KPI row.
    ui.row{ style = S.kpis,
      kpi("Revenue", money(totals:value("revenue"))),
      kpi("Orders", tostring(totals:value("n") or 0)),
      kpi("Avg order", money(totals:value("avg"))),
    },

    -- Chart + grid side by side, both bound to the by_segment result.
    ui.row{ style = S.row,
      ui.col{ style = S.panel,
        ui.text{ "Revenue by segment", style = S.h2 },
        ui.chart{ data = by_segment, type = "bar", x = "segment", y = "revenue", style = S.chart },
      },
      ui.col{ style = S.panel,
        ui.text{ "Per-segment breakdown", style = S.h2 },
        ui.table{ source = by_segment, id = "by_segment", style = S.grid },
      },
    },

    -- JOIN result + pivot cross-tab.
    ui.row{ style = S.row,
      ui.col{ style = S.panel,
        ui.text{ "Revenue by region (orders ⋈ customers)", style = S.h2 },
        ui.table{ source = by_region, id = "by_region", style = S.grid },
      },
      ui.col{ style = S.panel,
        ui.text{ "Sales: status × segment (pivot)", style = S.h2 },
        ui.table{ source = cross, id = "cross", style = S.grid },
      },
    },

    -- Add-row workflow: a small form over this app's own scratch CRDT (doc:text), and a button that
    -- writes a row to the orders SOURCE table via data.table(...):add. The write bumps the source
    -- version, so all the panels above recompute next frame.
    ui.col{ style = S.panel,
      ui.text{ "Add an order", style = S.h2 },
      ui.row{ style = { direction = "row", gap = 10 },
        ui.editor{ value = doc:text("f_segment"), style = S.field },
        ui.editor{ value = doc:text("f_cust"), style = S.field },
        ui.editor{ value = doc:text("f_sales"), style = S.field },
        ui.button{ "add order", style = S.add, hover = S.addhi,
          on_click = function()
            local seg = doc:text("f_segment"):get()
            local cust = doc:text("f_cust"):get()
            local sales = tonumber(doc:text("f_sales"):get()) or 0
            if seg ~= "" then
              data.table("orders"):add{ segment = seg, cust = cust, sales = sales, status = "open" }
              doc:text("f_segment"):set("")
              doc:text("f_cust"):set("")
              doc:text("f_sales"):set("")
            end
          end },
      },
      ui.text{ "segment · cust · sales", style = S.klabel },
    },
  }
end
