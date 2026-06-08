-- app_engine demo — loaded at RUNTIME from this file, not compiled into the binary.
-- This is the "uploadable app" in its simplest form: edit this file, save, and the cell
-- hot-reloads. No rebuild. (The canonical store becomes a CRDT text container later;
-- a file is the stand-in for now.)

local S = {
  page  = { padding = 28, gap = 16, background = "#14161a", width = "100%", height = "100%" },
  title = { font = 24, color = "#e6e6ea" },
  body  = { font = 15, color = "#9aa0ab", width = 440 },
  card  = { padding = 20, gap = 12, background = "#1e2228", corner = 12, width = 360 },
  note  = { font = 18, color = "#e6e6ea" },
  desc  = { font = 14, color = "#9aa0ab", width = 320 },
  pill  = { padding = 7, font = 13, color = "white", background = "#4c8bf5", corner = 7 },
  -- A button with explicit state styles (the :hover / :active equivalent — app-controlled,
  -- not the engine's default tint).
  btn    = { padding = 9, font = 16, color = "#cdd3df", background = "#2d3340", corner = 8 },
  btn_hi = { background = "#3a4150", color = "white" },          -- hover
  btn_lo = { background = "#23272f", color = "#9aa0ab" },        -- active (pressed)
  input  = { width = 320, padding = 10, font = 16, color = "#e6e6ea", background = "#0f1115", corner = 8 },
}

-- Ephemeral local state: a plain Lua upvalue, captured by the view and the click handlers.
-- It is NOT synced, and it resets on hot-reload. (Collaborative state lives in the CRDT.)
local count = 0

-- Seed the editable field once, only when its CRDT text is empty (so it survives reloads and
-- doesn't clobber what you typed). `#` is its length; `:set` replaces the content.
local note = doc:text("note")
if #note == 0 then note:set("edit me — I'm a CRDT field") end

-- Setup runs once; the returned function is the view, re-run on every change.
return function()
  return ui.col{ style = S.page,
    ui.text{ "app_engine — uploaded Lua (edit me, I hot-reload)", style = S.title },
    ui.text{ "Read from disk at run time, and now interactive: click the buttons below.", style = S.body },
    ui.col{ style = S.card,
      ui.text{ "Counter", style = S.note },
      ui.text{ "count: " .. count, style = S.desc },
      ui.row{ style = { gap = 8 },
        ui.button{ "−1", style = S.btn, hover = S.btn_hi, active = S.btn_lo, on_click = function() count = count - 1 end },
        ui.button{ "+1", style = S.btn, hover = S.btn_hi, active = S.btn_lo, on_click = function() count = count + 1 end },
      },
    },
    -- An editable field: click to focus, type, arrows + shift to select, Cmd/Ctrl+A to select
    -- all. Its content is a LoroText — it persists, and an external (MCP) writer edits the same
    -- field. (Marks — bold/italic — land with the rich-text editing slice.)
    ui.col{ style = S.card,
      ui.text{ "Input", style = S.note },
      ui.editor{ id = "note", style = S.input },
    },
    ui.row{ style = { gap = 8 },
      ui.text{ "crdt",  style = S.pill },
      ui.text{ "lua",   style = S.pill },
      ui.text{ "taffy", style = S.pill },
    },
    -- Rich text: any ui.text can carry marks (bold / italic / code / link) and explicit colour.
    ui.text{ style = { font = 15, color = "#9aa0ab", width = 480 },
      "Rich text works now: ",
      { "bold", bold = true, color = "#e6e6ea" }, ", ",
      { "italic", italic = true }, ", ",
      { "code", code = true }, ", ",
      { "a link", link = "https://osvauld" }, ".",
    },
  }
end
