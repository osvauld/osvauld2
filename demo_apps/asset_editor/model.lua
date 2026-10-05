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
			vars = hero.vars,
			parts = hero.parts,
			clips = require("seed/clips"),
			sockets = { head_top = { kind = "hat", part = "head", at = { 80, 34 }, width = 60 } },
			-- Top-down, only the feet meet a wall: a small circle where they stand.
			world = { collider = { circle = 16, at = { 80, 222 } } },
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
	crate = function()
		return require("seed/crate")
	end,
	room = function()
		return require("seed/room")
	end,
}
M.names = { "hero", "toggle", "chest", "hat", "crate" }
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
M.tag = tag

-- Part `id` and every part under it, as a set: a moved arm takes its hand along.
function M.family(parts, id)
	local ids, grew = { [id] = true }, true
	while grew do -- a child may be listed before its parent
		grew = false
		for _, p in ipairs(parts) do
			if p.parent and ids[p.parent] and not ids[p.id] then
				ids[p.id], grew = true, true
			end
		end
	end
	return ids
end

local function shift(p, d)
	return p and { p[1] + d[1], p[2] + d[2] }
end

-- A copy of `part` moved by `d`: its pivot and every point it draws in drawing space — path
-- points, gradient points, a shadow's corner. Lengths, angles and stops stay.
function M.moved(part, d)
	local out = table.clone(part)
	out.pivot = shift(part.pivot, d)
	out.shapes = {}
	for i, s in ipairs(part.shapes or {}) do
		local c = table.clone(s)
		if s.shadow then
			local r = s.shadow.rect
			c.shadow = table.clone(s.shadow)
			c.shadow.rect = { r[1] + d[1], r[2] + d[2], r[3], r[4] }
		end
		if s.path then
			c.path = {}
			for j, cmd in ipairs(s.path) do
				local m = { cmd[1] }
				for k = 2, #cmd do -- x, y, x, y…
					m[k] = cmd[k] + d[1 + k % 2]
				end
				c.path[j] = m
			end
		end
		if type(s.fill) == "table" then
			local kind, g = next(s.fill)
			local f = table.clone(g)
			f.from, f.to = shift(g.from, d), shift(g.to, d)
			f.center, f.focus = shift(g.center, d), shift(g.focus, d)
			c.fill = { [kind] = f }
		end
		out.shapes[i] = c
	end
	return out
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
