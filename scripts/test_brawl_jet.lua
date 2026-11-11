-- Brawl's jetpack in stock Lua (plan: docs/design/brawl-fighting.md §0, P-J1-J6), through F.step.
--   lua scripts/test_brawl_jet.lua
package.path = "scripts/?.lua;demo_apps/brawl/?.lua;" .. package.path
local U = require("brawl_test_util")
local T, I = U.tune, U.input
local M = require("combat").moves

local function put(f, x, y, face)
	f.x, f.y, f.vx, f.vy = x, y or 0, 0, 0
	f.face = face or f.face
	f.grounded = (y or 0) == 0
	f.on, f.coyote = 0, f.grounded and T.coyote or 0
	f.act, f.act_t = f.grounded and "idle" or "fall", 0
	f.take_y, f.top_y = f.y, f.y
end
local function solo(x, y)
	local s, rules = U.fresh()
	put(s.fighters[1], x or -800, y or -900, 1)
	return s, rules, s.fighters[1]
end
-- K held from a press (heavy_n = 1), with a direction.
local function held(x, y) return I({ heavy_n = 1, heavy = true, x = x or 0, y = y or 0 }) end
local JET = { jet_hover = true, jet_thrust = true }

assert(T.jet_kick == 8 and T.jet_cost == 24 and T.jet_fuel == 120 and T.jet_accel == 0.375 and T.jet_rise == 6
	and T.jet_side == 8 and T.jet_refill == 4 and T.cut_ticks == 10, "the plan's jet numbers in tune.lua")

-- P-J1: ignite. Heavy, neutral or Up, in the air: vy = -8 on the press tick, gravity on, act `jet` for
-- 12 ticks, rising exactly 76.5; fuel - jet_cost.
for _, up in ipairs({ 0, -1 }) do
	local s, rules, f = solo()
	assert(f.fuel == T.jet_fuel, "P-J1: a fighter starts with a full tank")
	local y0 = f.y
	U.step(s, rules, held(0, up))
	assert(f.act == "jet" and f.act_t == 0 and f.vy == -8 + T.gravity, "P-J1: ignite, act " .. f.act .. " vy " .. f.vy)
	assert(f.fuel == T.jet_fuel - T.jet_cost, "P-J1: fuel " .. f.fuel)
	assert(f.jet_pow == 3, "P-J1: the ignite burns at jet_pow 3")
	for k = 2, 12 do
		U.step(s, rules, held(0, up))
		assert(f.act == "jet" and f.act_t == k - 1, "P-J1: act jet for 12 ticks, tick " .. k .. " " .. f.act)
	end
	assert(y0 - f.y == 76.5 and f.vy == -5, "P-J1: the rise is " .. (y0 - f.y) .. ", vy " .. f.vy)
	assert(f.fuel == T.jet_fuel - T.jet_cost, "P-J1: no burn during the ignite")
