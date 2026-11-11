-- Brawl's CPU in stock Lua (plan: docs/design/brawl-fighting.md §0, P-CPU1-5), through F.step.
--   lua scripts/test_brawl_bot.lua
package.path = "scripts/?.lua;demo_apps/brawl/?.lua;" .. package.path
local U = require("brawl_test_util")
local F, T, I = U.F, U.tune, U.input
local Bot = require("bot")
local M = require("combat").moves

assert(T.bot_react == 12 and T.bot_think == 10 and T.kill_pct == 100 and T.spacing == 140 and T.edge_margin == 120,
	"the plan's bot numbers in tune.lua")

local function put(f, x, y, face)
	f.x, f.y, f.vx, f.vy = x, y or 0, 0, 0
	f.face = face or f.face
	f.grounded = (y or 0) == 0
	f.on, f.coyote = 0, f.grounded and T.coyote or 0
	f.act, f.act_t = f.grounded and "idle" or "fall", 0
	f.take_y, f.top_y = f.y, f.y
end

local function stage_of(name)
	return U.map.compile(require("maps/" .. name), U.library, { body = { w = T.body_w, h = T.body_h } }).stage
end
local function rules_on(stage, cast, tune)
	return F.rules({ stage = stage, tune = tune or T, cast = cast })
end
local function with(over)
	local t = {}
	for k, v in pairs(T) do t[k] = v end
	for k, v in pairs(over) do t[k] = v end
	return t
end
local IDLE = I()

