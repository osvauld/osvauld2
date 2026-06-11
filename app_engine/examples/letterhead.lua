-- letterhead: the reference print app. Declares an A4 page (the host shows a white sheet
-- preview, 1 px = 1 pt, with an "export pdf" action); the company chrome is template (this
-- file), the addressee/body live in the CRDT — editable by clicking into the sheet, by a peer,
-- or over MCP (app_data_set_text), all the same op.
page = { size = "A4" } -- or orientation = "landscape", or width/height in mm

local NAVY, INK, MUTED, RULE = "#1a2b4c", "#222222", "#667085", "#d0d5dd"

local to   = doc:text("to")
local body = doc:text("body")
if to:get() == "" and body:get() == "" then
  to:set("To whom it may concern")
  body:set("Type the letter here — or ask the agent to.")
end

return function()
  return ui.col{ style = { padding = "64 56", gap = 0 },
    -- ── company header ────────────────────────────────────────────────
    ui.row{ style = { justify_content = "space-between", align_items = "flex-end" },
      ui.col{ style = { gap = 2 },
        ui.text{ "ACME Corporation", style = { font_size = 26, color = NAVY } },
        ui.text{ "Quality since 1962", style = { font_size = 11, color = MUTED } } },
      ui.col{ style = { gap = 2, align_items = "flex-end" },
        ui.text{ "12 Foundry Lane, Kochi 682001", style = { font_size = 10, color = MUTED } },
        ui.text{ "hello@acme.example · +91 484 000 0000", style = { font_size = 10, color = MUTED } } } },
    ui.col{ style = { height = 2, background = NAVY, margin = "14 0 36 0" } },

    -- ── the letter (CRDT-backed, click to edit) ───────────────────────
    ui.editor{ value = to, id = "to",
      style = { font_size = 13, color = INK, padding = "2 0" } },
    ui.col{ style = { height = 12 } },
    ui.editor{ value = body, id = "body",
      style = { font_size = 13, color = INK, padding = "2 0", flex_grow = 1 } },

    -- ── footer ────────────────────────────────────────────────────────
    ui.col{ style = { height = 1, background = RULE, margin = "0 0 10 0" } },
    ui.text{ "ACME Corporation · CIN U00000KL1962PLC000000", style = { font_size = 9, color = MUTED } },
  }
end
