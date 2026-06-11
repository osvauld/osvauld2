-- Style gallery: exercises every new engine capability in one screen.
local BG     = "#0f1115"
local CARD   = "#1a1e26"
local FG     = "#e8eaf0"
local MUTED  = "#8a91a0"
local ACCENT = "#4c8bf5"

local function card(title, body)
  return ui.col{
    style = {
      background = CARD, padding = 16, gap = 8, width = 250,
      border_radius = 12, border = "1 #2a3040",
      box_shadow = "0 6 18 #00000099",
    },
    ui.text{ title, style = { font_size = 16, color = FG } },
    body,
  }
end

return function()
  return ui.col{
    style = {
      background = BG, width = "100%", height = "100%",
      padding = 24, gap = 16,
    },

    -- centered hero: justify/align on a fixed-height band
    ui.col{
      style = {
        height = 110, justify_content = "center", align_items = "center",
        background = "hsl(222, 30%, 14%)", border_radius = 16,
        border = "1 #2a3040", gap = 6,
      },
      ui.text{ "Style Gallery", style = { font_size = 28, color = FG } },
      ui.text{ "alignment · borders · shadows · opacity · absolute · colors",
               style = { font_size = 13, color = MUTED } },
    },

    -- cards row: shadow + border + per-corner radius + margin
    ui.row{ style = { gap = 16 },
      card("Shadow + border", ui.text{ "This card floats on a soft drop shadow with a 1px hairline.",
        style = { font_size = 13, color = MUTED } }),

      -- per-corner: top corners only (a "tab")
      ui.col{
        style = { background = CARD, padding = 16, width = 250, gap = 8,
                  border_radius = "16 16 0 0", border = "1 #2a3040" },
        ui.text{ "Per-corner radius", style = { font_size = 16, color = FG } },
        ui.text{ "16 16 0 0 — only the top corners round.",
          style = { font_size = 13, color = MUTED } },
      },

      -- opacity ladder
      card("Opacity", ui.row{ style = { gap = 8 },
        ui.col{ style = { width = 40, height = 40, background = ACCENT, border_radius = 8, opacity = 1.0 } },
        ui.col{ style = { width = 40, height = 40, background = ACCENT, border_radius = 8, opacity = 0.66 } },
        ui.col{ style = { width = 40, height = 40, background = ACCENT, border_radius = 8, opacity = 0.33 } },
      }),
    },

    -- absolute positioning: a badge pinned to a banner's corner
    ui.col{
      style = { height = 90, background = "oklch(0.35 0.08 250)", border_radius = 12,
                justify_content = "center", padding = "0 20" },
      ui.text{ "Absolute badge — pinned top-right, out of flow", style = { color = FG } },
      ui.col{
        style = { position = "absolute", top = 10, right = 12,
                  background = "crimson", padding = "4 10", border_radius = 999,
                  box_shadow = "0 2 8 #0008" },
        ui.text{ "NEW", style = { font_size = 12, color = "white" } },
      },
    },

    -- css colors + margin: swatch strip (named, hsl, rgb, oklch)
    ui.row{ style = { gap = 0, align_items = "center" },
      ui.text{ "CSS colors:", style = { font_size = 13, color = MUTED, margin = "0 12 0 0" } },
      ui.col{ style = { width = 36, height = 24, background = "rebeccapurple", border_radius = "6 0 0 6" } },
      ui.col{ style = { width = 36, height = 24, background = "hsl(160, 70%, 45%)" } },
      ui.col{ style = { width = 36, height = 24, background = "rgb(244, 112, 104)" } },
      ui.col{ style = { width = 36, height = 24, background = "oklch(0.7 0.15 80)", border_radius = "0 6 6 0" } },
    },
  }
end
