-- Brawl's frame data and hitboxes in stock Lua (plan: docs/design/brawl-fighting.md §0): P-H1, P-H2
-- (moves_baked.lua is the bake of moves.lua and the clips), P-H3 (baked), P-H4, P-H5, P-A5.
--   lua scripts/test_brawl_hitboxes.lua
package.path = "scripts/?.lua;demo_apps/brawl/?.lua;" .. package.path
local U = require("brawl_test_util")
local T = U.tune
local K = require("checks")
local Moves = require("moves")
local Baked = require("moves_baked")
local Look = require("look")

local function read(path)
	local f = assert(io.open(path, "rb"))
	local s = f:read("a")
	f:close()
	return s
end

local function copy(t)
	if type(t) ~= "table" then return t end
	local c = {}
	for k, v in pairs(t) do c[k] = copy(v) end
	return c
end

local function exists(path)
	local f = io.open(path, "rb")
	if f then f:close() end
	return f ~= nil
end

-- Every move's clip exists (step 3), by length.
local clips = {}
for name in pairs(Moves) do
	local path = "demo_apps/brawl/" .. Look.OWN_DIR .. Moves[name].clip .. ".lua"
	assert(exists(path), "P-H1: " .. name .. "'s clip " .. path .. " is missing")
	clips[Moves[name].clip] = dofile(path).length
end

-- P-H2: baked = clips. The bake run again, in-process and as the script, gives the checked-in file;
-- every number is on the 1/8 grid (P-H3 below); a clip edited without a rebake fails, naming the move
-- and the tick.
local Bake = dofile("scripts/bake_brawl_moves.lua")
do
	local lua = arg[-1] or "lua5.4"
	local p = io.popen(lua .. " scripts/bake_brawl_moves.lua --stdout")
	local fresh = p:read("a")
	p:close()
	assert(fresh == read("demo_apps/brawl/moves_baked.lua"), "P-H2: moves_baked.lua is not the bake of moves.lua and the clips: run lua scripts/bake_brawl_moves.lua")
	local bake = Bake.bake(copy(Moves))
	assert(bake.text == fresh, "P-H2: the in-process bake is the script's")
	local why = Bake.diff(Baked.moves, bake.moves)
	assert(why == nil, "P-H2: " .. tostring(why))
	-- sheavy's clip with the far arm turned 12 degrees more from its first key on: its active ticks move
	local edited = Bake.bake(copy(Moves), function(name)
		local c = Bake.clip(name)
		if name == "sheavy" then
			for _, k in ipairs(c.tracks.shoulder_back.rot) do k[2] = k[2] + 12 end
		end
		return c
	end)
	why = Bake.diff(Baked.moves, edited.moves)
	assert(why and why:find("^sheavy tick %d+"), "P-H2: an edited clip names the move and the tick: " .. tostring(why))
	local tick = tonumber(why:match("tick (%d+)"))
	assert(tick >= Moves.sheavy.windows[1].from and tick <= Moves.sheavy.windows[1].to, "P-H2: the first changed tick is active: " .. tick)
	print("P-H2: the bake is current; an edited sheavy reads \"" .. why .. "\"")
end

