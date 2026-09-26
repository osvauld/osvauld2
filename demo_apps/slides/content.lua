-- The deck is data: one table per slide, rendered by `kind` in main.lua.
return {
	{
		id = "title", kind = "title",
		eyebrow = "LIGHTNING TALK",
		title = "How I built a perpendicular browser",
		sub = "Not a Chrome fork. Not an Electron app. A browser at 90° to the web.",
		foot = "Osvauld protocol  ·  Sthalam browser",
	},
	{
		id = "map", kind = "table",
		eyebrow = "THE PIECES",
		title = "If you know the web, you know the shape.",
		rows = {
			{ "the web", "Xnet" },
			{ "HTTP", "Osvauld — the protocol" },
			{ "Chrome", "Sthalam — the browser" },
			{ "a web server", "a node: a Raspberry Pi, a used laptop" },
			{ "a web page", "a Lua app" },
		},
	},
	{
		id = "page", kind = "code",
		eyebrow = "A PAGE IS A LUA APP",
		title = "Describe the screen as boxes.",
		-- demo_apps/tally/main.lua with its styling props removed.
		code = [[local t = doc:open("tally")  -- synced document

return function()
  local n = t.count and t.count.n or 0
  local function add(d)
    return function() t:set({ "count", "n" }, n + d) end
  end
  return ui.col({  -- stack top to bottom
    ui.text({ tostring(n) }),
    ui.row({  -- side by side
      ui.button({ id = "minus", ui.text({ "−" }), on_click = add(-1) }),
      ui.button({ id = "plus", ui.text({ "+" }), on_click = add(1) }),
    }),
  })
end]],
		note = "This one is live. Click it.",
	},
	{
		id = "box", kind = "bullets",
		eyebrow = "WHAT'S IN THE BOX",
		title = "Built from the ground up.",
		points = {
			"Our own browser engine — wgpu, Vello, Parley, Taffy. Indic text included.",
			"Sandboxed Luau apps with hot reload.",
			"Loro CRDT documents: local first, merged on sync.",
			"Signed permits: every person is a key; access is a role you can delegate or revoke.",
			"A node on a Raspberry Pi or an old laptop. Transport is pluggable.",
		},
	},
	{
		id = "ai", kind = "bullets",
		eyebrow = "AI CAN DRIVE IT",
		title = "Same app, no screen scraping.",
		points = {
			"Reads the app's element tree and its data.",
			"Clicks, types and drags through the app's own handlers.",
			"Edits the source of a running app — it hot-reloads, or rolls back.",
		},
	},
	{
		id = "arm", kind = "app", app = "linkage",
		title = "Robotic arm — drag a joint",
	},
	{
		id = "3d", kind = "app", app = "model_viewer",
		title = "3D — orbit, zoom, click",
	},
	{
		id = "charts", kind = "app", app = "dashboard",
		title = "Charts, drawn in Lua",
	},
	{
		id = "kanban", kind = "app", app = "kanban",
		title = "Kanban — drag and drop",
	},
	{
		id = "learn", kind = "app", app = "math_mela",
		title = "A learning kit for kids",
	},
	{
		id = "demo", kind = "title",
		eyebrow = "DEMO",
		title = "Let's open Sthalam.",
		sub = "The same apps, each in its own tab — and an AI editing one live.",
		foot = "github.com/osvauld  ·  docs.osvauld.com",
	},
}
