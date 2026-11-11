-- Brawl's combat in stock Lua (plan: docs/design/brawl-fighting.md §0): P-F1-F13 (the jet's own
-- tests are test_brawl_jet.lua). Fighters are placed by hand; a press is a count, as everywhere.
--   lua scripts/test_brawl_fight.lua
package.path = "scripts/?.lua;demo_apps/brawl/?.lua;" .. package.path
local U = require("brawl_test_util")
local F, T, I = U.F, U.tune, U.input
local M = require("combat").moves

-- Two fighters (human, dummy unless `cast` says), fighter 1 at x1 facing f1, fighter 2 at x2.
-- In the air when y < 0 (at rest), else on the ground.
local function put(f, x, y, face)
	f.x, f.y, f.vx, f.vy = x, y or 0, 0, 0
	f.face = face or f.face
	f.grounded = (y or 0) == 0
	f.on, f.coyote = 0, f.grounded and T.coyote or 0
	f.act, f.act_t = f.grounded and "idle" or "fall", 0
	f.take_y, f.top_y = f.y, f.y
end
local function duel(x1, f1, x2, cast, y1, y2)
	local s, rules = U.fresh(cast or { "human", "dummy" })
	put(s.fighters[1], x1, y1, f1)
	put(s.fighters[2], x2, y2, -f1)
	return s, rules, s.fighters[1], s.fighters[2]
end
local function lunge_by(m, k)
	local d = 0
	if m.lunge then
		for t = 1, k do
			if t >= m.lunge.from and t <= m.lunge.to then d = d + m.lunge.speed end
		end
	end
	return d
end
-- Where a victim stands so the move's first circle sits on its centre line on the first active tick.
local function contact(m)
	local w = m.windows[1]
	return lunge_by(m, w.from) + m.ticks[w.from][1][1]
end

-- P-F5: one hit, by the numbers.
for _, face in ipairs({ 1, -1 }) do
	local s, rules, a, d = duel(0, face, 80 * face)
	U.step(s, rules, I({ light_n = 1 }))
	assert(a.act == "nlight" and a.act_t == 0, "P-F5: nlight started, act " .. a.act)
	U.step(s, rules, I({ light_n = 1 }), 9)
	assert(d.dmg == 0 and d.hitlag == 0, "P-F5: no hit in startup")
	U.step(s, rules, I({ light_n = 1 })) -- tick 11, the first active tick
	assert(d.dmg == 8, "P-F5: dmg " .. d.dmg)
	assert(d.hitlag == 8 and a.hitlag == 8, "P-F5: hitlag " .. d.hitlag .. " / " .. a.hitlag)
	assert(d.hitstun == 11 and d.last_stun == 11, "P-F5: hitstun " .. d.hitstun)
	assert(d.last_kb == 5.5 and d.last_dmg == 8 and d.last_angle == 30 and d.combo == 1, "P-F5: last hit")
	assert(d.kbx == 4.76953125 * face and d.kby == -2.75, "P-F5: launch " .. d.kbx .. ", " .. d.kby)
	local ax, at, dx, dy, dt = a.x, a.act_t, d.x, d.y, d.act_t
	for n = 1, 8 do
		U.step(s, rules, I({ light_n = 1 }))
		assert(a.x == ax and a.act_t == at and d.x == dx and d.y == dy and d.act_t == dt, "P-F5: frozen in hitlag, tick " .. n)
	end
	assert(d.hitlag == 0 and a.hitlag == 0 and d.hitstun == 11, "P-F5: hitlag over, hitstun still whole")
	U.step(s, rules, I({ light_n = 1 }))
	assert(d.x == dx + 4.76953125 * face and d.y == -2.5, "P-F5: the launch moves it: " .. d.x .. ", " .. d.y)
	assert(d.kbx == 5.375 * 0.8671875 * face and d.kby == -5.375 * 0.5 and d.act == "hurt", "P-F5: it decays by kb_decay")
	assert(a.act == "nlight" and a.act_t == at + 1, "P-F5: the attacker goes on")
end