-- P-H1: moves.lua is checked.
local M, angles = K.moves(Moves, Baked, clips)
do
	local n = 0
	for name, m in pairs(M) do
		n = n + 1
		assert(m.total == m.startup + m.active + m.recovery and m.total % 2 == 0, "P-H1: " .. name .. " total")
		assert(m.clip == name, "P-H1: " .. name .. " plays its own clip, not " .. m.clip)
	end
	assert(n == 12, "P-H1: 11 moves and the jet's ignite, got " .. n)
	print("P-H1: " .. n .. " moves, each on its own clip")
	local function fails(change, want)
		local m = copy(Moves)
		change(m)
		local ok, err = pcall(K.moves, m, Baked, clips)
		assert(not ok, "P-H1: accepted a bad move (" .. want .. ")")
		assert(tostring(err):find(want, 1, true), "P-H1: error should name " .. want .. ": " .. tostring(err))
	end
	fails(function(m) m.nlight.speed = 3 end, "moves.nlight: unknown field speed")
	fails(function(m) m.sair.windows[1].knock = 1 end, "moves.sair.windows[1]: unknown field knock")
	fails(function(m) m.nair.windows[2].circles[1].radius = 3 end, "moves.nair.windows[2].circles[1]: unknown field radius")
	fails(function(m) m.nair.windows[2].circles[1].static = { 0, 0 } end, "moves.nair.windows[2].circles[1]: unknown field static")
	fails(function(m) m.nair.windows[2].circles[1].at = nil end, "moves.nair.windows[2].circles[1]: needs part, at")
	fails(function(m) m.dlight.startup = 9 end, "moves.dlight.startup must be even")
	fails(function(m) m.nlight.windows[1].to = 17 end, "moves.nlight.windows[1]: ticks 11-17 are not inside active")
	fails(function(m) m.nlight.windows[1].from = 10 end, "not inside active")
	fails(function(m) m.nlight.windows[1].dmg = 0 end, "moves.nlight.windows[1].dmg")
	fails(function(m) m.nlight.windows[1].dmg = 7.5 end, "moves.nlight.windows[1].dmg")
	fails(function(m) m.nlight.windows[1].base = 5.001 end, "1/64 grid")
	fails(function(m) m.nlight.windows[1].growth = 1 / 100 end, "1/64 grid")
	fails(function(m) m.nlight.windows[1].angle = 31 end, "angle 31 is not in the angle table")
	fails(function(m) m.nlight.clip = nil end, "moves.nlight: no clip")
	fails(function(m) m.sair.land_lag = nil end, "moves.sair: an aerial needs land_lag")
	fails(function(m) m.zlight = copy(m.nlight) end, "moves.zlight: not in moves_baked.lua")
	-- a clip that exists must be the move's length
	local ok, err = pcall(K.moves, Moves, Baked, { nlight = 0.5 })
	assert(not ok and tostring(err):find("clip nlight is 60 ticks, the move 36", 1, true), "P-H1: clip length: " .. tostring(err))
end

