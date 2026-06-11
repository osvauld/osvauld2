-- kit storybook: every component in its variants, like a Storybook page. The buttons are live —
-- clicks count into the CRDT so the page proves interactivity, not just paint.
local K = require("lib.kit")

local BG, FG, MUTED = "#0d1016", K.fg, K.muted
local demo = doc:map("demo")

local function bump() demo.clicks = (demo.clicks or 0) + 1 end

local function section(title, note, ...)
  return ui.col{ style = { gap = 12 },
    ui.row{ style = { gap = 10, align_items = "baseline" },
      ui.text{ title, style = { font_size = 17, color = FG } },
      ui.text{ note or "", style = { font_size = 12, color = MUTED } } },
    ui.col{ style = { background = K.panel, border = "1 " .. K.line, border_radius = 14, padding = 18, gap = 14 },
      ...},
  }
end

return function()
  return ui.col{ style = { width = "100%", height = "100%", background = BG, align_items = "center" },
    ui.col{ style = { flex_grow = 1, width = "100%", max_width = 820, padding = 24, gap = 22 }, scroll = "page",

      ui.row{ style = { justify_content = "space-between", align_items = "center" },
        ui.text{ "Component kit", style = { font_size = 24, color = FG } },
        ui.text{ (demo.clicks or 0) .. " clicks", style = { font_size = 13, color = MUTED } } },

      section("Buttons", "4 variants × 3 sizes, all live",
        ui.row{ style = { gap = 10, align_items = "center", wrap = true },
          K.button{ "Primary", variant = "primary", on_click = bump },
          K.button{ "Secondary", variant = "secondary", on_click = bump },
          K.button{ "Ghost", variant = "ghost", on_click = bump },
          K.button{ "Danger", variant = "danger", on_click = bump } },
        ui.row{ style = { gap = 10, align_items = "center" },
          K.button{ "Small", size = "sm", on_click = bump },
          K.button{ "Medium", size = "md", on_click = bump },
          K.button{ "Large", size = "lg", on_click = bump } }),

      section("Badges & avatars", "oklch hue is the only parameter",
        ui.row{ style = { gap = 10, align_items = "center" },
          K.badge("stable", 150), K.badge("beta", 250), K.badge("deprecated", 25), K.badge("internal", 80),
          K.avatar("AG", 250), K.avatar("CL", 150), K.avatar("OPS", 40, 44) }),

      section("Alerts", "left accent bar via a stretched 4px column",
        K.alert("info",    "Heads up",          "the deck app syncs its slide position over the CRDT."),
        K.alert("success", "Engine tests green", "44 passed, including the offset-click regression."),
        K.alert("warning", "Indic shaping",      "egui fonts can't shape Malayalam yet — parley later."),
        K.alert("error",   "Hot-reload failed",  "a broken main.lua renders this card instead of crashing.")),

      section("Progress", "percent-width fills",
        ui.row{ style = { gap = 12, align_items = "center" },
          ui.text{ "23%", style = { font_size = 12, color = MUTED, width = 36 } }, K.progress(23, 25) },
        ui.row{ style = { gap = 12, align_items = "center" },
          ui.text{ "64%", style = { font_size = 12, color = MUTED, width = 36 } }, K.progress(64, 80) },
        ui.row{ style = { gap = 12, align_items = "center" },
          ui.text{ "100%", style = { font_size = 12, color = MUTED, width = 36 } }, K.progress(100, 150) }),

      section("Stats", "flex_grow splits the row evenly",
        ui.row{ style = { gap = 12 },
          K.stat("Workspaces", "12", "across 3 nodes"),
          K.stat("Apps", "7", "all Lua, all sandboxed"),
          K.stat("Peers", "5", "dial-by-pubkey") }),

      section("Keyboard", "kbd chips with a hard shadow",
        ui.row{ style = { gap = 8, align_items = "center" },
          K.kbd("⌘"), K.kbd("B"),
          ui.text{ "bold the selection", style = { font_size = 12, color = MUTED, margin = "0 14 0 4" } },
          K.kbd("⌘"), K.kbd("E"),
          ui.text{ "inline code", style = { font_size = 12, color = MUTED, margin = "0 0 0 4" } } }),
    },
  }
end