-- P-F2: input picks the move; a side move turns to the held side; a move lasts its total ticks.
do
	local cases = {
		{ true, "light", 0, 0, "nlight" }, { true, "light", 1, 0, "slight" }, { true, "light", 0, 1, "dlight" },
		{ true, "light", 0, -1, "nlight" }, { true, "light", 1, 1, "dlight" },
		{ true, "heavy", 0, 0, "nheavy" }, { true, "heavy", -1, 0, "sheavy" }, { true, "heavy", 0, 1, "dheavy" },
		{ true, "heavy", 0, -1, "nheavy" },
		{ false, "light", 0, 0, "nair" }, { false, "light", -1, 0, "sair" }, { false, "light", 0, 1, "dair" },
		{ false, "light", 0, -1, "nair" }, { false, "heavy", 1, 0, "shair" }, { false, "heavy", 0, 1, "dhair" },
		{ false, "heavy", 0, 0, "jet" }, { false, "heavy", 0, -1, "jet" }, -- the jet (step 4), K not held: it cuts after
	}
	for _, c in ipairs(cases) do
		local ground, kind, x, y, want = c[1], c[2], c[3], c[4], c[5]
		local s, rules = U.fresh()
		local f = s.fighters[1]
		if ground then put(f, 0, 0, 1) else put(f, -800, -900, 1) end
		local press = I({ x = x, y = y, [kind .. "_n"] = 1 })
		U.step(s, rules, press)
		local label = (ground and "ground " or "air ") .. kind .. " " .. x .. "," .. y
		if not want then
			assert(not M[f.act], "P-F2: " .. label .. " starts nothing in step 1, got " .. f.act)
		else
			assert(f.act == want, "P-F2: " .. label .. " is " .. want .. ", got " .. f.act)
			if x ~= 0 and want:sub(1, 1) == "s" then assert(f.face == x, "P-F2: " .. label .. " turns to the held side") end
			local m = M[want]
			local idle = I({ [kind .. "_n"] = 1 })
			for k = 2, m.total do
				U.step(s, rules, idle)
				if f.act ~= want then break end
				assert(f.act_t == k - 1, "P-F2: " .. want .. " act_t")
			end
			if not m.until_land then
				assert(f.act == want and f.act_t == m.total - 1, "P-F2: " .. want .. " lasts " .. m.total .. " ticks, ended at " .. f.act_t .. " " .. f.act)
				U.step(s, rules, idle)
				local next_act = ground and "idle" or want == "jet" and "jet_cut" or "fall"
				assert(f.act == next_act, "P-F2: after " .. want .. " the fighter is " .. next_act .. ", got " .. f.act)
			end
		end
	end
end

-- P-F3: commitment.
do
	-- nothing new starts during a move: no run, jump or dash
	local s, rules = U.fresh()
	local f = s.fighters[1]
	put(f, 0, 0, 1)
	U.step(s, rules, I({ light_n = 1 }))
	for k = 2, 20 do
		U.step(s, rules, I({ light_n = 1, x = 1, jump = true, jump_n = k, dash_n = k, dodge_n = k }))
		assert(f.act == "nlight" and f.vx == 0 and f.vy == 0 and f.y == 0 and f.dash_t == 0, "P-F3: committed, tick " .. k)
	end
	-- a ground move's vx is its lunge table, exactly, even from a run
	s, rules = U.fresh()
	f = s.fighters[1]
	put(f, -400, 0, 1)
	U.step(s, rules, I({ x = 1 }), 10)
	assert(f.vx == T.run_speed)
	local m = M.sheavy
	U.step(s, rules, I({ x = 1, heavy_n = 1 }))
	for k = 1, m.total do
		if k > 1 then U.step(s, rules, I({ x = 1, heavy_n = 1 })) end
		local want = (k >= m.lunge.from and k <= m.lunge.to) and m.lunge.speed or 0
		assert(f.vx == want, "P-F3: sheavy tick " .. k .. " vx " .. f.vx .. ", lunge " .. want)
	end
	-- an aerial keeps air drift (facing kept) and fast fall
	s, rules = U.fresh()
	f = s.fighters[1]
	put(f, -800, -900, 1)
	U.step(s, rules, I({ light_n = 1 }))
	assert(f.act == "nair")
	U.step(s, rules, I({ light_n = 1, x = -1 }), 4)
	assert(f.vx == -4 * T.air_accel and f.face == 1, "P-F3: drift " .. f.vx .. ", face " .. f.face)
	U.step(s, rules, I({ light_n = 1, x = -1, y = 1, down_n = 1 }))
	assert(f.vy == T.fast_fall and f.ff, "P-F3: fast fall in an aerial, vy " .. f.vy)
	-- landing during an aerial ends it and gives lag for land_lag ticks
	s, rules = U.fresh()
	f = s.fighters[1]
	put(f, -800, -30, 1)
	U.step(s, rules, I({ light_n = 1, x = 1 }))
	assert(f.act == "sair")
	local lag = 0
	for _ = 1, 60 do
		U.step(s, rules, I({ light_n = 1 }))
		if f.act == "lag" then lag = lag + 1 elseif lag > 0 then break end
	end
	assert(lag == M.sair.land_lag and f.act == "idle", "P-F3: sair's land lag " .. lag .. " then " .. f.act)
end