end
do
	-- no ignite below jet_cost; with jet_cost it does
	for _, fuel in ipairs({ T.jet_cost - 1, T.jet_cost }) do
		local s, rules, f = solo()
		f.fuel = fuel
		U.step(s, rules, held())
		if fuel < T.jet_cost then
			assert(f.act ~= "jet" and f.move == "" and f.fuel == fuel and f.vy == T.gravity, "P-J4: no ignite at fuel " .. fuel)
		else
			assert(f.act == "jet" and f.fuel == 0, "P-J1: ignite at exactly jet_cost")
		end
	end
	-- the ignite is a move: committed (no jump, dash, dodge), and its uppercut hits by the P-F rules
	local s, rules, f = solo()
	f.jumps2 = 1
	U.step(s, rules, held())
	for k = 2, 12 do U.step(s, rules, I({ heavy_n = 1, heavy = true, jump = true, jump_n = k, dash_n = k, dodge_n = k })) end
	assert(f.act == "jet" and f.jumps2 == 1 and f.dash_t == 0 and f.dodge == "", "P-J1: the ignite commits")
	local m = M.jet
	assert(m and m.air and m.total == 12 and m.windows[1].from == 5 and m.windows[1].to == 12, "P-J1: jet is the 12-tick air move, window 5-12")
	local w = m.windows[1]
	assert(w.dmg == 9 and w.base == 6 and w.growth == 5 / 64 and w.angle == 85, "P-J1: the uppercut's numbers")
	-- a dummy standing where the first live circle is on the first active tick, both in the air
	local c = m.ticks[w.from][1]
	local s2, r2 = U.fresh({ "human", "dummy" })
	local a, d = s2.fighters[1], s2.fighters[2]
	put(a, -800, -900, 1)
	-- the attacker has risen sum(8 - 0.25k), k = 1..5 = 36.25 when move tick 5's hits are gathered; the
	-- circle is put on the box's middle (the dummy falls 3.75 meanwhile, well inside r)
	put(d, -800 + c[1], -900 - 36.25 + c[2] + T.body_h / 2, -1)
	local hit = false
	for _ = 1, 12 do
		U.step(s2, r2, { held(), I() })
		if d.dmg > 0 then hit = true break end
	end
	assert(hit and d.dmg == 9 and d.last_angle == 85 and d.last_kb == 6 + 9 * 5 / 64, "P-J1: the ignite hits: dmg " .. d.dmg .. " angle " .. d.last_angle)
	print(string.format("P-J1: the ignite's first circle (%g, %g) r %g; hits the dummy for 9, kb %g at 85", c[1], c[2], c[3], d.last_kb))
end

-- P-J2: hover. Heavy still held, no direction: gravity off, vy approaches 0 by jet_accel, from -5 to
-- exactly 0 on the 14th hover tick; the peak from rest is exactly 107.375; fuel -1 a tick; jet_pow 1.
do
	local s, rules, f = solo()
	local y0 = f.y
	U.step(s, rules, held(), 12)
	local fuel = f.fuel
	local top = f.y
	for k = 1, 40 do
		U.step(s, rules, held())
		assert(f.act == "jet_hover" and f.act_t == k - 1 and f.jet_pow == 1, "P-J2: hover tick " .. k .. ": " .. f.act .. " pow " .. f.jet_pow)
		assert(f.fuel == fuel - k, "P-J2: fuel -1 a tick")
		if k < 14 then assert(f.vy == -5 + k * T.jet_accel, "P-J2: vy " .. f.vy .. " on hover tick " .. k) end
		if k >= 14 then assert(f.vy == 0, "P-J2: vy 0 from the 14th hover tick, tick " .. k .. " vy " .. f.vy) end
		top = math.min(top, f.y)
	end
	assert(y0 - top == 107.375 and f.y == top, "P-J2: the peak is " .. (y0 - top))
	-- from a fall, hover brakes it toward 0 the same way
	local s2, r2, g = solo()
	g.fuel = T.jet_fuel
	U.step(s2, r2, held(), 12)
	g.vy = 3
	U.step(s2, r2, held())
	assert(g.vy == 3 - T.jet_accel and g.act == "jet_hover", "P-J2: a falling hover brakes: " .. g.vy)
end

