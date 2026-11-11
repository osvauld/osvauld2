-- Brawl's movement in stock Lua (plan: docs/design/brawl-movement.md §0). Step 1: P-M1, P-M3 and
-- P-M12 over the moves step 1 has (run, jump, drop through, walls, blast zone, restart).
--   lua scripts/test_brawl_moves.lua
package.path = "scripts/?.lua;" .. package.path
local U = require("brawl_test_util")
local F, T, I = U.F, U.tune, U.input

-- P-M1: run, exactly, both ways; release stops in run_speed / ground_friction ticks, never past 0.
for _, dir in ipairs({ 1, -1 }) do
	local s, rules = U.fresh()
	local f = s.fighters[1]
	local x = f.x
	for n = 1, 20 do
		U.step(s, rules, I({ x = dir }))
		assert(f.vx == dir * math.min(n * T.ground_accel, T.run_speed), "P-M1: tick " .. n .. " vx=" .. f.vx)
		assert(f.face == dir and f.act == "run", "P-M1: facing and running")
	end
	assert(f.vx == dir * T.run_speed and (f.x - x) * dir > 0, "P-M1: at run speed, moved")
	local stop
	for n = 1, 20 do
		U.step(s, rules, I())
		assert(f.vx * dir >= 0, "P-M1: release went past 0: " .. f.vx)
		if f.vx == 0 and not stop then stop = n end
	end
	assert(stop == 7, "P-M1: stopped on tick " .. tostring(stop) .. ", not 7")
	assert(T.run_speed / T.ground_accel == 8, "P-M1: 8 ticks from rest to run_speed")
	assert(f.act == "idle", "P-M1: idle after stopping")
end

-- P-M3: full jump. The press tick is tick 1.
do
	local s, rules = U.fresh()
	local f = s.fighters[1]
	local top, top_tick, land
	for tick = 1, 200 do
		U.step(s, rules, I({ jump = true, jump_n = 1 }))
		if not top or f.y < top then top, top_tick = f.y, tick end
		if tick > 1 and f.grounded then land = tick; break end
	end
	assert(top == -331.5, "P-M3: peak " .. tostring(-top))
	assert(top_tick == 51, "P-M3: peak reached on tick " .. top_tick)
	assert(land == 103 and f.y == 0, "P-M3: landed on tick " .. tostring(land))
	assert(f.peak == 331.5, "P-M3: the state's peak is " .. tostring(f.peak))
end


-- Helpers. A fighter in the air at rest, a fighter at a wall, and a tick-by-tick recorder.
local function air(x, y, vx, vy)
	local s, rules = U.fresh()
	local f = s.fighters[1]
	f.x, f.y, f.vx, f.vy, f.grounded, f.on, f.act = x, y, vx or 0, vy or 0, false, 0, "fall"
	f.take_y, f.top_y, f.coyote = y, y, 0
	return s, rules, f
end
local WALL_X = U.yard.solids[1].x + U.yard.solids[1].w + T.body_w / 2 -- right of the ground block: it touches the box's left side
local function at_wall(vy)
	local s, rules, f = air(WALL_X, 100, 0, vy or 0)
	U.step(s, rules) -- touching is read at the end of a tick
	f.vy = vy or 0
	assert(f.wall == -1, "setup: touching the wall, wall=" .. f.wall)
	return s, rules, f
end
local function run(s, rules, f, n, input, each)
	for tick = 1, n do
		U.step(s, rules, input)
		if each then each(tick, f) end
	end
end

