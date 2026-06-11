-- dashboard: data→style mapping. Bars, progress fills and color scales are all computed from
-- plain Lua data — no chart library, just styled boxes.
local BG, PANEL, LINE, FG, MUTED = "#0d1016", "#151a23", "#232a36", "#e8eaf0", "#8a91a0"
local GOOD, BAD = "#3fb46f", "#e5484d"

local kpis = {
  { label = "Active users",  value = "12,418", delta = "+8.2%",  good = true },
  { label = "p95 latency",   value = "184ms",  delta = "-11ms",  good = true },
  { label = "Error rate",    value = "0.42%",  delta = "+0.09%", good = false },
  { label = "Open incidents", value = "3",     delta = "+1",     good = false },
}

local deploys = {
  { day = "mon", n = 9 }, { day = "tue", n = 14 }, { day = "wed", n = 6 },
  { day = "thu", n = 17 }, { day = "fri", n = 11 }, { day = "sat", n = 2 }, { day = "sun", n = 4 },
}

local rollouts = {
  { name = "vault v2",        pct = 92 },
  { name = "iroh transport",  pct = 64 },
  { name = "lua sandbox",     pct = 100 },
  { name = "presence",        pct = 23 },
}

local activity = {
  { who = "AG", what = "merged engine CSS expansion",     when = "2m",  hue = 250 },
  { who = "CL", what = "authored deck + dashboard apps",  when = "9m",  hue = 150 },
  { who = "AG", what = "restarted gatekeeper node",       when = "1h",  hue = 250 },
  { who = "OPS", what = "rotated workspace permits",      when = "3h",  hue = 40 },
}

local function card(extra, ...)
  local style = { background = PANEL, border = "1 " .. LINE, border_radius = 14, padding = 18, gap = 12 }
  for k, v in pairs(extra) do style[k] = v end
  return ui.col{ style = style, ... }
end

local function kpi(k)
  return card({ flex_grow = 1 },
    ui.text{ k.label, style = { font_size = 12, color = MUTED } },
    ui.row{ style = { justify_content = "space-between", align_items = "center" },
      ui.text{ k.value, style = { font_size = 26, color = FG } },
      ui.col{ style = { background = k.good and "#15301f" or "#33181a", border_radius = 999, padding = "2 8" },
        ui.text{ k.delta, style = { font_size = 11, color = k.good and GOOD or BAD } } },
    })
end

local function bar_chart()
  local max = 0
  for _, d in ipairs(deploys) do if d.n > max then max = d.n end end
  local bars = {}
  for _, d in ipairs(deploys) do
    local h = math.max(6, math.floor(d.n / max * 130))
    bars[#bars + 1] = ui.col{ style = { flex_grow = 1, align_items = "center", gap = 6, justify_content = "end" },
      ui.text{ tostring(d.n), style = { font_size = 11, color = MUTED } },
      -- color scale: lightness tracks the value
      ui.col{ style = { width = "70%", height = h, border_radius = "6 6 0 0",
                        background = string.format("oklch(%.2f 0.14 250)", 0.45 + d.n / max * 0.3) } },
      ui.text{ d.day, style = { font_size = 11, color = MUTED } },
    }
  end
  return card({ flex_grow = 2 },
    ui.text{ "Deploys this week", style = { font_size = 15, color = FG } },
    ui.row{ style = { gap = 8, height = 180, align_items = "end" }, table.unpack(bars) })
end

local function rollout_rows()
  local rows = {}
  for _, r in ipairs(rollouts) do
    local color = r.pct == 100 and GOOD or string.format("oklch(0.7 0.15 %d)", 200 + r.pct)
    rows[#rows + 1] = ui.col{ style = { gap = 6 },
      ui.row{ style = { justify_content = "space-between" },
        ui.text{ r.name, style = { font_size = 13, color = FG } },
        ui.text{ r.pct .. "%", style = { font_size = 13, color = MUTED } } },
      ui.row{ style = { height = 8, background = "#0d1016", border_radius = 999 },
        ui.col{ style = { width = r.pct .. "%", background = color, border_radius = 999 } } },
    }
  end
  return card({ flex_grow = 1, gap = 14 },
    ui.text{ "Rollouts", style = { font_size = 15, color = FG } },
    table.unpack(rows))
end

local function activity_rows()
  local rows = {}
  for _, a in ipairs(activity) do
    rows[#rows + 1] = ui.row{ style = { gap = 12, align_items = "center" },
      ui.col{ style = { width = 32, height = 32, border_radius = 999, justify_content = "center", align_items = "center",
                        background = string.format("oklch(0.4 0.09 %d)", a.hue) },
        ui.text{ a.who, style = { font_size = 11, color = FG } } },
      ui.text{ a.what, style = { font_size = 13, color = FG, flex_grow = 1 } },
      ui.text{ a.when, style = { font_size = 12, color = MUTED } },
    }
  end
  return card({ flex_grow = 1, gap = 14 },
    ui.text{ "Activity", style = { font_size = 15, color = FG } },
    table.unpack(rows))
end

return function()
  return ui.col{ style = { width = "100%", height = "100%", background = BG },
    ui.col{ style = { flex_grow = 1, padding = 22, gap = 16 }, scroll = "page",

      ui.row{ style = { justify_content = "space-between", align_items = "center" },
        ui.text{ "Operations", style = { font_size = 24, color = FG } },
        ui.row{ style = { gap = 8, align_items = "center" },
          ui.col{ style = { width = 8, height = 8, border_radius = 999, background = GOOD } },
          ui.text{ "live", style = { font_size = 13, color = MUTED } } } },

      ui.row{ style = { gap = 14 }, kpi(kpis[1]), kpi(kpis[2]), kpi(kpis[3]), kpi(kpis[4]) },
      ui.row{ style = { gap = 14, align_items = "stretch" }, bar_chart(), rollout_rows() },
      activity_rows(),
    },
  }
end
