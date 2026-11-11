-- Brawl's stage and rules in stock Lua (plan: docs/design/brawl-movement.md §0, P-B1..P-B7).
--   lua scripts/test_brawl_rules.lua
package.path = "scripts/?.lua;" .. package.path
local U = require("brawl_test_util")
local F, T, I = U.F, U.tune, U.input

-- P-B1: plain data; a step on two copies of one state with one input gives equal states.
do
	local s, rules = U.fresh()
	local inputs = { I(), I({ x = 1 }), I({ x = 1, jump = true, jump_n = 1 }), I({ x = -1, jump = true, jump_n = 1 }),
		I({ y = 1, down_n = 1 }), I({ restart_n = 1 }) }
	for tick = 1, 300 do
		local ok, why = U.plain(s)
		assert(ok, "P-B1 tick " .. tick .. ": " .. tostring(why))
		local a, b = U.copy(s), U.copy(s)
		local input = inputs[tick % #inputs + 1]
		U.step(a, rules, input)
		U.step(b, rules, input)
		assert(U.same(a, b), "P-B1: two copies stepped alike differ")
		U.step(s, rules, input)
		assert(U.same(s, a), "P-B1: the live state and a stepped copy differ")
	end
end

-- P-B2: standing still on the ground for 600 ticks.
do
	local s, rules = U.fresh()
	local f = s.fighters[1]
	assert(f.y == 0 and f.vy == 0, "P-B2: spawns on the ground")
	for tick = 1, 600 do
		U.step(s, rules)
		assert(f.y == 0 and f.vy == 0 and f.grounded, "P-B2: tick " .. tick .. " y=" .. f.y .. " vy=" .. f.vy)
		assert(f.act == "idle", "P-B2: idle, not " .. f.act)
	end
end

-- P-B3: one-way platforms.
do
	local s, rules = U.fresh()
	local left, li = U.platform(rules, "left")
	local f = s.fighters[1]
	f.x = (left.x + left.x + left.w) / 2
	local above, landed = false, nil
	for tick = 1, 200 do
		U.step(s, rules, I({ jump = true, jump_n = 1 }))
		if f.y < left.y then above = true end
		if f.grounded then landed = tick; break end
		if not above then assert(not f.grounded, "P-B3: caught on the way up") end
	end
	assert(above, "P-B3: the jump passed up through the platform")
	assert(landed and f.y == left.y and f.on == li, "P-B3: lands on top: y=" .. f.y .. " on=" .. tostring(f.on))
	assert(f.vy == 0, "P-B3: at rest on it")

	-- Feet below the top at the start of a tick: never snapped onto it, rising or falling.
	for _, dy in ipairs({ 0.125, 1, 5, 20, 60 }) do
		for _, vy in ipairs({ -40, -20, -13, -6, -1, 0, 3, 14 }) do
			local t, r = U.fresh()
			local g = t.fighters[1]
			g.x, g.y, g.vy, g.grounded, g.on = f.x, left.y + dy, vy, false, 0
			U.step(t, r)
			assert(g.on ~= li, "P-B3: snapped from below (dy " .. dy .. ", vy " .. vy .. "), y=" .. g.y)
		end
	end
end

-- P-B4: drop through.
do
	local s, rules = U.fresh()
	local left, li = U.platform(rules, "left")
	local f = s.fighters[1]
	f.x, f.y, f.grounded = left.x + left.w / 2, left.y - 40, false
	U.step(s, rules, I(), 40)
	assert(f.grounded and f.on == li and f.y == left.y, "P-B4: standing on the platform")
	U.step(s, rules, I({ y = 1, down_n = 1 }))
	local below_at
	if f.y > left.y then below_at = 1 end
	U.step(s, rules, I({ y = 1, down_n = 1 }))
	if not below_at and f.y > left.y then below_at = 2 end
	assert(below_at, "P-B4: below the top within 2 ticks, y=" .. f.y)
	assert(not f.grounded and f.on == 0, "P-B4: falling")

	-- While dropping, a platform under a falling fighter does not catch it; after drop_ticks, one does.
	local t, r = U.fresh()
	local g = t.fighters[1]
	g.x, g.y, g.vy, g.grounded = left.x + left.w / 2, left.y - 5, 10, false
	g.drop = T.drop_ticks
	U.step(t, r)
	assert(g.on == 0 and g.y > left.y, "P-B4: dropping, the platform let it through")
	local t2, r2 = U.fresh()
	local h = t2.fighters[1]
	h.x, h.y, h.vy, h.grounded = left.x + left.w / 2, left.y - 5, 10, false
	U.step(t2, r2)
	assert(h.on == li and h.y == left.y, "P-B4: not dropping, the same fall is caught")

	-- The drop lasts exactly drop_ticks ticks, the press tick included.
	local d, dr = U.fresh()
	local k = d.fighters[1]
	k.x, k.y, k.grounded = left.x + left.w / 2, left.y - 40, false
	U.step(d, dr, I(), 40)
	assert(k.on == li, "P-B4: on the platform again")
	for tick = 1, T.drop_ticks do
		U.step(d, dr, I({ down_n = 1 }))
		assert(k.drop > 0 and k.on == 0, "P-B4: drop tick " .. tick)
	end
	U.step(d, dr, I({ down_n = 1 }))
	assert(k.drop == 0, "P-B4: the drop is over after drop_ticks")

	-- Down on solid ground does nothing.
	local g2, gr = U.fresh()
	local m = g2.fighters[1]
	U.step(g2, gr, I({ y = 1, down_n = 1 }), 30)
	assert(m.y == 0 and m.grounded and m.drop == 0 and m.vy == 0, "P-B4: Down on the ground did something")
end

-- P-B5: walls, both sides of the ground block.
do
	local ground = U.yard.solids[1]
	for _, side in ipairs({ 1, -1 }) do
		local s, rules = U.fresh()
		local f = s.fighters[1]
		local line = side == 1 and ground.x or ground.x + ground.w
		f.x, f.y, f.vy, f.grounded = line - side * (T.body_w / 2 + 60), ground.y + 300, 0, false
		local touched
		for tick = 1, 60 do
			U.step(s, rules, I({ x = side }))
			if f.wall ~= 0 then touched = tick; break end
		end
		assert(touched, "P-B5: reached the wall")
		for _ = 1, 10 do
			assert(f.x == line - side * T.body_w / 2, "P-B5: x at the wall line, x=" .. f.x)
			assert(f.vx == 0 and f.wall == side, "P-B5: vx=" .. f.vx .. " wall=" .. f.wall)
			U.step(s, rules, I({ x = side }))
		end
		-- Let go: still touching, still at the line.
		U.step(s, rules, I())
		assert(f.x == line - side * T.body_w / 2 and f.wall == side, "P-B5: touching without pushing")
	end
end

-- P-B6: blast zone, stocks, respawn, ko and restart.
do
	local b = U.yard.blast
	local outside = {
		{ b.x - 1, -300 }, { b.x + b.w + 1, -300 },
		{ 0, b.y - 1 + T.body_h / 2 - 1 }, { 0, b.y + b.h + T.body_h / 2 + 1 }, -- feet placed so the box centre is just outside
	}
	for i, at in ipairs(outside) do
		local s, rules = U.fresh()
		local f = s.fighters[1]
		f.jumps2, f.dashes, f.wall_jumps = 0, 0, 0
		f.x, f.y, f.vx, f.vy, f.grounded = at[1], at[2], 0, 0, false
		U.step(s, rules)
		assert(f.stocks == T.stocks - 1 and f.act == "out", "P-B6 side " .. i .. ": a stock lost, act " .. f.act)
		U.step(s, rules, I(), T.respawn_ticks - 1)
		assert(f.act == "out", "P-B6: still out before respawn_ticks")
		U.step(s, rules)
		local rp = rules.stage.respawn
		assert(f.act ~= "out" and f.x == rp.x and f.y == rp.y and f.vx == 0 and f.vy == 0,
			"P-B6: back at the respawn point at rest: " .. f.x .. "," .. f.y)
		assert(f.jumps2 == T.air_jumps and f.dashes == T.air_dashes and f.wall_jumps == T.wall_jumps, "P-B6: refreshed")
	end

	local s, rules = U.fresh()
	local f = s.fighters[1]
	for lost = 1, T.stocks do
		f.x, f.y = b.x - 10, -300
		U.step(s, rules)
		assert(f.stocks == T.stocks - lost, "P-B6: stocks " .. f.stocks)
		if lost < T.stocks then U.step(s, rules, I(), T.respawn_ticks) end
	end
	assert(f.act == "ko", "P-B6: no stocks is ko, not " .. f.act)
	U.step(s, rules, I(), 600)
	assert(f.act == "ko" and f.stocks == 0, "P-B6: ko stays ko")
	U.step(s, rules, I({ restart_n = 1 }))
	f = s.fighters[1]
	local sp = rules.stage.spawns[1]
	assert(f.stocks == T.stocks and f.x == sp.x and f.y == sp.y and f.act ~= "ko", "P-B6: a restart press resets")
	-- The same count again is the same press: nothing more.
	U.step(s, rules, I({ restart_n = 1 }), 10)
	assert(s.fighters[1].stocks == T.stocks, "P-B6: one press, one restart")
	-- A restart press while playing does nothing.
	local x = s.fighters[1].x
	U.step(s, rules, I({ restart_n = 2 }))
	assert(s.fighters[1].x == x and s.fighters[1].stocks == T.stocks, "P-B6: restart only after ko")
end

-- P-B7: stage validation names the entry.
do
	local function copy(t)
		if type(t) ~= "table" then return t end
		local c = {}
		for k, v in pairs(t) do c[k] = copy(v) end
		return c
	end
	local function fails(change, want)
		local stage = copy(U.yard)
		change(stage)
		local ok, err = pcall(F.check_stage, stage)
		assert(not ok, "P-B7: accepted a bad stage (" .. want .. ")")
		assert(tostring(err):find(want, 1, true), "P-B7: error should name " .. want .. ": " .. tostring(err))
	end
	assert(F.check_stage(copy(U.yard)), "P-B7: the yard is valid")
	fails(function(s) s.colour = "red" end, "colour")
	fails(function(s) s.art = {} end, "unknown field art")
	fails(function(s) s.platforms[2].wdith = 3 end, "wdith")
	fails(function(s) s.platforms[2].w = nil end, "platforms[2]")
	fails(function(s) s.platforms[1].w = 0 end, "platforms[1]")
	fails(function(s) s.solids[1].h = -1 end, "solids[1]")
	fails(function(s) s.spawns[1] = { x = 0, y = 100 } end, "spawns[1]")
	fails(function(s) s.respawn = { x = 0, y = 300 } end, "respawn")
	fails(function(s) s.blast.w = 1000 end, "solids[1]")
	fails(function(s) s.platforms[3].x = s.blast.x - 500 end, "platforms[3]")
	fails(function(s) s.solids[1].x = "wide" end, "solids[1]")
end

print("brawl rules ok")