-- P-M2: turn. At run_speed, holding the other way skids: act turn, vx falls by skid a tick, clamps
-- to 0 on the 5th, facing flips on that tick; then it runs.
do
	assert(T.run_speed - 4 * T.skid > 0 and T.run_speed - 5 * T.skid < 0, "P-M2: the skid takes 5 ticks")
	for _, dir in ipairs({ 1, -1 }) do
		local s, rules = U.fresh()
		local f = s.fighters[1]
		run(s, rules, f, 8, I({ x = dir }))
		assert(f.vx == dir * T.run_speed and f.face == dir)
		local x = dir * T.run_speed
		for n = 1, 5 do
			U.step(s, rules, I({ x = -dir }))
			x = x - dir * T.skid
			if n == 5 then x = 0 end
			assert(f.vx == x, "P-M2: skid tick " .. n .. " vx=" .. f.vx)
			assert(f.act == "turn", "P-M2: skid tick " .. n .. " act=" .. f.act)
			assert(f.face == (n == 5 and -dir or dir), "P-M2: skid tick " .. n .. " face=" .. f.face)
		end
		U.step(s, rules, I({ x = -dir }))
		assert(f.act == "run" and f.face == -dir and f.vx == -dir * T.ground_accel, "P-M2: then it runs: vx=" .. f.vx)
	end
	-- Below turn_min: facing flips at once, no turn.
	local s, rules = U.fresh()
	local f = s.fighters[1]
	run(s, rules, f, 4, I({ x = 1 }))
	assert(f.vx == 3.5 and 3.5 < T.turn_min)
	U.step(s, rules, I({ x = -1 }))
	assert(f.face == -1 and f.act == "run" and f.vx == 3.5 - T.ground_accel, "P-M2: slow reversal: " .. f.act .. " " .. f.vx)
	-- At exactly turn_min it skids.
	local s2, r2 = U.fresh()
	local g = s2.fighters[1]
	g.vx = T.turn_min
	U.step(s2, r2, I({ x = -1 }))
	assert(g.act == "turn" and g.face == 1 and g.vx == T.turn_min - T.skid, "P-M2: skids at turn_min")
	-- Letting go mid-skid stops the skid: plain friction.
	local s3, r3 = U.fresh()
	local h = s3.fighters[1]
	run(s3, r3, h, 8, I({ x = 1 }))
	run(s3, r3, h, 2, I({ x = -1 }))
	U.step(s3, r3, I())
	assert(h.act ~= "turn" and h.vx == T.run_speed - 2 * T.skid - T.ground_friction, "P-M2: released mid-skid, vx=" .. h.vx)
	-- In the air there is no skid: facing flips at once.
	local s4, r4, a = air(750, -300, T.run_speed, 0)
	U.step(s4, r4, I({ x = -1 }))
	assert(a.face == -1 and a.act ~= "turn" and a.vx == T.run_speed - T.air_accel, "P-M2: air reversal")
end

-- P-M4: short hop. Released on tick 2: peak 107.25. Later: between. A press and release inside one
-- input frame (the press tick already reads "released") is the same short hop.
do
	local function peak_released_on(k)
		local s, rules = U.fresh()
		local f = s.fighters[1]
		local top = 0
		for tick = 1, 140 do
			local held = k == nil or tick < k
			U.step(s, rules, I({ jump = held, jump_n = 1 }))
			top = math.min(top, f.y)
			if tick > 1 and f.grounded then break end
		end
		assert(f.grounded and f.y == 0, "P-M4: lands")
		return -top
	end
	assert(peak_released_on(2) == 107.25, "P-M4: release on tick 2 peaks at " .. peak_released_on(2))
	assert(peak_released_on(nil) == 331.5, "P-M4: held peaks at 331.5")
	assert(peak_released_on(1) == 107.25, "P-M4: press and release in one frame")
	local last = 107.25
	for k = 3, 60 do
		local p = peak_released_on(k)
		assert(p >= last and p <= 331.5, "P-M4: release on tick " .. k .. " peaks at " .. p .. " after " .. last)
		last = p
	end
	assert(last == 331.5, "P-M4: released late is the full jump")
	-- With the input in frames of 4 ticks: press and release inside one frame, then a held press.
	local s, rules = U.fresh()
	local f = s.fighters[1]
	local top = 0
	for tick = 1, 140 do
		U.step(s, rules, I({ jump = false, jump_n = 1 }))
		top = math.min(top, f.y)
		if tick > 1 and f.grounded then break end
	end
	assert(top == -107.25, "P-M4: frames of 4")
	-- Landing clears it: the next held jump is full.
	U.step(s, rules, I({ jump = true, jump_n = 2 }))
	local top2 = 0
	for _ = 1, 140 do
		U.step(s, rules, I({ jump = true, jump_n = 2 }))
		top2 = math.min(top2, f.y)
	end
	assert(top2 == -331.5, "P-M4: next jump is full: " .. top2)
	-- The cut is for the ground jump only: a double jump let go early still goes the full 282.
	local d, dr, a = air(750, -300, 0, 0)
	local y0 = a.y
	local top3 = a.y
	U.step(d, dr, I({ jump = false, jump_n = 1 }))
	for _ = 1, 60 do
		top3 = math.min(top3, a.y)
		U.step(d, dr, I({ jump = false, jump_n = 1 }))
	end
	assert(y0 - top3 == 282, "P-M4: double jump is not cut: " .. (y0 - top3))