-- P-CPU1: pure and deterministic. Bot.input reads state and rules and returns an ordinary input;
-- presses are counts that grow by at most one a call; its memory is plain data and a wire copy gives
-- the same input and the same next state.
do
	-- MINSTD, exact
	local b = { seed = 1 }
	local want = { 48271, 182605794, 1291394886, 1914720637, 2078669041 }
	for k = 1, #want do
		local r = Bot.draw(b)
		assert(r == want[k] and b.seed == want[k], "P-CPU1: MINSTD draw " .. k .. " is " .. r)
	end
	local rules = U.rules({ "bot", "bot" })
	assert(rules.spans and rules.ground, "P-CPU1: F.rules derives the stage spans for the bot")
	local s = F.new(rules)
	for i = 1, 2 do assert(type(s.fighters[i].bot) == "table" and s.fighters[i].bot.seed > 0, "P-CPU1: a bot carries its memory") end
	assert(s.fighters[1].bot.seed ~= s.fighters[2].bot.seed, "P-CPU1: two bots draw from different seeds")
	local FIELDS = { "x", "y", "jump", "heavy", "jump_n", "down_n", "dash_n", "light_n", "heavy_n", "dodge_n", "restart_n", "pct_n", "pct" }
	local KEYS = { "jump", "down", "dash", "light", "heavy", "dodge" }
	local presses = {}
	for t = 1, 2400 do
		if t % 150 == 0 then
			local ok, why = U.plain(s)
			assert(ok, "P-CPU1: tick " .. t .. ": " .. tostring(why))
			for i = 1, 2 do
				local c1, c2 = U.copy(s), U.copy(s)
				local a, b2 = Bot.input(c1, i, rules), Bot.input(c2, i, rules)
				ok, why = U.same(a, b2)
				assert(ok, "P-CPU1: the same state, the same input, tick " .. t .. ": " .. tostring(why))
				ok, why = U.same(c1, c2)
				assert(ok, "P-CPU1: the same memory after, tick " .. t .. ": " .. tostring(why))
				local n = 0
				for _ in pairs(a) do n = n + 1 end
				assert(n == #FIELDS, "P-CPU1: an ordinary input has " .. #FIELDS .. " fields, got " .. n)
				for _, k in ipairs(FIELDS) do assert(a[k] ~= nil, "P-CPU1: input." .. k) end
				local f = c1.fighters[i]
				for _, k in ipairs(KEYS) do
					local d = a[k .. "_n"] - f.seen[k]
					assert(d == 0 or d == 1, "P-CPU1: " .. k .. "_n grows by 0 or 1, by " .. d)
				end
				assert(a.restart_n == f.seen.restart and a.pct_n == f.seen.pct, "P-CPU1: a bot never restarts or trains")
			end
		end
		local before = {}
		for i = 1, 2 do before[i] = {} for _, k in ipairs(KEYS) do before[i][k] = s.fighters[i].seen[k] end end
		F.step(s, {}, rules)
		for i = 1, 2 do
			for _, k in ipairs(KEYS) do
				if s.fighters[i].seen[k] > before[i][k] then presses[k] = (presses[k] or 0) + 1 end
			end
		end
	end
	local ps = {}
	for _, k in ipairs(KEYS) do ps[#ps + 1] = k .. "=" .. (presses[k] or 0) end
	assert((presses.light or 0) > 0 and (presses.heavy or 0) > 0 and (presses.jump or 0) > 0, "P-CPU1: the bots press: " .. table.concat(ps, " "))
	print("P-CPU1: presses in 2 400 ticks of bot vs bot: " .. table.concat(ps, " "))
end

-- P-CPU2: it doesn't fall off. Alone, 7 200 ticks from 8 points on each stage: no stock lost. From 4
-- points offstage within P-J5's reach it gets back onto the stage, and from a fifth against the stage's
-- side with no double jump and 30 fuel.
do
	local starts = {
		yard = { { -850, 0 }, { -600, 0 }, { -300, 0 }, { -40, 0 }, { 120, 0 }, { 450, 0 }, { 840, 0 }, { -460, -260 } },
		isles = { { -1060, 0 }, { -700, 0 }, { -480, -156 }, { -200, 0 }, { 150, 0 }, { 500, 0 }, { 1050, 0 }, { 760, -280 } },
	}
	for _, name in ipairs({ "yard", "isles" }) do
		local stage = stage_of(name)
		local rules = rules_on(stage, { "bot" })
		local moved, jumps = 0, 0
		for k, p in ipairs(starts[name]) do
			local s = F.new(rules)
			local f = s.fighters[1]
			put(f, p[1], p[2], 1)
			if p[2] ~= 0 then f.grounded, f.act = true, "idle" end
			local x0, x1 = f.x, f.x
			for t = 1, 7200 do
				F.step(s, {}, rules)
				assert(f.outs == 0, "P-CPU2: " .. name .. " start " .. k .. " (" .. p[1] .. ", " .. p[2] .. "): out at tick " .. t
					.. " " .. F.describe(f, t))
				x0, x1 = math.min(x0, f.x), math.max(x1, f.x)
			end
			moved = moved + (x1 - x0)
			jumps = jumps + f.seen.jump
		end
		assert(moved > 8 * 600 and jumps > 8 * 10, "P-CPU2: alone it wanders: " .. moved .. " units, " .. jumps .. " jumps")
		-- offstage, falling, the double jump still there
		local e1, e0 = rules.ground.x1, rules.ground.x0
		-- the fifth was spiked: on the stage's side, low, the double jump spent, little fuel (wall jumps)
		local points = { { e1 + 300, 400, T.max_fall }, { e0 - 300, 400, T.max_fall }, { e1 + 450, -200, 0 }, { e0 - 400, 100, 4 },
			{ e1 + T.body_w / 2, 450, 3, jumps2 = 0, fuel = 30 } }
		local took = {}
		for k, p in ipairs(points) do
			local s = F.new(rules)
			local f = s.fighters[1]
			put(f, p[1], p[2], p[1] > 0 and -1 or 1)
			f.vy = p[3]
			if p.jumps2 then f.jumps2, f.fuel = p.jumps2, p.fuel end
			local back
			for t = 1, 900 do
				F.step(s, {}, rules)
				assert(f.outs == 0, "P-CPU2: " .. name .. " offstage " .. k .. " (" .. p[1] .. ", " .. p[2] .. "): out at tick " .. t)
				if f.grounded and f.y == rules.ground.y and f.x > e0 and f.x < e1 then
					back = t
					break
				end
			end
			assert(back, "P-CPU2: " .. name .. " offstage " .. k .. ": not back on the stage, " .. F.describe(f, 900))
			took[#took + 1] = string.format("(%g, %g) %d ticks fuel %g", p[1], p[2], back, f.fuel)
		end
		print("P-CPU2: " .. name .. ": 8 starts x 7 200 ticks, no stock lost (wandered " .. moved .. " units, " .. jumps
			.. " jumps); recovered from " .. table.concat(took, ", "))
	end
end

-- P-CPU3: it fights. Against the dummy for 3 600 ticks: >= 20 hits, >= 6 different moves, the dummy rung
-- out at least once; the bot itself loses nothing.
do
	local rules = U.rules({ "bot", "dummy" })
	local s = F.new(rules)
	local b, d = s.fighters[1], s.fighters[2]
	local hits, used, prev, outs0 = 0, {}, 0, 0
	for _ = 1, 3600 do
		F.step(s, {}, rules)
		if d.dmg > prev then
			hits = hits + 1
			used[b.move] = (used[b.move] or 0) + 1
		end
		prev = d.dmg
	end
	local names, kinds = {}, 0
	for name, n in pairs(used) do names[#names + 1] = name .. "x" .. n kinds = kinds + 1 end
	table.sort(names)
	assert(hits >= 20, "P-CPU3: only " .. hits .. " hits")
	assert(kinds >= 6, "P-CPU3: only " .. kinds .. " moves hit: " .. table.concat(names, " "))
	assert(d.outs >= 1, "P-CPU3: the dummy was never rung out")
	assert(b.outs == outs0 and b.stocks == T.stocks, "P-CPU3: the bot fell off")
	print("P-CPU3: vs the dummy, 3 600 ticks: " .. hits .. " hits, " .. kinds .. " moves (" .. table.concat(names, " ")
		.. "), dummy outs " .. d.outs)
end

-- P-CPU6 (step 6, feel): two CPUs kill each other. 8 seeds x 7 200 ticks each lose at least one stock (they
-- once passed 130 % without a single KO), and the median KO percent is at most 150. The numbers are
-- printed for the step note.
do
	local rules = U.rules({ "bot", "bot" })
	local kos = {}
	for seed = 1, 8 do
		rules.seed = seed
		local s = F.new(rules)
		local pd, ps, lost = { 0, 0 }, { T.stocks, T.stocks }, 0
		for _ = 1, 7200 do
			F.step(s, {}, rules)
			for i, f in ipairs(s.fighters) do
				if f.stocks < ps[i] then kos[#kos + 1] = pd[i]; lost = lost + 1 end
				pd[i], ps[i] = f.dmg, f.stocks
			end
			if s.winner ~= 0 then break end
		end
		assert(lost >= 1, "P-CPU6: seed " .. seed .. ": 7 200 ticks and nobody lost a stock")
	end
	table.sort(kos)
	local median = kos[(#kos + 1) // 2]
	assert(median <= 150, "P-CPU6: median KO percent " .. median)
	print("P-CPU6: bot vs bot, 8 seeds x 7 200 ticks: " .. #kos .. " KOs at " .. table.concat(kos, " ") .. "%; median " .. median)
end

-- P-CPU4: it waits. A dodge answers an attack no sooner than bot_react ticks after the attack started,
-- and only an attack that will reach it; with bot_dodge 0 it never dodges.
do
	local function trial(tune, gap)
		local rules = U.rules({ "human", "bot" })
		rules.tune = tune
		local s = F.new(rules)
		local h, b = s.fighters[1], s.fighters[2]
		put(h, -200, 0, 1)
		put(b, -200 + gap, 0, -1)
		b.bot.think, b.bot.goal = 100000, b.x -- no plans: only its eyes and its dodge
		for _ = 1, 40 do F.step(s, { IDLE }, rules) end
		local t0 = s.tick + 1
		local started, dodged = nil, nil
		for _ = 1, 120 do
			F.step(s, { I({ heavy_n = 1, x = 1 }) }, rules)
			if not started and h.act == "sheavy" then started = s.tick end
			if b.dodge ~= "" and b.dk == 1 then dodged = s.tick end
			if dodged then break end
		end
		assert(started == t0, "P-CPU4: the human's sheavy starts on the press")
		return dodged and dodged - started or nil, b
	end
	for _, react in ipairs({ 12, 20 }) do
		local late, b = trial(with({ bot_react = react, bot_dodge = 1 }), 150)
		assert(late and late >= react, "P-CPU4: a dodge " .. tostring(late) .. " ticks after the attack, react " .. react)
		assert(b.dodge == "spot", "P-CPU4: on the ground it spot dodges")
		print("P-CPU4: react " .. react .. ": dodged " .. late .. " ticks after sheavy started")
	end
	assert(not trial(with({ bot_dodge = 0 }), 150), "P-CPU4: bot_dodge 0 never dodges")
	assert(not trial(with({ bot_dodge = 1 }), 700), "P-CPU4: an attack that can't reach it is not dodged")
end

-- P-CPU5: replay. Bot vs bot and bot vs an idle human, 3 600 ticks: straight, through wire copies at
-- 900, 1 777 and 2 500, and rewound to 1 777, equal. The final lines are printed for S-F5. The cost of
-- Bot.input per call is printed (stock Lua).
do
	local function run_from(s, rules, from, to, each)
		for t = from, to do
			F.step(s, { IDLE }, rules)
			if each then each(t, s) end
		end
		return s
	end
	for _, cast in ipairs({ { "bot", "bot" }, { "human", "bot" } }) do
		local rules = U.rules(cast)
		local label = cast[1] .. " vs " .. cast[2]
		local hits, prev = 0, { 0, 0 }
		local straight = run_from(F.new(rules), rules, 1, 3600, function(t, s)
			for i, f in ipairs(s.fighters) do
				if f.dmg > prev[i] then hits = hits + 1 end
				prev[i] = f.dmg
			end
		end)
		local live = U.copy(run_from(F.new(rules), rules, 1, 900))
		run_from(live, rules, 901, 1777)
		local at = U.copy(live)
		live = U.copy(live)
		run_from(live, rules, 1778, 2500)
		live = U.copy(live)
		run_from(live, rules, 2501, 3600)
		local ok, why = U.same(straight, live)
		assert(ok, "P-CPU5: " .. label .. " through copies: " .. tostring(why))
		ok, why = U.same(straight, run_from(U.copy(at), rules, 1778, 3600))
		assert(ok, "P-CPU5: " .. label .. " rewound to 1 777: " .. tostring(why))
		assert(hits >= 10, "P-CPU5: " .. label .. ": only " .. hits .. " hits")
		if cast[1] == "human" then
			assert(straight.fighters[1].stocks < T.stocks, "P-CPU5: the idle human lost no stock")
			assert(straight.fighters[2].stocks == T.stocks, "P-CPU5: the bot fell off against an idle human")
		end
		print("P-CPU5: " .. label .. ": " .. hits .. " hits, stocks " .. straight.fighters[1].stocks .. "/" .. straight.fighters[2].stocks)
		for i, f in ipairs(straight.fighters) do print("P-CPU5 final " .. label .. " " .. i .. ": " .. F.describe_fight(f)) end
	end
	-- played to the end against the idle human: the CPU wins
	local rules = U.rules({ "human", "bot" })
	local s = F.new(rules)
	for _ = 1, 30000 do
		F.step(s, { IDLE }, rules)
		if s.winner ~= 0 then break end
	end
	assert(s.winner == 2, "P-CPU5: the CPU wins against the idle human")
	print("P-CPU5: the CPU wins at tick " .. s.tick)
	-- the cost
	local r2 = U.rules({ "bot", "bot" })
	local s2 = F.new(r2)
	for _ = 1, 600 do F.step(s2, {}, r2) end
	local n, t0 = 0, os.clock()
	for _ = 1, 3000 do
		local c = s2
		Bot.input(c, 1, r2)
		Bot.input(c, 2, r2)
		F.step(s2, {}, r2)
		n = n + 2
	end
	local per_step = (os.clock() - t0) / 3000
	local t1 = os.clock()
	for _ = 1, 3000 do Bot.input(s2, 1, r2) end
	local per = (os.clock() - t1) / 3000
	assert(per < 1e-4, "P-CPU5: Bot.input costs " .. per * 1e6 .. " us")
	print(string.format("P-CPU5: Bot.input %.1f us a call; a bot-vs-bot tick with two extra calls %.1f us (stock Lua)", per * 1e6, per_step * 1e6))
end

print("brawl bot ok")