-- P-J3: climb and thrust. Up: vy -> -jet_rise, jet_pow 2, still `jet_hover`. A side: vx -> x * jet_side,
-- act `jet_thrust`, facing follows, jet_pow 2. Down is ignored (a hover; no fast fall).
do
	local s, rules, f = solo()
	U.step(s, rules, held(), 12)
	local want = { -5.375, -5.75, -6, -6, -6 }
	for k = 1, #want do
		U.step(s, rules, held(0, -1))
		assert(f.vy == want[k] and f.act == "jet_hover" and f.jet_pow == 2, "P-J3: climb tick " .. k .. " vy " .. f.vy .. " pow " .. f.jet_pow)
	end
	for _, side in ipairs({ 1, -1 }) do
		local s2, r2, g = solo()
		U.step(s2, r2, held(), 12)
		local vx = 0
		for k = 1, 30 do
			U.step(s2, r2, held(side, 0))
			vx = side > 0 and math.min(vx + T.jet_accel, T.jet_side) or math.max(vx - T.jet_accel, -T.jet_side)
			assert(g.vx == vx and g.act == "jet_thrust" and g.face == side and g.jet_pow == 2, "P-J3: thrust tick " .. k .. " vx " .. g.vx .. " act " .. g.act)
			if k >= 14 then assert(g.vy == 0, "P-J3: a side alone holds the height") end
		end
		assert(g.vx == side * T.jet_side, "P-J3: thrust reaches jet_side")
		-- side and Up together: thrust and climb
		U.step(s2, r2, held(side, -1), 20)
		assert(g.act == "jet_thrust" and g.vy == -6 and g.vx == side * T.jet_side, "P-J3: side + Up")
	end
	local s3, r3, h = solo()
	U.step(s3, r3, held(), 12)
	U.step(s3, r3, I({ heavy_n = 1, heavy = true, y = 1, down_n = 1 }), 20)
	assert(h.act == "jet_hover" and h.jet_pow == 1 and h.vy == 0 and not h.ff, "P-J3: Down is a hover: vy " .. h.vy)
end

-- P-J4: the tank.
do
	-- held from full: 96 jet ticks after the ignite, then jet_cut for cut_ticks, then a plain air act
	local s, rules, f = solo()
	U.step(s, rules, held(), 12)
	local jet, cut = 0, 0
	for _ = 1, 200 do
		U.step(s, rules, held())
		if JET[f.act] then
			assert(cut == 0, "P-J4: no jet after the cut")
			jet = jet + 1
		elseif f.act == "jet_cut" then
			cut = cut + 1
			assert(f.jet_pow == 0, "P-J4: no flame in the cut")
		else
			break
		end
	end
	assert(jet == T.jet_fuel - T.jet_cost and cut == T.cut_ticks, "P-J4: " .. jet .. " jet ticks, " .. cut .. " cut ticks")
	assert(f.act == "apex" or f.act == "fall", "P-J4: then falls, act " .. f.act)
	assert(f.fuel == 0, "P-J4: empty")
	-- empty: no ignite, and no refill in the air
	U.step(s, rules, I({ heavy_n = 2, heavy = true }), 30)
	assert(f.act ~= "jet" and f.fuel == 0, "P-J4: no ignite and no refill in the air, act " .. f.act .. " fuel " .. f.fuel)
	-- on the ground it refills jet_refill a tick: 30 ticks from empty, the landing tick the first
	local s2, r2, g = solo(-400, -100)
	g.fuel = 0
	local landed = 0
	for _ = 1, 200 do
		U.step(s2, r2, I())
		if g.grounded then
			landed = landed + 1
			assert(g.fuel == math.min(T.jet_fuel, landed * T.jet_refill), "P-J4: refill " .. g.fuel .. " on ground tick " .. landed)
			if g.fuel == T.jet_fuel then break end
		else
			assert(g.fuel == 0, "P-J4: no refill in the air")
		end
	end
	assert(landed == 30, "P-J4: full in " .. landed .. " ground ticks")
	-- letting go cuts at once, and the cut stops the rise
	local s3, r3, h = solo()
	U.step(s3, r3, held(0, -1), 12 + 8)
	assert(h.act == "jet_hover" and h.vy < 0)
	local fuel = h.fuel
	U.step(s3, r3, I({ heavy_n = 1 }))
	assert(h.act == "jet_cut" and h.act_t == 0 and h.vy == T.gravity and h.fuel == fuel, "P-J4: let go: " .. h.act .. " vy " .. h.vy)
	-- the ignite let go of runs its 12 ticks, then cuts
	local s4, r4, k4 = solo()
	U.step(s4, r4, held())
	U.step(s4, r4, I({ heavy_n = 1 }), 11)
	assert(k4.act == "jet" and k4.act_t == 11)
	U.step(s4, r4, I({ heavy_n = 1 }))
	assert(k4.act == "jet_cut" and k4.vy == T.gravity, "P-J4: an ignite let go of cuts after it: " .. k4.act)
	-- a jump, dash or dodge press ends the jet without jet_cut
	for _, p in ipairs({ { "jump_n", "jump2" }, { "dash_n", "dash" }, { "dodge_n", "dodge_spot" } }) do
		local s5, r5, g5 = solo()
		U.step(s5, r5, held(), 20)
		assert(g5.act == "jet_hover")
		local fuel5 = g5.fuel
		U.step(s5, r5, I({ heavy_n = 1, heavy = true, [p[1]] = 1, jump = p[1] == "jump_n" }))
		assert(g5.act == p[2] and g5.jet == "" and g5.jet_pow == 0 and g5.fuel == fuel5, "P-J4: " .. p[1] .. " ends the jet: " .. g5.act)
		for _ = 1, 40 do
			U.step(s5, r5, I({ heavy_n = 1, heavy = true, [p[1]] = 1, jump = p[1] == "jump_n" }))
			assert(g5.act ~= "jet_cut" and not JET[g5.act], "P-J4: no cut and no jet after " .. p[1] .. ": " .. g5.act)
		end
	end
	-- landing ends it without a cut; the cut is free (a press acts in it) and a re-ignite needs jet_cost
	local s6, r6, g6 = solo(-400, -40)
	U.step(s6, r6, held())
	U.step(s6, r6, held(), 11)
	g6.vy = 12
	for _ = 1, 30 do
		U.step(s6, r6, held())
		if g6.grounded then break end
	end
	assert(g6.grounded and g6.jet == "" and g6.act ~= "jet_cut", "P-J4: landing ends the jet, act " .. g6.act)
	local s7, r7, g7 = solo()
	U.step(s7, r7, held(), 20)
	U.step(s7, r7, I({ heavy_n = 1 }), 3)
	assert(g7.act == "jet_cut")
	U.step(s7, r7, I({ heavy_n = 2, heavy = true }))
	assert(g7.act == "jet" and g7.act_t == 0, "P-J4: a re-ignite in the cut, act " .. g7.act)
	local s8, r8, g8 = solo()
	U.step(s8, r8, held(), 20)
	U.step(s8, r8, I({ heavy_n = 1 }), 3)
	U.step(s8, r8, I({ heavy_n = 1, light_n = 1 }))
	assert(g8.act == "nair", "P-J4: an aerial out of the cut, act " .. g8.act)
	-- a light press in the jet ends it (no cut) and starts the aerial
	local s9, r9, g9 = solo()
	U.step(s9, r9, held(), 20)
	U.step(s9, r9, I({ heavy_n = 1, heavy = true, light_n = 1 }))
	assert(g9.act == "nair" and g9.jet == "", "P-J4: an aerial out of the jet, act " .. g9.act)