-- P-F4: the attack buffer, from a move and from lag.
do
	local m = M.nlight
	for _, early in ipairs({ T.atk_buffer, T.atk_buffer + 1 }) do
		local s, rules = U.fresh()
		local f = s.fighters[1]
		put(f, 0, 0, 1)
		U.step(s, rules, I({ light_n = 1 }))
		local press_tick = m.total - early + 1 -- its move tick: `early` ticks left, this one included
		for k = 2, m.total do U.step(s, rules, I({ light_n = k >= press_tick and 2 or 1 })) end
		assert(f.act == "nlight" and f.act_t == m.total - 1)
		U.step(s, rules, I({ light_n = 2 }))
		if early == T.atk_buffer then
			assert(f.act == "nlight" and f.act_t == 0, "P-F4: a press " .. early .. " ticks before the end starts on the first free tick")
		else
			assert(f.act == "idle", "P-F4: a press " .. early .. " ticks before the end is dropped, act " .. f.act)
		end
	end
	for _, early in ipairs({ T.atk_buffer, T.atk_buffer + 1 }) do
		local s, rules = U.fresh()
		local f = s.fighters[1]
		put(f, -800, -30, 1)
		U.step(s, rules, I({ heavy_n = 1, x = 1 }))
		assert(f.act == "shair")
		while f.act ~= "lag" do U.step(s, rules, I({ heavy_n = 1 })) end
		local left = M.shair.land_lag - 1 -- lag ticks after the landing tick
		local n = 1
		while left > 0 do
			n = 1 + (M.shair.land_lag - left) -- this coming tick is lag tick n
			local remaining = M.shair.land_lag - n + 1
			U.step(s, rules, I({ heavy_n = 1, light_n = remaining <= early and 1 or 0 }))
			left = left - 1
		end
		assert(f.act == "lag")
		U.step(s, rules, I({ heavy_n = 1, light_n = 1 }))
		if early == T.atk_buffer then
			assert(f.act == "nlight", "P-F4: a press in lag's last ticks starts after it, act " .. f.act)
		else
			assert(f.act ~= "nlight", "P-F4: a press earlier in lag is dropped")
		end
	end
end

-- P-F6: once per window; nair's two windows; two victims; a grounded victim's downward angle mirrored.
do
	-- the circle overlaps through the whole window: one hit
	local s, rules, a, d = duel(0, 1, 60)
	U.step(s, rules, I({ light_n = 1 }), M.nlight.total)
	assert(d.dmg == 8, "P-F6: nlight hit once, dmg " .. d.dmg)
	-- nair: two windows, two hits (both fighters in the air, at rest, one place)
	s, rules, a, d = duel(-800, 1, -790, nil, -900, -900)
	U.step(s, rules, I({ light_n = 1 }), M.nair.total)
	assert(d.dmg == 9, "P-F6: nair hit twice, dmg " .. d.dmg .. " combo " .. d.combo)
	-- A true combo (step 6): the first hit's base is 4.5, so its hitstun is floor(4.5 * 2) = 9 ticks and the second
	-- window, 8 move ticks after the first opens, lands inside it. (With base 3 the stun was 6: combo 1.)
	assert(d.combo == 2, "P-F6: nair's second hit lands in the first's hitstun: combo " .. d.combo)
	print("P-F6 nair: two hits, dmg " .. d.dmg .. ", combo counter " .. d.combo)
	-- one move, two victims, once each
	s, rules = U.fresh({ "human", "dummy", "dummy" })
	put(s.fighters[1], 0, 0, 1)
	put(s.fighters[2], 70, 0, -1)
	put(s.fighters[3], 80, 0, -1)
	U.step(s, rules, I({ light_n = 1 }), M.nlight.total)
	assert(s.fighters[2].dmg == 8 and s.fighters[3].dmg == 8, "P-F6: both victims once")
	-- dair on a grounded victim: 300 becomes 60, so it goes up
	s, rules, a, d = duel(0, 1, 0, nil, -100, 0)
	U.step(s, rules, I({ light_n = 1, y = 1 }))
	assert(a.act == "dair")
	U.step(s, rules, I({ light_n = 1 }), 12)
	assert(d.dmg == 8 and d.last_angle == 60 and d.kby < 0, "P-F6: grounded spike mirrored: angle " .. d.last_angle)
	-- and an airborne victim is spiked down
	s, rules, a, d = duel(-800, 1, -800, nil, -900, -800)
	U.step(s, rules, I({ light_n = 1, y = 1 }))
	U.step(s, rules, I({ light_n = 1 }), 12)
	assert(d.dmg == 8 and d.last_angle == 300 and d.kby > 0, "P-F6: airborne spike goes down: angle " .. d.last_angle)
end

-- P-F7: knockback scales; the launch decays to exactly 0; gravity on the own vy; no input in hitstun.
-- One move on a victim at `pct`, standing at x (both in the air for an aerial): how far the launch
-- carried it in x before it ended or the victim rang out.
local function launch(name, pct, vx, cast)
	local m = M[name]
	local vy = m.air and -300 or 0
	local s, rules, a, d = duel(vx - contact(m), 1, vx, cast, vy, vy)
	d.dmg = pct
	local key = (name:find("heavy") or name:find("hair") or name == "jet") and "heavy_n" or "light_n"
	U.step(s, rules, { I({ [key] = 1, x = name:sub(1, 1) == "s" and 1 or 0, y = name:sub(1, 1) == "d" and 1 or 0 }) })
	assert(a.act == name, "launch: " .. name .. " started as " .. a.act)
	local x0, far, out = nil, 0, false
	for _ = 1, 900 do
		U.step(s, rules, { I({ [key] = 1 }) })
		if d.act == "out" then out = true; break end
		if d.dmg > pct then
			x0 = x0 or d.x
			far = math.max(far, d.x - x0)
			if d.kb == 0 and d.hitstun == 0 and d.hitlag == 0 then break end
		end
	end
	return { hit = x0 ~= nil, travel = far, out = out }
