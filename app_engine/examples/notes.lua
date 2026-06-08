-- A rich-text note: one editable field over a LoroText. Select a span and press Cmd+B / Cmd+I /
-- Cmd+E to bold / italicise / code it — the marks live in the CRDT and render back into the field.
-- `r##` isn't a thing in Lua; the "#hex" colours are fine in a normal string here.

local S = {
  page   = { padding = 24, gap = 14, background = "#14161a", width = "100%", height = "100%" },
  title  = { font = 22, color = "#e6e6ea" },
  hint   = { font = 13, color = "#9aa0ab", width = "100%" },
  card   = { padding = 16, background = "#1e2228", corner = 10, width = "100%", grow = 1 },
  editor = { font = 17, color = "#e6e6ea", width = "100%" },
}

local note = doc:text("note")
if #note == 0 then
  note:set("Select any of these words and bold them. This whole line is one editable rich-text field — its marks live in the CRDT, so a peer or an agent sees the same styled runs.")
end

return function()
  return ui.col{ style = S.page,
    ui.text{ "Notes", style = S.title },
    ui.text{ "Select text, then ",
      { "Cmd+B", bold = true }, " bold · ",
      { "Cmd+I", italic = true }, " italic · ",
      { "Cmd+E", code = true }, " code.",
      style = S.hint },
    ui.col{ style = S.card,
      ui.editor{ id = "note", value = note, style = S.editor },
    },
  }
end
