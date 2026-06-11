-- kit: a userland component library — every "widget" is just a Lua function returning styled
-- boxes, the engine has no widget tier. Variants/sizes are plain data tables.
local K = {}

K.fg, K.muted, K.line, K.panel = "#e8eaf0", "#8a91a0", "#232a36", "#151a23"

local button_variants = {
  primary   = { bg = "#4c8bf5", fg = "white",   hover = "#629af7", active = "#3b78e0" },
  secondary = { bg = "#1b212c", fg = "#e8eaf0", hover = "#242b38", active = "#2c3442", border = "1 #2a3140" },
  ghost     = { bg = nil,       fg = "#9aa3b5", hover = "#1a202b", active = "#232a37" },
  danger    = { bg = "#a12832", fg = "white",   hover = "#b8303b", active = "#8c222b" },
}
local button_sizes = {
  sm = { pad = "5 12",  font = 12 },
  md = { pad = "8 16",  font = 14 },
  lg = { pad = "11 22", font = 16 },
}

function K.button(opts)
  local v = button_variants[opts.variant or "primary"]
  local s = button_sizes[opts.size or "md"]
  return ui.button{ opts[1],
    style = { background = v.bg, color = v.fg, border = v.border,
              padding = s.pad, font_size = s.font, border_radius = 8 },
    hover  = { background = v.hover },
    active = { background = v.active },
    on_click = opts.on_click }
end

function K.badge(text, hue)
  return ui.col{ style = { background = string.format("oklch(0.3 0.07 %d)", hue),
                           border = string.format("1 oklch(0.45 0.1 %d)", hue),
                           padding = "2 9", border_radius = 999 },
    ui.text{ text, style = { font_size = 11, color = string.format("oklch(0.8 0.12 %d)", hue) } } }
end

function K.avatar(initials, hue, size)
  size = size or 36
  return ui.col{ style = { width = size, height = size, border_radius = 999,
                           justify_content = "center", align_items = "center",
                           background = string.format("oklch(0.42 0.1 %d)", hue) },
    ui.text{ initials, style = { font_size = size * 0.33, color = "white" } } }
end

local alert_kinds = {
  info    = { hue = 250, icon = "ℹ" },
  success = { hue = 150, icon = "✓" },
  warning = { hue = 80,  icon = "▲" },
  error   = { hue = 25,  icon = "✕" },
}

function K.alert(kind, title, body)
  local a = alert_kinds[kind]
  return ui.row{ style = { background = string.format("oklch(0.2 0.03 %d)", a.hue),
                           border = string.format("1 oklch(0.35 0.07 %d)", a.hue),
                           border_radius = 10, align_items = "stretch" },
    ui.col{ style = { width = 4, background = string.format("oklch(0.65 0.15 %d)", a.hue),
                      border_radius = "10 0 0 10" } },
    ui.row{ style = { padding = 14, gap = 12, flex_grow = 1, align_items = "start" },
      ui.text{ a.icon, style = { font_size = 16, color = string.format("oklch(0.7 0.15 %d)", a.hue) } },
      ui.col{ style = { gap = 4, flex_grow = 1 },
        ui.text{ title, style = { font_size = 14, color = K.fg } },
        ui.text{ body, style = { font_size = 13, color = K.muted } } } } }
end

function K.progress(pct, hue)
  return ui.row{ style = { height = 8, background = "#0d1016", border_radius = 999, flex_grow = 1 },
    ui.col{ style = { width = pct .. "%", border_radius = 999,
                      background = string.format("oklch(0.65 0.15 %d)", hue) } } }
end

function K.stat(label, value, hint)
  return ui.col{ style = { background = K.panel, border = "1 " .. K.line, border_radius = 12,
                           padding = 16, gap = 4, flex_grow = 1 },
    ui.text{ label, style = { font_size = 12, color = K.muted } },
    ui.text{ value, style = { font_size = 24, color = K.fg } },
    ui.text{ hint, style = { font_size = 11, color = K.muted, opacity = 0.7 } } }
end

function K.kbd(text)
  return ui.col{ style = { background = "#1b212c", border = "1 #2f3747", border_radius = 6, padding = "2 7",
                           box_shadow = "0 2 0 #00000066" },
    ui.text{ text, style = { font_size = 11, color = K.muted } } }
end

function K.divider()
  return ui.col{ style = { height = 1, background = K.line } }
end

return K