end

-- P-M5: double jump.
do
	local s, rules, f = air(750, -300, 0, 0)
	local y0 = f.y
	f.jumps2 = T.air_jumps
	local top = y0
	U.step(s, rules, I({ jump = true, jump_n = 1 }))
	assert(f.vy == -T.double_jump_speed + T.gravity and f.act == "jump2" and f.jumps2 == 0, "P-M5: vy=" .. f.vy .. " act=" .. f.act)
	for _ = 1, 200 do
		top = math.min(top, f.y)
		if f.grounded then break end
		U.step(s, rules, I({ jump = true, jump_n = 1 }))
		if f.vy < 0 then assert(f.act == "jump2", "P-M5: jump2 while rising") end
	end
	assert(y0 - top == 282, "P-M5: peak " .. (y0 - top))
	assert(f.grounded and f.jumps2 == T.air_jumps, "P-M5: landing refreshes")

	-- A third press (no double left, no wall): does nothing. Compare to a run without it.
	local a, ar, af = air(750, -300, 0, 0)
	local b, br, bf = air(750, -300, 0, 0)
	U.step(a, ar, I({ jump_n = 1 }))
	U.step(b, br, I({ jump_n = 1 }))
	U.step(a, ar, I({ jump_n = 1 }), 3)
	U.step(b, br, I({ jump_n = 1 }), 3)
	U.step(a, ar, I({ jump_n = 2 }))
	U.step(b, br, I({ jump_n = 1 }))
	assert(af.vy == bf.vy and af.y == bf.y and af.jumps2 == 0, "P-M5: a third press did something")
	-- One per airtime: a ground jump leaves it; two presses after leave one double jump.
	local g, gr = U.fresh()
	local gf = g.fighters[1]
	U.step(g, gr, I({ jump = true, jump_n = 1 }), 5)
	assert(gf.jumps2 == 1 and gf.act == "rise")
	U.step(g, gr, I({ jump = true, jump_n = 2 }))
	assert(gf.jumps2 == 0 and gf.act == "jump2")
	-- Respawn refreshes it, too (P-B6 checks the fields).
end

-- P-M6: coyote time, off the right end of the left platform.
do
	local left, li = U.platform(U.rules(), "left")
	local function off_ledge(k)
		local s, rules = U.fresh()
		local f = s.fighters[1]
		f.x, f.y, f.grounded, f.on, f.vy = left.x + left.w - 50, left.y, true, li, 0
		U.step(s, rules, I())
		assert(f.grounded and f.on == li)
		local n = 0
		repeat -- walk off: tick W is the first that ends in the air
			U.step(s, rules, I({ x = 1 }))
			n = n + 1
			assert(n < 40)
		until not f.grounded
		for _ = 1, k - 1 do U.step(s, rules, I({ x = 1 })) end
		U.step(s, rules, I({ x = 1, jump = true, jump_n = 1 })) -- the press, k ticks after W
		return s, rules, f
	end
	for k = 1, T.coyote + 4 do
		local _, _, f = off_ledge(k)
		if k <= T.coyote then
			assert(f.vy == -T.jump_speed + T.gravity and f.jumps2 == T.air_jumps and f.act == "rise",
				"P-M6: k=" .. k .. " should be a ground jump: vy=" .. f.vy .. " jumps2=" .. f.jumps2)
		else
			assert(f.vy == -T.double_jump_speed + T.gravity and f.jumps2 == 0 and f.act == "jump2",
				"P-M6: k=" .. k .. " should be the double jump: vy=" .. f.vy)
		end
	end
	-- Coyote is spent by a jump: a second press right after is the double jump, not a second ground jump.
	local s, rules, f = off_ledge(2)
	U.step(s, rules, I({ jump = true, jump_n = 2 }))
	assert(f.jumps2 == 0 and f.act == "jump2", "P-M6: coyote jump spent")
	-- A drop-through is not a ledge: no coyote.
	local d, dr = U.fresh()
	local df = d.fighters[1]
	df.x, df.y, df.grounded, df.on, df.vy = left.x + 100, left.y, true, li, 0
	df.jumps2 = 0
	U.step(d, dr, I({ y = 1, down_n = 1 }))
	U.step(d, dr, I({ jump = true, jump_n = 1 }))
	assert(df.vy > 0 and df.jumps2 == 0, "P-M6: no coyote after a drop-through, vy=" .. df.vy)
