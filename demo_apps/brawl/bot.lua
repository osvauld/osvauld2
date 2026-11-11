-- The CPU (docs/design/brawl-fighting.md decision 10, P-CPU1-5). Pure, plain state, stock Lua 5.4 and
-- Luau alike, no trig, `^`, `pairs` or clock (P-H5). A bot is a fighter with `ctl = "bot"`; F.step asks
-- `B.input(s, i, rules)` for its input before anyone moves, and the answer is an ordinary input record.
-- It reads state only: its own fighter now, the others as they were `bot_react` ticks ago (the ring).
--
-- Memory, `f.bot`: `seed` (MINSTD), `ring`/`rk` (what it saw, oldest at `rk`), `think` (ticks to the next
-- plan), `goal` (the x it walks to), `face_to`, `hold` (ticks the jump stays held), `judged` (the last
-- attack it decided about), `plan` (a name, for reading). Presses are the fighter's own `seen` counts + 1,
-- so they go on growing across a restart.
local B = {}

local MOD = 2147483647
local GROUND = { "nlight", "slight", "dlight", "nheavy", "sheavy", "dheavy" }
local AIR = { "nair", "sair", "dair", "shair", "dhair" }
local HEAVY = { nheavy = true, sheavy = true, dheavy = true, shair = true, dhair = true }
local KEYS = { "jump", "down", "dash", "light", "heavy", "dodge" }

local function sign(v)
	if v > 0 then return 1 end
	if v < 0 then return -1 end
	return 0
end

local function approach(v, target, by)
	if v < target then return math.min(v + by, target) end
	if v > target then return math.max(v - by, target) end
	return v
end

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- MINSTD: seed × 48271 mod 2³¹ − 1, exact in doubles (the product is under 2⁴⁷).
function B.draw(b)
	b.seed = b.seed * 48271 % MOD
	return b.seed
end