end

-- P-J5: a recovery, not flight.
do
	-- a full tank's climb from rest gains exactly 651.625 at its highest, Up held throughout
	local s, rules, f = solo(1500, 1000)
	local y0, top = f.y, f.y
	for _ = 1, 200 do
		U.step(s, rules, held(0, -1))
		top = math.min(top, f.y)
	end
	assert(y0 - top == 651.625, "P-J5: the held climb gains " .. (y0 - top))
	-- tapping ignites as fast as the fuel allows gains less
	local s2, r2, g = solo(1500, 1000)
	local n, gy0, gtop = 1, g.y, g.y
	for t = 1, 400 do
		if g.act ~= "jet" and g.fuel >= T.jet_cost then n = n + 1 end
		U.step(s2, r2, I({ heavy_n = n, heavy = false }))
		gtop = math.min(gtop, g.y)
		if g.act == "out" then break end
	end
	assert(gy0 - gtop < 651.625, "P-J5: ignites alone gain " .. (gy0 - gtop))
	print(string.format("P-J5: held climb 651.625; ignites alone %.17g", gy0 - gtop))
	-- From (edge + 300, 400) falling at max_fall: the double jump, then the jet in and up, lands on the
	-- stage. From (edge + 300, 1000) with the double jump spent it does not.
	local edge = 0
	for _, sol in ipairs(U.yard.solids) do edge = math.max(edge, sol.x + sol.w) end
	local function recover(y, jumps)
		local s3, r3, h = solo(edge + 300, y)
		h.vy, h.jumps2, h.face = T.max_fall, jumps, -1
		local jn, hn, phase = 0, 0, "jump"
		for t = 1, 900 do
			local inp
			if phase == "jump" then
				jn = jn + 1
				inp = I({ jump = true, jump_n = jn, x = -1 })
				phase = "rise"
			elseif phase == "rise" then
				inp = I({ jump = true, jump_n = jn, x = -1 })
				if h.vy >= -2 then phase = "ignite" end
			elseif phase == "ignite" then
				hn = hn + 1
				inp = I({ heavy_n = hn, heavy = true, y = -1, jump_n = jn })
				phase = "jet"
			elseif phase == "jet" then
				inp = I({ heavy_n = hn, heavy = true, x = -1, y = -1, jump_n = jn })
				if h.y < -40 and h.x < edge - 40 then phase = "drop" end
			else
				inp = I({ heavy_n = hn, x = -1, jump_n = jn })
			end
			U.step(s3, r3, inp)
			if h.act == "out" then return false, t end
			if h.grounded and h.y == 0 then return true, t, h end
		end
		return false, 900
	end
	local ok, t, h = recover(400, 1)
	assert(ok and h.x < edge, "P-J5: from (edge + 300, 400) the double jump and the jet make the stage")
	local ok2, t2 = recover(1000, 0)
	assert(not ok2, "P-J5: from (edge + 300, 1000) without the double jump it does not")
	print(string.format("P-J5: recovered from (edge + 300, 400) in %d ticks, fuel left %g; from 1000 out after %d", t, h.fuel, t2))
