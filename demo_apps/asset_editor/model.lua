-- The open asset: a workspace doc holding a drawing (its views and clips) or a stage (a scene of
-- placed assets). An asset is seeded from its bundled seed; after that the asset is the truth.
local M = {}

M.view = "front"

-- Each seed gives the asset's roots; a drawing's `parts` go under `views`.
M.seeds = {
	hero = function()
		local hero = require("seed/hero")
		return {
			size = hero.size,
			parts = hero.parts,
			clips = require("seed/clips"),
			sockets = { head_top = { kind = "hat", part = "head", at = { 80, 34 }, width = 60 } },
		}
	end,
	toggle = function()
		return require("seed/toggle")
	end,
	chest = function()
		return require("seed/chest")
	end,
	hat = function()
		return require("seed/hat")
	end,
	room = function()
		return require("seed/room")
	end,
}
M.names = { "hero", "toggle", "chest", "hat" }
M.scenes = { room = true }

-- Doc writes are tagged at every depth: a table with only positional entries is a list,
-- anything else a map.
local function tag(v)
	if type(v) ~= "table" then
		return v
	end
	local out, n = {}, 0
	for k, x in pairs(v) do
		out[k] = tag(x)
		n += 1
	end
	return (n > 0 and n == #v) and doc.list(out) or doc.map(out)
end

local loaded = {}

-- An asset by name, writing any root its seed has and it lacks — so a seed that grows (the
-- hero's socket) reaches an asset made before it. What it writes shows from the next frame.
function M.load(name)
	if loaded[name] then
		return loaded[name]
	end
	local a = asset:open(name)
	for k, v in pairs(M.seeds[name] and M.seeds[name]() or {}) do
		local root, value = k, v
		if k == "parts" then
			root, value = "views", { [M.view] = { parts = v } }
		end
		if a[root] == nil then
			a:set({ root }, tag(value))
		end
	end
	loaded[name] = a
	return a
end

function M.open(name)
	M.name = name
	M.asset = M.load(name)
end

-- The current view's parts of a drawing asset, or nil before its seed is visible.
function M.parts(a)
	local views = (a or M.asset).views
	return views and views[M.view] and views[M.view].parts
end

M.open("hero")

return M
