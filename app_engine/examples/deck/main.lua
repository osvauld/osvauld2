-- deck: a presentation app. Slide index lives in the CRDT (doc:map "nav"), so navigation
-- syncs to every viewer — presenter mode for free.
local slides = require("slides")

local BG, FG, MUTED, ACCENT = "#0c0e13", "#eceef4", "#8a91a0", "#4c8bf5"
local nav = doc:map("nav")

local function cur()
  local i = nav.i or 1
  if i < 1 then i = 1 end
  if i > #slides then i = #slides end
  return i
end

local function go(d)
  local i = cur() + d
  if i >= 1 and i <= #slides then nav.i = i end
end

-- per-slide accent hue, walked around the wheel
local function hue(i) return 230 + (i - 1) * 40 end

local function render_title(s, i)
  return ui.col{
    style = { flex_grow = 1, justify_content = "center", align_items = "center", gap = 16 },
    ui.text{ s.title, style = { font_size = 56, color = FG } },
    ui.col{ style = { width = 70, height = 4, border_radius = 2,
                      background = string.format("oklch(0.7 0.17 %d)", hue(i)) } },
    ui.text{ s.subtitle, style = { font_size = 20, color = MUTED } },
    s.footer ~= "" and ui.text{ s.footer, style = { font_size = 13, color = MUTED, opacity = 0.7, margin = "26 0 0 0" } } or nil,
  }
end

local function render_bullets(s, i)
  local rows = {}
  for _, p in ipairs(s.points) do
    rows[#rows + 1] = ui.row{ style = { gap = 14, align_items = "start" },
      ui.text{ "›", style = { font_size = 20, color = string.format("oklch(0.7 0.17 %d)", hue(i)) } },
      ui.text{ p, style = { font_size = 21, color = FG } },
    }
  end
  return ui.col{ style = { flex_grow = 1, padding = "70 90", gap = 26, justify_content = "center" },
    ui.text{ s.title, style = { font_size = 36, color = FG, margin = "0 0 12 0" } },
    table.unpack(rows) }
end

local function panel(side, i)
  return ui.col{
    style = { flex_grow = 1, background = "#161a22", border = "1 #262c38", border_radius = 14,
              padding = 26, gap = 12 },
    ui.text{ side.head, style = { font_size = 22, color = string.format("oklch(0.75 0.15 %d)", hue(i)) } },
    ui.text{ side.body, style = { font_size = 17, color = MUTED } },
  }
end

local function render_two_col(s, i)
  return ui.col{ style = { flex_grow = 1, padding = "70 90", gap = 28, justify_content = "center" },
    ui.text{ s.title, style = { font_size = 36, color = FG } },
    ui.row{ style = { gap = 20, align_items = "stretch" }, panel(s.left, i), panel(s.right, i) },
  }
end

local function render_quote(s, i)
  return ui.col{
    style = { flex_grow = 1, justify_content = "center", align_items = "center", padding = "0 110", gap = 20,
              background = string.format("oklch(0.22 0.04 %d)", hue(i)) },
    ui.text{ "“", style = { font_size = 80, color = string.format("oklch(0.65 0.16 %d)", hue(i)), opacity = 0.8 } },
    ui.text{ { s.text, italic = true }, style = { font_size = 26, color = FG } },
    ui.text{ "— " .. s.who, style = { font_size = 15, color = MUTED } },
  }
end

local render = { title = render_title, bullets = render_bullets, two_col = render_two_col, quote = render_quote }

local function nav_button(label, d, enabled)
  return ui.button{ label,
    style = { background = "#1b212c", border = "1 #2a3140", border_radius = 8,
              padding = "6 16", font_size = 16, color = FG, opacity = enabled and 1.0 or 0.3 },
    hover  = { background = "#242b38" },
    active = { background = "#2c3442" },
    on_click = function() go(d) end }
end

return function()
  local i = cur()
  local s = slides[i]
  return ui.col{ style = { width = "100%", height = "100%", background = BG },

    (render[s.kind] or render_title)(s, i),

    -- progress hairline + control bar, pinned under the slide
    ui.row{ style = { height = 3, background = "#1b212c" },
      ui.col{ style = { width = string.format("%d%%", math.floor(i / #slides * 100)),
                        background = string.format("oklch(0.7 0.17 %d)", hue(i)) } } },
    ui.row{ style = { padding = "12 20", justify_content = "space-between", align_items = "center" },
      ui.text{ "osvauld · deck", style = { font_size = 13, color = MUTED, opacity = 0.7 } },
      ui.row{ style = { gap = 10, align_items = "center" },
        nav_button("‹", -1, i > 1),
        ui.text{ i .. " / " .. #slides, style = { font_size = 14, color = MUTED } },
        nav_button("›", 1, i < #slides),
      },
    },
  }
end
