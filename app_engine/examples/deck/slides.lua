-- Slide content: plain data, one table per slide, rendered by kind in main.lua.
return {
  { kind = "title",
    title = "osvauld",
    subtitle = "an agent-native collaborative OS",
    footer = "june 2026 · built with the app engine it presents" },

  { kind = "bullets",
    title = "Apps are Lua over CRDTs",
    points = {
      "a view is a pure function of synced state",
      "styles speak CSS — the LLM author stays in-distribution",
      "sandboxed VM: no os, no io, bounded cpu + memory",
      "this deck is one of those apps",
    } },

  { kind = "two_col",
    title = "One engine, two audiences",
    left  = { head = "Humans", body = "edit docs, boards and tables in real time — presence included, no media pipeline." },
    right = { head = "Agents", body = "author and maintain the same apps over MCP; every capability is a permitted CRDT doc." } },

  { kind = "quote",
    text = "The slide position lives in the document — when the presenter moves, every viewer follows.",
    who = "this deck, about itself" },

  { kind = "title",
    title = "fin",
    subtitle = "← → with the buttons below; open it twice to see nav sync",
    footer = "" },
}