end

-- P-J6: a hit during the jet stops it: hitstun, no jet_cut; the fuel left is kept.
do
	local s, rules = U.fresh({ "human", "human" })
	local a, v = s.fighters[1], s.fighters[2]
	put(v, -700, -700, -1)
	U.step(s, rules, { I(), held() }, 30)
	assert(v.act == "jet_hover", "P-J6: the victim hovers, act " .. v.act)
	put(a, v.x - 40, v.y, 1)
	local fuel
	for _ = 1, 20 do
		fuel = v.fuel
		U.step(s, rules, { I({ light_n = 1 }), held() })
		if v.dmg > 0 then break end
	end
	assert(v.dmg > 0 and v.act == "hurt" and v.jet == "" and v.jet_pow == 0, "P-J6: hit in the jet: " .. v.act .. " dmg " .. v.dmg)
	-- (the tick it is hit it was still burning: that tick's fuel is gone, nothing more)
	assert(v.fuel == fuel - 1 and v.fuel > 0, "P-J6: the fuel is kept: " .. v.fuel .. " from " .. fuel)
	local kept = v.fuel
	local cut = false
	for _ = 1, 120 do
		U.step(s, rules, { I({ light_n = 1 }), held() })
		if v.act == "jet_cut" or JET[v.act] then cut = true end
		if not v.grounded and v.y < -10 then assert(v.fuel == kept, "P-J6: no burn and no refill in the air after the hit") end
	end
	assert(not cut, "P-J6: no jet_cut, and K still held does not relight it")
end

-- A respawn fills the tank.
do
	local s, rules, f = solo()
	f.fuel, f.x = 0, 2200
	U.step(s, rules, I(), 1 + T.respawn_ticks)
	assert(f.act ~= "out" and f.fuel == T.jet_fuel, "a respawn fills the tank: " .. f.act .. " " .. f.fuel)
end

-- Plain data, and a copy stepped alike stays equal (P-B1 over the jet's state).
do
	local s, rules = solo()
	U.step(s, rules, held(1, -1), 30)
	local ok, why = U.plain(s)
	assert(ok, tostring(why))
	local c1, c2 = U.copy(s), U.copy(s)
	U.step(c1, rules, held(1, -1), 50)
	U.step(c2, rules, held(1, -1), 50)
	ok, why = U.same(c1, c2)
	assert(ok, tostring(why))
end

print("brawl jet ok")