end

-- P-M7: jump buffer. k = ticks from the press to the first tick that starts on the ground (where
-- the buffered jump happens).
do
	local function fall_with_press(k)
		local c, cr, cf = air(750, -200, 0, 0)
		cf.jumps2 = 0
		local landed = 0
		repeat U.step(c, cr, I()); landed = landed + 1 until cf.grounded
		local first = landed + 1 -- the first tick that starts on the ground
		local s, rules, f = air(750, -200, 0, 0)
		f.jumps2 = 0
		for tick = 1, first do U.step(s, rules, I({ jump_n = tick >= first - k and 1 or 0 })) end
		return f
	end
	for k = 1, T.buffer + 3 do
		local f = fall_with_press(k)
		if k <= T.buffer then
			assert(f.vy == -T.jump_speed + T.gravity and f.act == "rise", "P-M7: k=" .. k .. " should jump: vy=" .. f.vy .. " " .. f.act)
		else
			-- (step 5: a landing with no direction held is `land`, the picture's act)
			assert(f.grounded and f.y == 0 and f.act == "land", "P-M7: k=" .. k .. " is too early: " .. f.act)
		end
	end
	-- With a double jump left, an air press is used at once; it is not saved for the landing.
	local s, rules, f = air(750, -200, 0, 0)
	U.step(s, rules, I({ jump_n = 1 }))
	assert(f.act == "jump2")
	local n = 0
	repeat U.step(s, rules, I({ jump_n = 1 })); n = n + 1 until f.grounded
	U.step(s, rules, I({ jump_n = 1 }))
	assert(f.grounded and f.y == 0, "P-M7: a used press is not buffered")
end

-- P-M8: fast fall.
do
	local function fall(press_at, ticks, start_vy, ff)
		local s, rules, f = air(750, -1200, 0, start_vy or 0)
		f.jumps2 = 0
		local vys = {}
		for tick = 1, ticks do
			U.step(s, rules, I({ y = 1, down_n = tick >= (press_at or 1e9) and 1 or 0 }))
			vys[tick] = f.vy
		end
		return vys, f
	end
	local control = fall(nil, 80)
	assert(control[80] == T.max_fall and math.max(table.unpack(control)) == T.max_fall, "P-M8: a normal fall caps at max_fall")
	local v = fall(10, 20)
	assert(v[9] == control[9] and v[10] == T.fast_fall and v[11] == T.fast_fall and v[20] == T.fast_fall, "P-M8: fast fall at once")
	assert(T.fast_fall > T.max_fall)
	-- Held Down with no new press does nothing.
	local held = fall(nil, 10)
	assert(held[10] == control[10], "P-M8: only a fresh press")
	-- While rising: nothing.
	local s, rules, f = air(750, -600, 0, -5)
	f.jumps2 = 0
	local a, ar, af = air(750, -600, 0, -5)
	af.jumps2 = 0
	U.step(s, rules, I({ y = 1, down_n = 1 }))
	U.step(a, ar, I())
	assert(f.vy == af.vy and f.vy < 0, "P-M8: down while rising does nothing")
	-- It lasts until landing, then the next fall is normal.
	local l, lr, lf = air(750, -300, 0, 2)
	lf.jumps2 = 0
	U.step(l, lr, I({ down_n = 1 }))
	run(l, lr, lf, 40, I(), function() assert(lf.grounded or lf.vy == T.fast_fall) end)
	assert(lf.grounded)
	U.step(l, lr, I({ jump = true, jump_n = 1 }), 1)
	run(l, lr, lf, 70, I({ jump = true, jump_n = 1 }), function() assert(lf.vy <= T.max_fall, "P-M8: capped again after landing") end)
end

-- P-M9: dash.
do
	local function dash_run(input, setup)
		local s, rules, f = air(750, -500, 0, 0)
		if setup then setup(f) end
		local xs, ys = {}, {}
		local x0, y0 = f.x, f.y
		for tick = 1, T.dash_ticks do
			U.step(s, rules, input(tick))
			xs[tick], ys[tick] = f.x - x0, f.y - y0
			assert(f.act == "dash", "P-M9: dash tick " .. tick .. " act=" .. f.act)
		end
		return s, rules, f, xs, ys
	end
	local function press(x, y) return function(tick) return I({ x = x, y = y, dash_n = 1 }) end end
	-- straight, both ways: 12 ticks of 14, no gravity, then the velocity halves.
	for _, dir in ipairs({ 1, -1 }) do
		local s, rules, f, xs, ys = dash_run(press(dir, 0))
		assert(xs[T.dash_ticks] == dir * 168 and ys[T.dash_ticks] == 0, "P-M9: straight dash covers 168: " .. xs[T.dash_ticks])
		assert(f.vx == dir * T.dash_speed * T.dash_keep and f.vy == 0 and f.dashes == 0, "P-M9: after: vx=" .. f.vx)
		U.step(s, rules, I({ x = dir, dash_n = 1 }))
		assert(f.act ~= "dash" and f.vy == T.gravity, "P-M9: gravity is back; a second dash does nothing")
	end
	-- diagonal, vertical, and facing when no direction is held
	local _, _, f, xs, ys = dash_run(press(1, -1))
	assert(xs[12] == 120 and ys[12] == -120 and f.vx == 5 and f.vy == -5, "P-M9: diagonal")
	_, _, f, xs, ys = dash_run(press(-1, 1))
	assert(xs[12] == -120 and ys[12] == 120, "P-M9: diagonal down-left")
	_, _, f, xs, ys = dash_run(press(0, -1))
	assert(xs[12] == 0 and ys[12] == -168, "P-M9: straight up")
	_, _, f, xs, ys = dash_run(press(0, 0), function(f) f.face = -1 end)
	assert(xs[12] == -168 and f.face == -1, "P-M9: facing")
	-- steering is off during the dash
	local s, rules, g = air(750, -500, 0, 0)
	U.step(s, rules, I({ x = 1, dash_n = 1 }))
	U.step(s, rules, I({ x = -1, dash_n = 1 }), 5)
	assert(g.vx == T.dash_speed, "P-M9: no steering in a dash")
	-- landing refreshes; a wall touch refreshes
	local l, lr, lf = air(500, -50, 0, 0)
	U.step(l, lr, I({ dash_n = 1 }))
	run(l, lr, lf, 40, I({ dash_n = 1 }))
	assert(lf.grounded and lf.dashes == T.air_dashes, "P-M9: landing refreshes")
	local w, wr, wf = at_wall(0)
	wf.dashes = 0
	U.step(w, wr)
	assert(wf.dashes == T.air_dashes, "P-M9: touching a wall refreshes")
	-- On the ground: horizontal, the same numbers, free (the air dash is not spent).
	local gs, gr = U.fresh()
	local gf = gs.fighters[1]
	for tick = 1, T.dash_ticks do
		U.step(gs, gr, I({ x = 1, y = -1, dash_n = 1 }))
		assert(gf.act == "dash" and gf.grounded and gf.y == 0 and gf.vx == (tick < T.dash_ticks and T.dash_speed or T.dash_speed * T.dash_keep), "P-M9: ground dash tick " .. tick)
	end
	assert(gf.x == 168 and gf.vx == 7 and gf.dashes == T.air_dashes, "P-M9: ground dash covers 168, x=" .. gf.x)
	-- Off a ledge a ground dash keeps going and falls.
	local ls, lr2 = U.fresh()
	local lf2 = ls.fighters[1]
	lf2.x = U.yard.solids[1].x + U.yard.solids[1].w - 100
	U.step(ls, lr2, I({ x = 1, dash_n = 1 }))
	for _ = 1, 10 do U.step(ls, lr2, I({ x = 1, dash_n = 1 })) end
	assert(not lf2.grounded and lf2.vy > 0 and lf2.dashes == T.air_dashes, "P-M9: dashed off the edge, falling")
	-- A jump cancels a dash and keeps its speed.
	local js, jr = U.fresh()
	local jf = js.fighters[1]
	U.step(js, jr, I({ x = 1, dash_n = 1 }), 3)
	U.step(js, jr, I({ x = 1, dash_n = 1, jump = true, jump_n = 1 }))
	assert(jf.vy == -T.jump_speed + T.gravity and jf.vx == T.dash_speed and jf.act == "rise", "P-M9: dash-jump")
	U.step(js, jr, I({ x = 1, dash_n = 1, jump = true, jump_n = 1 }))
	assert(jf.act ~= "dash", "P-M9: the dash is over")
	-- A dash press on the tick of a jump is ignored; a dash down onto the ground ends there.
	local ds, dr = U.fresh()
	local df = ds.fighters[1]
	U.step(ds, dr, I({ x = 1, jump = true, jump_n = 1, dash_n = 1 }))
	assert(df.act == "rise" and df.dashes == T.air_dashes and df.dash_t == 0, "P-M9: jump wins over dash")
	local es, er, ef = air(500, -100, 0, 0)
	U.step(es, er, I({ y = 1, dash_n = 1 }), 8)
	assert(ef.grounded and ef.dash_t == 0 and ef.vy == 0, "P-M9: a dash that lands ends")
	U.step(es, er, I())
	assert(ef.act == "idle", "P-M9: and is over")
	-- A dash into a wall stops dead there (vx 0) and ends.
	local ws, wr2, wf2 = air(U.yard.solids[1].x - T.body_w / 2 - 60, 100, 0, 0)
	U.step(ws, wr2, I({ x = 1, dash_n = 1 }), 5)
	assert(wf2.x == U.yard.solids[1].x - T.body_w / 2 and wf2.vx == 0 and wf2.wall == 1 and wf2.act == "dash", "P-M9: at the wall")
	U.step(ws, wr2, I())
	assert(wf2.act ~= "dash" and wf2.vy == T.gravity, "P-M9: the dash ended at the wall")
end

-- P-M10: wall slide.
do
	local s, rules, f = at_wall(10)
	run(s, rules, f, 12, I({ x = -1 }), function(tick, f)
		assert(f.vy == T.wall_slide and f.wall == -1 and f.act == "wall", "P-M10: tick " .. tick .. " vy=" .. f.vy .. " act=" .. f.act)
	end)
	-- Not holding toward it: a normal fall.
	local n, nr, nf = at_wall(10)
	run(n, nr, nf, 5, I(), function(tick, f) assert(f.vy > T.wall_slide and f.act == "fall", "P-M10: no hold, vy=" .. f.vy) end)
	-- Holding away is no slide either.
	local a, ar, af = at_wall(10)
	run(a, ar, af, 3, I({ x = 1 }), function(tick, f) assert(f.vy > T.wall_slide, "P-M10: holding away") end)
	-- Rising: not capped.
	local r, rr, rf = at_wall(-8)
	U.step(r, rr, I({ x = -1 }))
	assert(rf.vy == -8 + T.gravity, "P-M10: rising along a wall is not capped")
	-- Not touching: not capped.
	local m, mr, mf = air(750, -400, 0, 10)
	U.step(m, mr, I({ x = -1 }))
	assert(mf.vy == 10 + T.gravity, "P-M10: no wall, no slide")
	-- The ground is not a wall: standing next to nothing never slides (and a grounded fighter doesn't).
end

-- P-M11: wall jump.
do
	local s, rules, f = at_wall(10)
	U.step(s, rules, I({ x = -1 }), 3)
	local x0 = f.x
	U.step(s, rules, I({ x = -1, jump = true, jump_n = 1 }))
	assert(f.vx == T.wall_jump_x and f.vy == -T.wall_jump_y + T.gravity, "P-M11: v=" .. f.vx .. "," .. f.vy)
	assert(f.jumps2 == T.air_jumps and f.wall_jumps == T.wall_jumps - 1 and f.face == 1, "P-M11: double jump kept")
	assert(f.x == x0 + T.wall_jump_x, "P-M11: moved away")
	-- Steering toward the wall is ignored for wall_lock ticks; away works from the first.
	local ref = f.vx
	for n = 1, T.wall_lock do
		U.step(s, rules, I({ x = -1, jump = true, jump_n = 1 }))
		ref = ref - T.air_friction
		assert(f.vx == ref and f.face == 1, "P-M11: locked tick " .. n .. " vx=" .. f.vx .. " face=" .. f.face)
	end
	U.step(s, rules, I({ x = -1, jump = true, jump_n = 1 }))
	assert(f.vx == ref - T.air_accel and f.face == -1, "P-M11: free on tick wall_lock + 1")
	local s2, r2, f2 = at_wall(10)
	U.step(s2, r2, I({ x = -1, jump = true, jump_n = 1 }))
	U.step(s2, r2, I({ x = 1, jump = true, jump_n = 1 }))
	assert(f2.vx == T.wall_jump_x - T.air_accel and f2.face == 1, "P-M11: away is not locked: " .. f2.vx)
	-- Out of wall jumps: the jump is a double jump. Not holding toward it: also.
	local o, orr, of = at_wall(10)
	of.wall_jumps = 0
	U.step(o, orr, I({ x = -1, jump = true, jump_n = 1 }))
	assert(of.act == "jump2" and of.vx ~= T.wall_jump_x and of.jumps2 == 0, "P-M11: no wall jumps left")
	local h, hr, hf = at_wall(10)
	U.step(h, hr, I({ jump = true, jump_n = 1 }))
	assert(hf.act == "jump2" and hf.wall_jumps == T.wall_jumps, "P-M11: not holding toward it")
	-- At most wall_jumps per airtime: three in a row on one wall, set up by hand each time.
	local c, cr, cf = at_wall(10)
	local made = 0
	for n = 1, T.wall_jumps + 2 do
		cf.x, cf.vx, cf.vy, cf.y = WALL_X, 0, 6, 200
		U.step(c, cr, I())
		cf.vy = 6
		U.step(c, cr, I({ x = -1, jump = true, jump_n = n }))
		if cf.vx == T.wall_jump_x then made = made + 1 end
		cf.jumps2 = 0
	end
	assert(made == T.wall_jumps, "P-M11: " .. made .. " wall jumps, not " .. T.wall_jumps)
	-- Landing refreshes them.
	local g, gr, gf = air(750, -50, 0, 0)
	gf.wall_jumps = 0
	run(g, gr, gf, 40, I())
	assert(gf.grounded and gf.wall_jumps == T.wall_jumps, "P-M11: landing refreshes")
end

-- P-M12: the 1200-tick script (demo_apps/brawl/script.lua) uses every move. Run straight; then
-- with copies through the wire at 100, 400 and 777, carrying on from each copy, and a rewind to the
-- 400 copy replayed to the end.
local Script = require("script")
assert(Script.length == 1200)

local function run_from(s, rules, from, to)
	for t = from, to do U.F.step(s, { Script.at(t) }, rules) end
	return s
end

local rules = U.rules()
local straight = run_from(U.F.new(rules, 1), rules, 1, 1200)

local live = run_from(U.F.new(rules, 1), rules, 1, 100)
live = U.copy(live)
run_from(live, rules, 101, 400)
local at400 = U.copy(live)
live = U.copy(live)
run_from(live, rules, 401, 777)
live = U.copy(live)
run_from(live, rules, 778, 1200)
local ok, why = U.same(straight, live)
assert(ok, "P-M12: through copies: " .. tostring(why))
local replay = run_from(U.copy(at400), rules, 401, 1200)
ok, why = U.same(straight, replay)
assert(ok, "P-M12: rewound to 400 and replayed: " .. tostring(why))
ok, why = U.plain(straight)
assert(ok, "P-M12: " .. tostring(why))

-- The script did what it says: every move happened, seen from the states alone.
do
	local used = {}
	local s = U.F.new(rules, 1)
	local f = s.fighters[1]
	local launch = -T.jump_speed + T.gravity
	for t = 1, 1200 do
		local was = U.copy(f)
		U.F.step(s, { Script.at(t) }, rules)
		used[f.act] = true
		if f.grounded and f.act == "dash" then used.ground_dash = true end
		if not f.grounded and f.act == "dash" then used.air_dash = true end
		if f.peak == 107.25 then used.short_hop = true end
		if f.vy == T.fast_fall then used.fast_fall = true end
		if f.wall_jumps < was.wall_jumps then used.wall_jump = true end
		if was.stocks == f.stocks + 1 then used.stock_lost = true end
		if f.vy == launch and f.act == "rise" and was.vy ~= launch then
			if was.grounded and was.buf >= 2 then used.buffered = true end
			if not was.grounded and was.coyote > 0 and f.jumps2 == was.jumps2 then used.coyote = true end
		end
	end
	for _, what in ipairs({ "turn", "jump2", "dash", "wall", "rise", "apex", "fall", "land", "run", "idle", "out", "ground_dash", "air_dash",
		"short_hop", "fast_fall", "wall_jump", "buffered", "coyote", "stock_lost" }) do
		assert(used[what], "P-M12: the script never used " .. what)
	end
end
-- Step 5, the picture's acts (docs/design/brawl-movement.md decision 4, P-A2's source): `apex` is the
-- air between rise and fall, `land` a landing with no direction held for land_ticks (a direction
-- ends it), and a switch keeps the old act and its time in state.
do
	local s, rules = U.fresh()
	local f = s.fighters[1]
	U.step(s, rules, U.input({ jump = true, jump_n = 1 }))
	assert(f.act == "rise" and f.prev_act == "idle" and f.prev_t == 1, "rise from idle")
	local switched = {}
	while not f.grounded do
		local before_act, before_t = f.act, f.act_t
		U.step(s, rules, U.input({ jump = true, jump_n = 1 }))
		if f.act ~= before_act then
			assert(f.prev_act == before_act and f.prev_t == before_t + 1 and f.act_t == 0,
				"a switch keeps the old act and where it would be: " .. before_act .. " -> " .. f.act)
			switched[#switched + 1] = f.act
		end
		if f.act == "apex" then assert(math.abs(f.vy) <= T.apex_vy, "apex vy " .. f.vy) end
		if f.act == "rise" then assert(f.vy < -T.apex_vy) end
		if f.act == "fall" then assert(f.vy > T.apex_vy) end
	end
	assert(table.concat(switched, " ") == "apex fall land", "a full jump: " .. table.concat(switched, " "))
	for t = 1, T.land_ticks - 1 do
		U.step(s, rules, U.input({ jump = true, jump_n = 1 }))
		assert(f.act == "land" and f.act_t == t, "land holds for land_ticks: " .. f.act .. " " .. t)
	end
	U.step(s, rules, U.input({ jump = true, jump_n = 1 }))
	assert(f.act == "idle" and f.prev_act == "land", "then idle: " .. f.act)
	-- landing while holding a direction runs at once; pressing one during land ends it
	local s2, r2 = U.fresh()
	local g = s2.fighters[1]
	U.step(s2, r2, U.input({ jump = true, jump_n = 1 }))
	repeat U.step(s2, r2, U.input({ jump = true, jump_n = 1, x = 1 })) until g.grounded
	assert(g.act == "run", "landing with a direction held: " .. g.act)
	local s3, r3 = U.fresh()
	local h = s3.fighters[1]
	U.step(s3, r3, U.input({ jump = true, jump_n = 1 }))
	repeat U.step(s3, r3, U.input({ jump_n = 1 })) until h.grounded
	assert(h.act == "land")
	U.step(s3, r3, U.input({ jump_n = 1, x = -1 }))
	assert(h.act == "run" and h.prev_act == "land", "a direction ends land: " .. h.act)
end

print("P-M12 final: " .. F.describe(straight.fighters[1], straight.tick))
print("brawl moves ok")