-- The stage as the bot reads it, derived once in F.rules: every top it can stand on (`spans`, solids'
-- and platforms' as { x0, x1, y }) and `ground`, the widest run of tops that touch at one height.
function B.stage(stage)
	local spans = {}
	for _, s in ipairs(stage.solids) do spans[#spans + 1] = { x0 = s.x, x1 = s.x + s.w, y = s.y } end
	for _, p in ipairs(stage.platforms) do spans[#spans + 1] = { x0 = p.x, x1 = p.x + p.w, y = p.y } end
	local runs = {}
	for _, s in ipairs(spans) do runs[#runs + 1] = { x0 = s.x0, x1 = s.x1, y = s.y } end
	local merged = true
	while merged do
		merged = false
		for a = 1, #runs do
			for c = a + 1, #runs do
				local r, q = runs[a], runs[c]
				if not merged and r.y == q.y and r.x0 <= q.x1 and q.x0 <= r.x1 then
					r.x0, r.x1 = math.min(r.x0, q.x0), math.max(r.x1, q.x1)
					table.remove(runs, c)
					merged = true
				end
			end
		end
	end
	local ground = runs[1]
	for _, r in ipairs(runs) do
		if r.x1 - r.x0 > ground.x1 - ground.x0 or (r.x1 - r.x0 == ground.x1 - ground.x0 and r.y > ground.y) then ground = r end
	end
	return spans, ground
end

-- A new bot's memory. Each index draws from its own seed, from the setup's.
function B.new(rules, i, f)
	local seed = (rules.seed + i * 7919) % (MOD - 1) + 1
	return { seed = seed, ring = {}, rk = 1, think = 0, goal = f.x, face_to = f.face, hold = 0, judged = -1, plan = "" }
end

local function in_play(g) return g.act ~= "out" and g.act ~= "ko" end

local function snap(dst, g, j)
	dst.who, dst.x, dst.y, dst.face, dst.act, dst.move, dst.mk = j, g.x, g.y, g.face, g.act, g.move, g.mk
	dst.vx, dst.vy = g.vx + g.kbx, g.vy + g.kby
	dst.dmg, dst.inv, dst.grounded = g.dmg, g.inv, g.grounded
	return dst
end

-- The others now, written over the ring's oldest slot (in place: no new tables once it is full).
local function remember(b, s, i, react)
	local function write(slot)
		local n = 0
		for j, g in ipairs(s.fighters) do
			if j ~= i then
				n = n + 1
				slot[n] = snap(slot[n] or {}, g, j)
			end
		end
		for k = #slot, n + 1, -1 do slot[k] = nil end
	end
	if #b.ring ~= react then
		b.ring, b.rk = {}, 1
		for k = 1, react do
			b.ring[k] = {}
			write(b.ring[k])
		end
	end
	return write
end

-- Where a move can reach, from the feet facing right: its baked box, pushed forward by its lunge.
local function lunge_by(m, k)
	local l = m.lunge
	if not l then return 0 end
	local from, to = l.from, math.min(l.to, k)
	if to < from then return 0 end
	return (to - from + 1) * l.speed
end

-- Would move `m`, started now by a fighter at (x, y) facing `face`, reach a box with feet at (tx, ty)?
local function reaches(m, x, y, face, tx, ty, T)
	local r = m.reach
	local hw, h = T.body_w / 2, T.body_h
	local w1 = m.windows[#m.windows]
	local rx = (tx - x) * face
	local x0, x1 = r[1] + lunge_by(m, m.windows[1].from), r[3] + lunge_by(m, w1.to)
	if not (rx + hw > x0 and rx - hw < x1) then return false end
	local top, bottom = ty - h - y, ty - y
	return bottom > r[2] and top < r[4]
end

-- Is there a top under the fighter's feet (or at them)?
local function safe(f, rules)
	local hw = rules.tune.body_w / 2
	for _, sp in ipairs(rules.spans) do
		if f.x > sp.x0 - hw and f.x < sp.x1 + hw and sp.y >= f.y then return true end
	end
	return false
end

-- The span the fighter stands on, if any.
local function under(f, rules)
	local hw = rules.tune.body_w / 2
	for _, sp in ipairs(rules.spans) do
		if f.y == sp.y and f.x > sp.x0 - hw and f.x < sp.x1 + hw then return sp end
	end
	return nil
end

-- Drifting toward `tx` from here, with nothing spent: does it come down onto the ground's top?
local function makes_it(x, y, vx, vy, tx, rules)
	local T, g = rules.tune, rules.ground
	local hw = T.body_w / 2
	local side = sign(tx - x)
	for _ = 1, 240 do
		vx = approach(vx, side * T.run_speed, T.air_accel)
		vy = math.min(vy + T.gravity, T.max_fall)
		x, y = x + vx, y + vy
		local over = x > g.x0 - hw and x < g.x1 + hw
		if over then return y <= g.y + vy end
		if y > g.y then return false end
	end
	return false
end

local function free(f)
	return f.hitlag == 0 and f.hitstun == 0 and f.move == "" and f.dodge == "" and f.lag == 0
end

local function press(out, f, key) out[key .. "_n"] = f.seen[key] + 1 end

-- Offstage: back toward the ground. Nothing is spent while a drift makes it; else, in this order, a
-- wall jump off the stage's side (even out of a burning jet), the double jump, the jet (ignited
-- neutral, then held with Up and the side, let go once a drift makes it), the air dash up and in (not
-- on a wall, where it would refresh), the air dodge.
local function recover(out, f, b, rules)
	local T, g = rules.tune, rules.ground
	local hw = T.body_w / 2
	local below = f.x > g.x0 - hw and f.x < g.x1 + hw and f.y > g.y -- under the stage: out from under it first
	local tx = clamp(f.x, g.x0 + hw + 24, g.x1 - hw - 24)
	if below then tx = f.x - g.x0 < g.x1 - f.x and g.x0 - hw - 40 or g.x1 + hw + 40 end
	local side = sign(tx - f.x)
	local wall = side ~= 0 and f.wall == side and f.wall_jumps > 0 and not below
	out.x = side
	b.plan = "recover"
	if f.jet == "on" and not wall then
		local vy = f.vy > 0 and f.vy or 0
		if below or not makes_it(f.x, f.y, f.vx, vy, tx, rules) then
			out.heavy, out.y = true, below and 0 or -1
		end
	elseif f.move == "jet" then
		out.heavy, out.y = true, below and 0 or -1
	elseif not (free(f) or f.jet == "on") then
		-- committed or stunned: only the drift
	elseif not below and makes_it(f.x, f.y, f.vx, f.vy, tx, rules) then
		-- drift in
	elseif wall then
		press(out, f, "jump")
		out.jump = true
	elseif f.jumps2 > 0 and f.vy >= -2 and not below then
		press(out, f, "jump")
		out.jump, b.hold = true, 20
	elseif f.fuel >= T.jet_cost and f.vy >= -2 then
		press(out, f, "heavy")
		out.heavy, out.x, out.y = true, 0, -1 -- neutral: a side would be shair
	elseif f.dashes > 0 and f.wall == 0 and f.dash_t == 0 and not below and f.vy >= 0 then
		press(out, f, "dash")
		out.y = -1
	elseif f.adodges > 0 and f.fuel < T.jet_cost and side ~= 0 then
		press(out, f, "dodge")
	end
end

-- An attack it sees coming (seen `react` ticks late) that is still live by now and reaches it: dodge
-- by the odds, decided once per attack.
local function threat(out, f, b, s, rules, seen)
	local T = rules.tune
	if not free(f) then return false end
	local can = f.grounded and f.dcd == 0 or not f.grounded and f.adodges > 0
	if not can then return false end
	for _, o in ipairs(seen) do
		if o.move ~= "" and in_play(o) then
			local m = rules.moves[o.move]
			local key = (s.tick - o.mk) * 16 + o.who
			if b.judged ~= key and o.mk + T.bot_react <= m.windows[#m.windows].to and reaches(m, o.x, o.y, o.face, f.x, f.y, T) then
				b.judged = key
				if B.draw(b) % 1024 < T.bot_dodge * 1024 then
					press(out, f, "dodge")
					b.plan = "dodge"
					return true
				end
			end
		end
	end
	return false
end

local function pick(list, b) return list[B.draw(b) % #list + 1] end

-- The moves that reach the target now, light and heavy apart. A ground move whose lunge would carry it
-- off the top it stands on is left out; so is any aerial with nothing under it.
local function fitting(f, t, side, rules)
	local T = rules.tune
	local lights, heavies = {}, {}
	local sp = f.grounded and under(f, rules) or nil
	local hw = T.body_w / 2
	for _, name in ipairs(f.grounded and GROUND or AIR) do
		local m = rules.moves[name]
		local face = name:sub(1, 1) == "s" and side or f.face
		local ok = reaches(m, f.x, f.y, face, t.x, t.y, T)
		if ok and sp and m.lunge then
			local to = f.x + face * lunge_by(m, m.total)
			ok = to > sp.x0 + hw and to < sp.x1 - hw
		end
		if ok and not f.grounded and not safe(f, rules) then ok = false end
		if ok then
			local list = HEAVY[name] and heavies or lights
			list[#list + 1] = name
		end
	end
	return lights, heavies
end

local function attack(out, f, name, side)
	press(out, f, HEAVY[name] and "heavy" or "light")
	local c = name:sub(1, 1)
	if c == "s" then out.x = side elseif c == "d" then out.y = 1 end
end

-- Every `bot_think` ticks: with a target not seen invulnerable, a move that reaches it now (a heavy
-- past kill_pct, before that one time in four, else a light), else a place `spacing` × ½, ¾ or 1 from
-- it; with none, wander.
local function think(out, f, b, rules, t)
	local T, g = rules.tune, rules.ground
	local lo, hi = g.x0 + T.edge_margin, g.x1 - T.edge_margin
	if not t then
		b.plan = "wander"
		local r = B.draw(b) % 8
		if r < 4 then
			b.goal = lo + B.draw(b) % (hi - lo)
		elseif r == 4 and f.grounded then
			press(out, f, "jump")
			out.jump, b.hold = true, 30
		elseif r == 5 and f.grounded and free(f) then
			local dir = sign(b.goal - f.x)
			local to = f.x + dir * 220
			if dir ~= 0 and to > lo and to < hi then
				press(out, f, "dash")
				out.x = dir
			end
		elseif r == 6 then
			if f.grounded then
				press(out, f, "jump") -- a short hop
			elseif free(f) and safe(f, rules) then
				press(out, f, "light")
			end
		elseif r == 7 and f.grounded and f.on ~= 0 then
			press(out, f, "down")
		end
		return
	end
	local side = sign(t.x - f.x)
	if side == 0 then side = f.face end
	b.face_to = side
	if free(f) and t.inv == 0 then
		local lights, heavies = fitting(f, t, side, rules)
		local name
		local heavy = t.dmg >= T.kill_pct and #heavies > 0 or #heavies > 0 and B.draw(b) % 4 == 0
		if heavy then
			name = pick(heavies, b)
		elseif #lights > 0 then
			name = pick(lights, b)
		end
		if name then
			b.plan = "attack"
			attack(out, f, name, side)
			return
		end
	end
	b.plan = "approach"
	local want = T.spacing * (2 + B.draw(b) % 3) / 4
	b.goal = clamp(t.x - side * want, lo, hi)
	if f.grounded and t.y < f.y - 120 and math.abs(t.x - f.x) < 300 and free(f) then
		press(out, f, "jump")
		out.jump, b.hold = true, 30
	elseif f.grounded and f.on ~= 0 and t.y > f.y + 60 then
		press(out, f, "down")
	elseif not f.grounded and t.y < f.y - 120 and f.jumps2 > 0 and f.vy > -2 and math.abs(t.x - f.x) < 250 then
		press(out, f, "jump")
		out.jump, b.hold = true, 20
	end
end

-- The nearest other in play, as seen, carried on by its seen velocity for the ticks it is late.
local function target_of(f, seen, react)
	local best, bd
	for _, o in ipairs(seen) do
		if in_play(o) then
			local d = math.abs(o.x - f.x) + math.abs(o.y - f.y)
			if not bd or d < bd then best, bd = o, d end
		end
	end
	if not best then return nil end
	return { x = best.x + best.vx * react, y = best.grounded and best.y or best.y + best.vy * react, dmg = best.dmg, inv = best.inv }
end

-- Fighter i's input this tick, from state at the tick's start. Its memory changes; nothing else does.
function B.input(s, i, rules)
	local f = s.fighters[i]
	local b, T = f.bot, rules.tune
	local out = { x = 0, y = 0, jump = false, heavy = false, restart_n = f.seen.restart, pct_n = f.seen.pct, pct = 0 }
	for _, k in ipairs(KEYS) do out[k .. "_n"] = f.seen[k] end
	local write = remember(b, s, i, T.bot_react)
	local seen = b.ring[b.rk]
	if in_play(f) then
		if b.hold > 0 then
			b.hold = b.hold - 1
			out.jump = true
		end
		if b.think > 0 then b.think = b.think - 1 end
		if not f.grounded and not safe(f, rules) then
			recover(out, f, b, rules)
		elseif not threat(out, f, b, s, rules, seen) then
			local t = target_of(f, seen, T.bot_react)
			if b.think == 0 then
				b.think = T.bot_think
				think(out, f, b, rules, t)
			end
			local moving = out.light_n == f.seen.light and out.heavy_n == f.seen.heavy and out.dash_n == f.seen.dash
			if moving then
				local d = b.goal - f.x
				local dir = math.abs(d) > 32 and sign(d) or 0
				if dir == 0 and t and f.grounded and f.face ~= b.face_to and free(f) then dir = b.face_to end
				out.x = dir
				if dir ~= 0 and f.grounded and f.wall == dir and free(f) and out.jump_n == f.seen.jump then
					press(out, f, "jump") -- a wall in the way: over it
					out.jump, b.hold = true, 30
				end
			end
		end
	end
	write(seen)
	b.rk = b.rk % T.bot_react + 1
	return out
end

return B
