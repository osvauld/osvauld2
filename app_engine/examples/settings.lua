-- settings: many editors + live validation (border flips red, error text appears) + toggle
-- switches built from an absolutely-positioned knob. All state is CRDT (text fields + a flag map).
local BG, PANEL, LINE, FG, MUTED = "#0d1016", "#151a23", "#232a36", "#e8eaf0", "#8a91a0"
local ACCENT, ERR = "#4c8bf5", "#e5484d"

local name  = doc:text("name")
local email = doc:text("email")
local ws    = doc:text("ws_name")
local flags = doc:map("flags")

if name:get() == "" and email:get() == "" then
  name:set("Abraham George")
  email:set("abraham@osvauld.dev")
  ws:set("osvauld core")
end

local function valid_email(s) return s:find("@", 1, true) ~= nil and s:find("%.", s:find("@", 1, true)) ~= nil end

local function field(label, value, id, err)
  return ui.col{ style = { gap = 6 },
    ui.text{ label, style = { font_size = 12, color = MUTED } },
    ui.editor{ value = value, id = id,
      style = { background = "#0d1016", padding = "9 12", border_radius = 8, font_size = 14, color = FG,
                border = err and ("1 " .. ERR) or ("1 " .. LINE) } },
    err and ui.text{ err, style = { font_size = 11, color = ERR } } or nil,
  }
end

local function switch(label, desc, key)
  local on = flags[key] and true or false
  return ui.row{ style = { justify_content = "space-between", align_items = "center", padding = "4 0" },
    on_click = function() flags[key] = not on end,
    hover = { background = "#181d27" },
    ui.col{ style = { gap = 2, flex_grow = 1 },
      ui.text{ label, style = { font_size = 14, color = FG } },
      ui.text{ desc, style = { font_size = 12, color = MUTED } } },
    ui.col{ style = { width = 42, height = 24, border_radius = 999,
                      background = on and ACCENT or "#2a3140" },
      ui.col{ style = { position = "absolute", top = 3, left = on and 21 or 3,
                        width = 18, height = 18, border_radius = 999, background = "white",
                        box_shadow = "0 1 3 #0007" } } },
  }
end

local function section(title, ...)
  return ui.col{ style = { background = PANEL, border = "1 " .. LINE, border_radius = 14,
                           padding = 20, gap = 14 },
    ui.text{ title, style = { font_size = 16, color = FG } },
    ...}
end

return function()
  local name_err  = name:get() == "" and "name can't be empty" or nil
  local email_err = (not valid_email(email:get())) and "that doesn't look like an email" or nil
  local valid = not name_err and not email_err

  return ui.col{ style = { width = "100%", height = "100%", background = BG, align_items = "center" },
    ui.col{ style = { flex_grow = 1, width = "100%", max_width = 640, padding = 24, gap = 16 }, scroll = "page",

      ui.text{ "Settings", style = { font_size = 24, color = FG } },

      section("Profile",
        field("Display name", name, "name", name_err),
        field("Email", email, "email", email_err)),

      section("Workspace",
        field("Workspace name", ws, "ws_name", nil),
        switch("Public read access", "anyone with the link can view", "public"),
        switch("Autosave snapshots", "export a snapshot after every change", "autosave"),
        switch("Agent notifications", "let the agent ping you on doc changes", "notify")),

      ui.row{ style = { justify_content = "end", gap = 10 },
        ui.button{ valid and "Save changes" or "Fix errors to save",
          style = { background = ACCENT, padding = "10 20", border_radius = 10,
                    font_size = 14, color = "white", opacity = valid and 1.0 or 0.35 },
          hover  = valid and { background = "#629af7" } or nil,
          active = valid and { background = "#3b78e0" } or nil,
          on_click = function() if valid then flags.saved = (flags.saved or 0) + 1 end end },
      },
      flags.saved and ui.text{ "saved " .. flags.saved .. "×", style = { font_size = 12, color = MUTED, align_self = "end" } } or nil,
    },
  }
end