end
do
	local last = -1
	for _, pct in ipairs({ 0, 50, 100, 150 }) do
		local r = launch("sheavy", pct, -400, { "human", "human" })
		assert(r.hit, "P-F7: sheavy connects at " .. pct)
		assert(r.travel > last, "P-F7: travel at " .. pct .. "% is " .. r.travel .. ", not more than " .. last)
		last = r.travel
	end
	-- the launch decays to exactly 0, never past; gravity on the own vy; no input in hitstun, then input
	local s, rules, a, d = duel(-200 - contact(M.sheavy), 1, -200, { "human", "human" })
	d.dmg = 60
	U.step(s, rules, { I({ heavy_n = 1, x = 1 }) })
	while d.hitlag == 0 do U.step(s, rules, { I({ heavy_n = 1 }) }) end
	while d.hitlag > 0 do U.step(s, rules, { I({ heavy_n = 1 }) }) end
	local kb, stun, acted = d.kb, d.hitstun, false
	assert(kb > 0 and stun > 0)
	for t = 1, 400 do
		local vy = d.vy
		U.step(s, rules, { I({ heavy_n = 1 }), I({ x = -1, jump = true, jump_n = t, light_n = t }) })
		local want = kb - T.kb_decay > 0 and kb - T.kb_decay or 0
		assert(d.kb == want, "P-F7: kb " .. d.kb .. ", want " .. want)
		kb = d.kb
		if d.act == "hurt" then
			assert(d.vx == 0 and d.seen.jump == t, "P-F7: input acted in hitstun: vx " .. d.vx)
			if not d.grounded then assert(d.vy == math.min(vy + T.gravity, T.max_fall), "P-F7: gravity on the own vy") end
		elseif d.kb > 0 and not acted then
			acted = d.vx < 0
		end
		if kb == 0 then
			U.step(s, rules, { I({ heavy_n = 1 }), I({ x = -1 }) })
			assert(d.kb == 0 and d.kbx == 0 and d.kby == 0, "P-F7: stays at 0")
			break
		end
	end
	assert(acted, "P-F7: after hitstun, input acts while the launch decays")
	-- the bounds, and the KO table. A KO here is the launch itself carrying the box out (blasted while
	-- kb > 0 or in hitstun); a dummy left to fall off the stage afterwards is not counted.
	local function kos(name, pct, x)
		local m = M[name]
		local vy = m.air and -300 or 0
		local s2, r2, a2, d2 = duel(x - contact(m), 1, x, nil, vy, vy)
		d2.dmg = pct
		local key = (name:find("heavy") or name:find("hair") or name == "jet") and "heavy_n" or "light_n"
		local held = I({ [key] = 1, x = name:sub(1, 1) == "s" and 1 or 0, y = name:sub(1, 1) == "d" and 1 or 0 })
		local hit = false
		for _ = 1, 900 do
			U.step(s2, r2, held)
			if d2.last_dmg > 0 then hit = true end -- (not dmg > pct: at 999 the cap holds it)
			if hit and d2.act == "out" then return true, true end
			if hit and d2.kb == 0 and d2.hitstun == 0 and d2.hitlag == 0 then return false, true end
		end
		return false, hit
	end
	assert(kos("sheavy", 150, 800), "P-F7: sheavy from x = 800 KOs at 150%")
	assert(not kos("sheavy", 0, 800), "P-F7: sheavy from x = 800 does not KO at 0%")
	assert(not kos("nlight", 150, 0), "P-F7: nlight from the centre does not KO at 150%")
	local function ko_pct(name, x)
		local _, hit = kos(name, 0, x)
		if not hit then return "no contact" end
		if not kos(name, 999, x) then return "never" end
		local lo, hi = 0, 999
		if kos(name, 0, x) then return 0 end
		while hi - lo > 1 do
			local mid = (lo + hi) // 2
			if kos(name, mid, x) then hi = mid else lo = mid end
		end
		return hi
	end
	local names = {}
	for name in pairs(M) do names[#names + 1] = name end
	table.sort(names)
	local rows = {}
	for _, name in ipairs(names) do rows[#rows + 1] = name .. " " .. ko_pct(name, 0) .. "/" .. ko_pct(name, 800) end
	print("P-F7 KO% (victim at x 0 / x 800, launch only): " .. table.concat(rows, ", "))
	local lo, hi = launch("sheavy", 0, 800, { "human", "human" }), launch("sheavy", 150, -800, { "human", "human" })
	print(string.format("P-F7 sheavy launch travel in x: %.17g at 0%%; %.17g at 150%% (from x -800, out: %s)", lo.travel, hi.travel, tostring(hi.out)))
end

-- P-F8: trades. Both moves land on one tick; swapping the indices mirrors it.
do
	local function trade(swap)
		local s, rules = U.fresh({ "human", "human" })
		local p, q = s.fighters[1], s.fighters[2]
		if swap then p, q = q, p end
		put(p, 0, 0, 1)
		put(q, 80, 0, -1)
		U.step(s, rules, { I({ light_n = 1 }), I({ light_n = 1 }) }, 11)
		return p, q
	end
	local p, q = trade(false)
	assert(p.dmg == 8 and q.dmg == 8, "P-F8: both land")
	assert(p.kbx == -4.76953125 and q.kbx == 4.76953125 and p.hitlag == 8 and q.hitlag == 8, "P-F8: each launched away")
	assert(p.act == "hurt" and q.act == "hurt", "P-F8: both moves cancelled")
	local p2, q2 = trade(true)
	for _, k in ipairs({ "dmg", "kbx", "kby", "hitlag", "hitstun", "x", "y", "act", "combo" }) do
		assert(p2[k] == p[k] and q2[k] == q[k], "P-F8: swapped indices differ in " .. k)
	end
end

-- P-F10: ring-out and the match.
do
	-- the dummy: unlimited stocks, counts outs, respawns invulnerable with dmg 0
	local s, rules, a, d = duel(0, 1, 0)
	assert(d.stocks == -1 and d.ctl == "dummy", "P-F10: the dummy has stocks = -1")
	d.dmg = 50
	d.x = 2200
	U.step(s, rules)
	assert(d.act == "out" and d.outs == 1 and d.stocks == -1 and d.dmg == 0, "P-F10: the dummy rang out")
	U.step(s, rules, nil, T.respawn_ticks)
	assert(d.act ~= "out" and d.inv == T.respawn_inv and d.x == U.yard.respawn.x, "P-F10: respawned invulnerable, inv " .. d.inv)
	-- invulnerable: a hit does nothing
	put(a, d.x - 60, d.y < 0 and d.y or 0, 1)
	put(d, d.x, a.y, -1)
	d.inv = 100
	U.step(s, rules, I({ light_n = 1 }), 20)
	assert(d.dmg == 0 and d.hitlag == 0 and a.hitlag == 0, "P-F10: no hit while invulnerable")
	-- two humans: the last with stocks wins, Enter restarts everyone
	s, rules = U.fresh({ "human", "human" })
	local p, q = s.fighters[1], s.fighters[2]
	q.stocks = 1
	q.x = -2200
	U.step(s, rules, { I(), I() })
	assert(q.act == "ko" and q.stocks == 0 and s.winner == 1, "P-F10: the winner is fighter 1, winner " .. s.winner)
	U.step(s, rules, { I({ restart_n = 1 }), I() })
	assert(s.winner == 0 and s.fighters[2].stocks == T.stocks and s.fighters[2].act == "idle", "P-F10: Enter restarts everyone")
	-- against the dummy the human losing its last stock makes the dummy the winner
	s, rules = U.fresh()
	s, rules = U.fresh({ "human", "dummy" })
	p = s.fighters[1]
	p.stocks = 1
	p.x = 2200
	U.step(s, rules)
	assert(p.act == "ko" and s.winner == 2, "P-F10: the dummy wins")
	U.step(s, rules, I({ restart_n = 1 }))
	assert(s.winner == 0 and s.fighters[1].stocks == T.stocks, "P-F10: restart")
	-- alone: ko, no winner (P-B6 as before)
	s, rules = U.fresh()
	p = s.fighters[1]
	p.stocks = 1
	p.x = 2200
	U.step(s, rules)
	assert(p.act == "ko" and s.winner == 0, "P-F10: alone there is no winner")
end

-- P-F11: the dummy takes no input; it is launched, stunned, falls, lands, idles, never attacks.
do
	local s, rules, a, d = duel(0, 1, 80)
	local acts = {}
	for t = 1, 400 do
		U.step(s, rules, { I({ light_n = 1 }), I({ light_n = t, heavy_n = t, x = 1, jump = true, jump_n = t }) })
		acts[d.act] = true
		assert(not M[d.act], "P-F11: the dummy attacked")
	end
	assert(acts.hurt and d.act == "idle" and d.grounded and d.x > 80, "P-F11: launched, stunned, landed, idle")
	assert(d.last_dmg == 8 and d.last_kb == 5.5 and d.last_angle == 30 and d.last_stun == 11 and d.combo == 1, "P-F11: last_* as P-F5")
end

-- P-F9: dodges. Spot: invulnerable on dodge ticks 3-22; directional: 8/tick for 16 ticks (128), then
-- x dodge_keep, invulnerable 3-20; ground cooldown dodge_cd between starts; one air dodge per airtime,
-- refreshed by landing and by a hit; no gravity on the invulnerable ticks in the air.
do
	local function run(s, rules, input, n, each)
		for k = 1, n do
			U.step(s, rules, input)
			if each then each(k) end
		end
	end
	-- spot, on the ground: the invulnerable window and the length, tick by tick
	local s, rules, a = duel(0, 1, 400)
	local x0 = a.x
	local seen = {}
	for k = 1, 34 do
		U.step(s, rules, I({ dodge_n = 1 }))
		seen[k] = { a.act, a.inv > 0, a.act_t, a.x }
	end
	for k = 1, 34 do
		local want = k >= 3 and k <= 22
		assert(seen[k][2] == want, "P-F9: spot inv on dodge tick " .. k .. ": " .. tostring(seen[k][2]))
		if k <= 30 then
			assert(seen[k][1] == "dodge_spot" and seen[k][3] == k - 1, "P-F9: spot tick " .. k .. " act " .. seen[k][1] .. " " .. seen[k][3])
		else
			assert(seen[k][1] == "idle", "P-F9: a spot dodge lasts exactly 30 ticks, tick " .. k .. " is " .. seen[k][1])
		end
		assert(seen[k][4] == x0, "P-F9: a spot dodge stays put")
	end
	assert(a.dcd > 0 and a.dcd == T.dodge_cd - 33, "P-F9: cooldown counts from the start, " .. a.dcd)

	-- a hit on the invulnerable ticks: nothing (no damage, no hitlag, the window is not used up); on
	-- tick 2 or tick 23 it lands and ends the dodge. The victim dodges from tick `start`; the
	-- attacker's nlight press is at tick `press` and is live 10 ticks later for 6.
	local function hit_during(start, press)
		local s2, r2, atk, vic = duel(0, 1, 80, { "human", "human" })
		local log = {}
		for t = 1, 40 do
			U.step(s2, r2, { I({ light_n = t >= press and 1 or 0 }), I({ dodge_n = t >= start and 1 or 0 }) })
			log[t] = { vic.dmg, vic.hitlag, atk.hitlag, vic.act }
		end
		return log, vic
	end
	local log = hit_during(1, 1) -- live on ticks 11-16 = dodge ticks 11-16
	for t = 1, 40 do assert(log[t][1] == 0 and log[t][2] == 0 and log[t][3] == 0, "P-F9: a hit on an invulnerable tick does nothing, tick " .. t) end
	log = hit_during(1, 12) -- live from tick 22 = dodge tick 22 (inv), tick 23 is not
	assert(log[22][1] == 0 and log[22][2] == 0, "P-F9: dodge tick 22 is invulnerable")
	assert(log[23][1] == 8 and log[23][2] == 8 and log[23][3] == 8 and log[23][4] == "hurt", "P-F9: dodge tick 23 is hit: " .. log[23][1] .. " " .. log[23][4])
	log = hit_during(10, 1) -- live from tick 11 = dodge tick 2
	assert(log[11][1] == 8 and log[11][4] == "hurt", "P-F9: dodge tick 2 is hit")
	log = hit_during(9, 1) -- tick 11 = dodge tick 3
	for t = 11, 16 do assert(log[t][1] == 0, "P-F9: dodge tick 3 on is invulnerable, tick " .. t) end
	local _, vic = hit_during(10, 1)
	assert(vic.dodge == "" and vic.dk == 0, "P-F9: a hit ends the dodge")

	-- the cooldown: the next ground dodge starts dodge_cd ticks after this one started, not before
	for _, gap in ipairs({ T.dodge_cd - 1, T.dodge_cd }) do
		local s3, r3, f = duel(0, 1, 400)
		U.step(s3, r3, I({ dodge_n = 1 }))
		run(s3, r3, I({ dodge_n = 1 }), gap - 1)
		U.step(s3, r3, I({ dodge_n = 2 })) -- tick gap + 1
		if gap == T.dodge_cd then
			assert(f.act == "dodge_spot" and f.act_t == 0, "P-F9: a dodge dodge_cd ticks after the first starts, act " .. f.act)
		else
			assert(f.act == "idle", "P-F9: one tick too soon is dropped, act " .. f.act)
		end
	end

	-- directional: 8 a tick for 16 ticks = exactly 128, then x dodge_keep; face unchanged; 32 ticks
	for _, dir in ipairs({ 1, -1 }) do
		local s4, r4, f = duel(0, 1, 800 * dir)
		f.x = 0
		local inv = {}
		for k = 1, 34 do
			U.step(s4, r4, I({ dodge_n = 1, x = dir }))
			inv[k] = f.inv > 0
			if k <= 16 then
				assert(f.act == "dodge_dir" and f.vx == dir * 8 and f.x == dir * 8 * k, "P-F9: dodge tick " .. k .. " x " .. f.x)
			end
			if k == 16 then assert(f.x == dir * 128, "P-F9: exactly 128") end
			if k == 17 then assert(f.vx == dir * 8 * T.dodge_keep and f.x == dir * 130, "P-F9: x dodge_keep after, vx " .. f.vx) end
			if k <= 32 then assert(f.act == "dodge_dir" and f.face == 1, "P-F9: dodge_dir lasts 32 ticks, face unchanged, tick " .. k .. " " .. f.act) end
			if k == 32 then assert(f.x == dir * 131, "P-F9: friction takes the last 3 (2 + 1): " .. f.x) end
			if k > 32 then assert(f.act ~= "dodge_dir", "P-F9: over after 32") end
		end
		for k = 1, 34 do assert(inv[k] == (k >= 3 and k <= 20), "P-F9: directional inv on tick " .. k) end
	end

	-- commitment: nothing starts during a dodge; an attack press in the last atk_buffer ticks starts after
	for _, press in ipairs({ 22, 23 }) do
		local s5, r5, f = duel(0, 1, 400)
		for k = 1, 29 do
			U.step(s5, r5, I({ dodge_n = 1, jump = true, jump_n = k, dash_n = k, light_n = k >= press and 1 or 0, x = 0 }))
			assert(f.act == "dodge_spot" and f.grounded, "P-F9: jump and dash don't start in a dodge, tick " .. k .. " " .. f.act)
		end
		U.step(s5, r5, I({ dodge_n = 1, jump = true, jump_n = 29, dash_n = 29, light_n = 1 })) -- dodge tick 30
		U.step(s5, r5, I({ dodge_n = 1, jump = true, jump_n = 29, dash_n = 29, light_n = 1 }))
		if press == 23 then
			assert(f.act == "nlight" and f.act_t == 0, "P-F9: a buffered press starts after the dodge, act " .. f.act)
		else
			assert(f.act == "idle", "P-F9: a press before the buffer is dropped, act " .. f.act)
		end
	end
	do -- and a dodge press during a move is dropped
		local s5, r5, f = duel(0, 1, 400)
		U.step(s5, r5, I({ light_n = 1 }))
		run(s5, r5, I({ light_n = 1, dodge_n = 1 }), 10)
		assert(f.act == "nlight", "P-F9: no dodge out of a move")
	end

	-- the air: one air dodge per airtime; no gravity on the invulnerable ticks; landing refreshes it
	local s6, r6, f = duel(-800, 1, -400, nil, -500, -500)
	local ys, vys = {}, {}
	for k = 1, 32 do
		U.step(s6, r6, I({ dodge_n = 1 }))
		ys[k], vys[k] = f.y, f.vy
	end
	assert(f.adodges == 0, "P-F9: the air dodge is spent")
	for k = 4, 22 do assert(ys[k] == ys[3] and vys[k] == 0, "P-F9: no gravity on the invulnerable ticks, tick " .. k) end
	assert(ys[1] > -500 and ys[23] > ys[22], "P-F9: gravity before and after the invulnerable ticks")
	assert(f.dcd == 0, "P-F9: an air dodge starts no cooldown")
	f.act, f.act_t = "fall", 0
	for k = 1, 3 do U.step(s6, r6, I({ dodge_n = 1 + k })) end
	assert(f.act ~= "dodge_spot" and f.act ~= "dodge_dir", "P-F9: one air dodge per airtime, act " .. f.act)
	-- a landing refreshes it
	f.y, f.vy = -4, 6
	U.step(s6, r6, I({ dodge_n = 1 }))
	run(s6, r6, I({ dodge_n = 1 }), 3)
	assert(f.grounded and f.adodges == T.air_dodges, "P-F9: landing refreshes the air dodge")
	-- a hit refreshes it too
	local s7, r7, atk, vic = duel(-800, 1, -720, { "human", "human" }, -500, -500)
	vic.adodges = 0
	for t = 1, 30 do
		U.step(s7, r7, { I({ light_n = 1 }), I() })
		if vic.dmg > 0 then break end
	end
	assert(vic.dmg > 0 and vic.adodges == T.air_dodges, "P-F9: a hit refreshes the air dodge")
	run(s7, r7, { I({ light_n = 1 }), I({ dodge_n = 1 }) }, 200)
	-- (hitstun first: no dodge in it; then the air dodge answers)
	-- the directional air dodge: 128 across, vy 0 on its invulnerable ticks 3-20
	local s8, r8, g = duel(-800, 1, -400, nil, -500, -500)
	local gy = {}
	for k = 1, 17 do
		U.step(s8, r8, I({ dodge_n = 1, x = 1 }))
		gy[k] = g.y
		if k == 16 then assert(g.x == -800 + 128, "P-F9: air directional goes 128, x " .. g.x) end
	end
	for k = 4, 17 do assert(gy[k] == gy[3], "P-F9: air directional: no gravity, tick " .. k) end
	-- a dodge doesn't touch the dash (P-M9 holds on its own): a dash press still dashes
	local s9, r9, h = duel(0, 1, 400)
	U.step(s9, r9, I({ dash_n = 1 }))
	assert(h.act == "dash" and h.vx == T.dash_speed, "P-F9: the dash is untouched")
	-- plain data, and a copy stepped alike stays equal
	local s10, r10, p = duel(0, 1, 400)
	U.step(s10, r10, I({ dodge_n = 1, x = 1 }), 5)
	local ok, why = U.plain(s10)
	assert(ok, "P-F9: " .. tostring(why))
	local c1, c2 = U.copy(s10), U.copy(s10)
	U.step(c1, r10, I({ dodge_n = 1, x = 1 }), 40)
	U.step(c2, r10, I({ dodge_n = 1, x = 1 }), 40)
	ok, why = U.same(c1, c2)
	assert(ok, "P-F9: copies: " .. tostring(why))
end

-- P-F12: movement is unchanged with a dummy at spawn 2; fighters pass through each other.
local Script = require("script")
local WANT = "tick=1200 x=-759.5 y=-226 vx=-7 vy=4 act=fall act_t=7 face=-1 jumps2=1 dashes=1 wall_jumps=3 stocks=2 peak=0 wall=0 drop=0"
do
	for _, cast in ipairs({ { "human" }, { "human", "dummy" } }) do
		local rules = U.rules(cast)
		local s = F.new(rules)
		assert(#s.fighters == #cast)
		if cast[2] then assert(s.fighters[2].x == U.yard.spawns[2].x, "P-F12: the dummy stands at spawn 2") end
		for t = 1, 1200 do F.step(s, { Script.at(t) }, rules) end
		local line = F.describe(s.fighters[1], s.tick)
		assert(line == WANT, "P-F12: with " .. #cast .. " fighters: " .. line)
	end
end

-- P-F13 and P-F1: the fight script against the dummy, straight, through wire copies at 300, 777 and
-- 1234, and rewound to 777; at every 50th tick two copies stepped with one input stay equal.
do
	local Fight = require("script_fight")
	assert(Fight.length == 1800)
	local rules = U.rules({ "human", "dummy" })
	local function run_from(s, from, to, each)
		for t = from, to do
			F.step(s, { Fight.at(t) }, rules)
			if each then each(t, s) end
		end
		return s
	end
	local started, hits, prev_dmg, dodges, seen_acts = {}, 0, 0, {}, {}
	local straight = run_from(F.new(rules), 1, 1800, function(t, s)
		local f, d = s.fighters[1], s.fighters[2]
		if M[f.act] and f.act_t == 0 then started[f.act] = (started[f.act] or 0) + 1 end
		seen_acts[f.act] = true
		if f.dk == 1 then
			local key = f.dodge .. (f.grounded and "" or " (air)")
			dodges[key] = (dodges[key] or 0) + 1
		end
		if d.dmg > prev_dmg then hits = hits + 1 end
		prev_dmg = d.dmg
		if t % 50 == 0 then
			local ok, why = U.plain(s)
			assert(ok, "P-F1: tick " .. t .. ": " .. tostring(why))
			local c1, c2 = U.copy(s), U.copy(s)
			F.step(c1, { Fight.at(t + 1) }, rules)
			F.step(c2, { Fight.at(t + 1) }, rules)
			ok, why = U.same(c1, c2)
			assert(ok, "P-F1: tick " .. t .. ": " .. tostring(why))
		end
	end)
	local live = U.copy(run_from(F.new(rules), 1, 300))
	run_from(live, 301, 777)
	local at777 = U.copy(live)
	live = U.copy(live)
	run_from(live, 778, 1234)
	live = U.copy(live)
	run_from(live, 1235, 1800)
	local ok, why = U.same(straight, live)
	assert(ok, "P-F13: through copies: " .. tostring(why))
	ok, why = U.same(straight, run_from(U.copy(at777), 778, 1800))
	assert(ok, "P-F13: rewound to 777: " .. tostring(why))
	local missing, used = {}, {}
	for name in pairs(M) do
		if not started[name] then missing[#missing + 1] = name else used[#used + 1] = name .. "x" .. started[name] end
	end
	table.sort(missing)
	table.sort(used)
	assert(#missing == 0, "P-F13: the fight script never started " .. table.concat(missing, " "))
	local ds = {} for k, v in pairs(dodges) do ds[#ds + 1] = k .. "x" .. v end table.sort(ds)
	assert(dodges.spot and dodges.dir and dodges["spot (air)"], "P-F13: dodges: " .. table.concat(ds, ", "))
	for _, act in ipairs({ "jet_hover", "jet_thrust", "jet_cut" }) do
		assert(seen_acts[act], "P-F13: the fight script never reached " .. act)
	end
	assert(hits >= 9, "P-F13: only " .. hits .. " hits landed")
	print("P-F13 moves: " .. table.concat(used, " ") .. "; hits on the dummy " .. hits .. ", its outs " .. straight.fighters[2].outs)
	for i, f in ipairs(straight.fighters) do print("P-F13 final " .. i .. ": " .. F.describe_fight(f)) end
end

print("brawl fight ok")