-- P-H3 (baked): circles only inside windows, at least one on every window tick; side moves in
-- front, down moves below the box's middle, dheavy on both sides; `reach` holds them all. Each
-- circle's `at` is on its part's picture (inside the slot's setup bounds), so it rides the limb drawn.
-- (nair's active span 9-20 has a gap, 13-16: "active tick" here is a tick inside a window.)
do
	local SIDE = { slight = true, sheavy = true, sair = true, shair = true }
	local DOWN = { dlight = true, dheavy = true, dair = true, dhair = true }
	for name, m in pairs(M) do
		local left, right = false, false
		for k = 1, m.total do
			local inside = false
			for _, w in ipairs(m.windows) do
				if k >= w.from and k <= w.to then inside = true end
			end
			local cs = m.ticks[k]
			assert(inside == (#cs > 0), "P-H3: " .. name .. " tick " .. k .. ": " .. #cs .. " circles, inside a window " .. tostring(inside))
			for _, c in ipairs(cs) do
				local x, y, r, w = c[1], c[2], c[3], c[4]
				assert(m.windows[w] and k >= m.windows[w].from and k <= m.windows[w].to, "P-H3: " .. name .. " circle names window " .. w)
				for _, v in ipairs({ x, y, r }) do assert(v * 8 == math.floor(v * 8), "P-H3: " .. name .. " off the 1/8 grid") end
				if SIDE[name] then assert(x - r > 0, "P-H3: side move " .. name .. " has a circle behind: x=" .. x) end
				if DOWN[name] then assert(y > -T.body_h / 2, "P-H3: down move " .. name .. " has a circle above the middle: y=" .. y) end
				local b = m.reach
				assert(x - r >= b[1] and y - r >= b[2] and x + r <= b[3] and y + r <= b[4], "P-H3: " .. name .. "'s reach misses a circle")
				if x < 0 then left = true end
				if x > 0 then right = true end
			end
		end
		if name == "dheavy" then assert(left and right, "P-H3: dheavy hits both sides") end
	end
	local A = require("characters/robot/author")
	local scout = require("characters/robot/scout")
	local shapes = {}
	for _, p in ipairs(scout.parts) do shapes[p.id] = p.shapes end
	local w = A.world(A.setup_pose())
	for name, m in pairs(Moves) do
		for wi, win in ipairs(m.windows) do
			for ci, c in ipairs(win.circles) do
				local slot = "slot:" .. c.part
				assert(shapes[slot], "P-H3: " .. name .. " rides " .. c.part .. ", which has no picture")
				local t = w(slot)
				local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
				for _, sh in ipairs(shapes[slot]) do
					for _, cmd in ipairs(sh.path) do
						for i = 2, #cmd, 2 do
							local x, y = t[1] * cmd[i] + t[3] * cmd[i + 1] + t[5], t[2] * cmd[i] + t[4] * cmd[i + 1] + t[6]
							x0, y0, x1, y1 = math.min(x0, x), math.min(y0, y), math.max(x1, x), math.max(y1, y)
						end
					end
				end
				assert(c.at[1] >= x0 and c.at[1] <= x1 and c.at[2] >= y0 and c.at[2] <= y1,
					string.format("P-H3: %s window %d circle %d: at (%g, %g) is off %s (%.1f %.1f %.1f %.1f)", name, wi, ci, c.at[1], c.at[2], slot, x0, y0, x1, y1))
			end
		end
	end
end

-- P-H4: the angle table: unit length within 1/256, the angle within 0.25 degrees. (Trig here only.)
do
	local n = 0
	for deg, v in pairs(angles) do
		n = n + 1
		local len = math.sqrt(v.x * v.x + v.y * v.y)
		assert(math.abs(len - 1) <= 1 / 256, "P-H4: " .. deg .. " has length " .. len)
		local got = math.deg(math.atan(v.y, v.x)) % 360
		local d = math.abs(got - deg)
		if d > 180 then d = 360 - d end
		assert(d <= 0.25, "P-H4: " .. deg .. " points at " .. got)
		assert(v.x * 256 == math.floor(v.x * 256) and v.y * 256 == math.floor(v.y * 256), "P-H4: " .. deg .. " off the 1/256 grid")
	end
	assert(angles[30].x == 0.8671875 and angles[30].y == 0.5, "P-H4: 30 degrees is (0.8671875, 0.5), as P-F5 says")
	assert(angles[60] and angles[300], "P-H4: a downward angle has its mirror")
	print("P-H4: " .. n .. " angles")
end

-- P-H5: the rule modules use no platform maths, no `^`, no `pairs` and no clock (comments and
-- strings left out of the scan).
do
	local function code(src)
		src = src:gsub("%-%-%[(=*)%[.-%]%1%]", "")
		src = src:gsub("%-%-[^\n]*", "")
		src = src:gsub('"[^"\n]*"', '""'):gsub("'[^'\n]*'", "''")
		return src
	end
	local banned = { "math%.sin", "math%.cos", "math%.tan", "math%.atan", "math%.sqrt", "math%.exp", "math%.random",
		"math%.pow", "%^", "%f[%w_]pairs%f[^%w_]", "%f[%w_]os%.", "%f[%w_]now%(", "clock" }
	local seen = 0
	for _, mod in ipairs({ "fighter", "combat", "jet", "bot" }) do
		local path = "demo_apps/brawl/" .. mod .. ".lua"
		if exists(path) then
			seen = seen + 1
			local src = code(read(path))
			for _, b in ipairs(banned) do
				assert(not src:find(b), "P-H5: " .. mod .. ".lua uses " .. b)
			end
		end
	end
	assert(seen == 4, "P-H5: fighter, combat, jet and bot scanned")
end

-- P-A5: Look.ACTS is total over the new acts; every move and both dodges play their own clip (step 3),
-- with a stand-in for the frames before it is warmed.
do
	local all = {}
	for _, n in ipairs(Look.all_clips()) do all[n] = true end
	local acts = { "lag", "dodge_spot", "dodge_dir", "hurt", "jet", "jet_hover", "jet_thrust", "jet_cut" }
	for name in pairs(Moves) do acts[#acts + 1] = name end
	for _, act in ipairs(acts) do
		local a = Look.ACTS[act]
		assert(a, "P-A5: no look for act " .. act)
		assert(all[a.clip], "P-A5: act " .. act .. " names clip " .. tostring(a.clip) .. ", not one of Look's")
		local m = M[act]
		if m then
			assert(a.clip == act, "P-A5: " .. act .. " plays " .. a.clip)
			assert(a.stand == (m.air and "apex" or "idle"), "P-A5: " .. act .. "'s stand-in is " .. tostring(a.stand))
			assert((a.plant == true) == not m.air, "P-A5: a ground move stands planted, an aerial does not: " .. act)
		end
		if act == "dodge_spot" or act == "dodge_dir" then assert(a.clip == act and a.plant, "P-A5: " .. act .. " plays its own clip, planted") end
		if act:sub(1, 4) == "jet_" then assert(a.clip == act and not a.plant and a.stand == "apex", "P-A5: " .. act .. " plays its own clip in the air") end
	end
end

print("brawl hitboxes ok")
