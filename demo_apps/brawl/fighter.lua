-- The rules: one tick of every fighter, the stage check and collision. Pure, and runs unchanged
-- under stock Lua 5.4 and Luau (scripts/test_brawl_*.lua test this file): no `+=`, no `^`, no
-- `goto`/`continue`, no math.clamp. Plan: docs/design/brawl-movement.md.
--
-- State is plain data (net.match copies it and hands back copies): `{ tick, fighters = {...} }`.
-- The stage and tunables are the match's setup, passed in as `rules` and never stored in state.
-- +y is down. A fighter's (x, y) is its feet: the box is x ± body_w/2, y - body_h .. y.
local F = {}

local EMPTY = {}
F.EMPTY = EMPTY

local function approach(v, target, by)
	if v < target then return math.min(v + by, target) end
	if v > target then return math.max(v - by, target) end
	return v
end

local function sign(v)
	if type(v) ~= "number" then return 0 end
	if v > 0 then return 1 end
	if v < 0 then return -1 end
	return 0
end

-- The stage check ------------------------------------------------------------------------------

local SHAPES = {
	stage = { { "name", "string" }, { "solids", "table" }, { "platforms", "table" }, { "spawns", "table" },
		{ "respawn", "table" }, { "blast", "table" }, { "camera", "table" } },
	solid = { { "id", "string" }, { "x", "number" }, { "y", "number" }, { "w", "number" }, { "h", "number" } },
	platform = { { "id", "string" }, { "x", "number" }, { "y", "number" }, { "w", "number" } },
	point = { { "x", "number" }, { "y", "number" } },
	rect = { { "x", "number" }, { "y", "number" }, { "w", "number" }, { "h", "number" } },
}

local function bad(where, why) error("stage: " .. where .. ": " .. why, 0) end

local function fields(t, shape, where)
	if type(t) ~= "table" then bad(where, "not a table") end
	local known, unknown = {}, {}
	for _, f in ipairs(SHAPES[shape]) do known[f[1]] = true end
	for k in pairs(t) do
		if not known[k] then unknown[#unknown + 1] = tostring(k) end
	end
	table.sort(unknown)
	if unknown[1] then bad(where, "unknown field " .. unknown[1]) end
	for _, f in ipairs(SHAPES[shape]) do
		if t[f[1]] == nil then bad(where, "no " .. (f[1] == "w" and "width" or f[1] == "h" and "height" or f[1])) end
		if type(t[f[1]]) ~= f[2] then bad(where, f[1] .. " must be a " .. f[2]) end
	end
end

local function list(t, shape, where)
	for i, v in ipairs(t) do
		local at = where .. "[" .. i .. "]"
		if type(v) == "table" and type(v.id) == "string" then at = at .. " \"" .. v.id .. "\"" end
		fields(v, shape, at)
		if v.w ~= nil and not (v.w > 0) then bad(at, "w (width) must be above 0") end
		if v.h ~= nil and not (v.h > 0) then bad(at, "h (height) must be above 0") end
	end
end

local function inside(r, outer)
	return r.x >= outer.x and r.y >= outer.y and r.x + r.w <= outer.x + outer.w and r.y + (r.h or 0) <= outer.y + outer.h
end

-- The stage, checked: unknown fields, a platform with no width, a spawn inside a solid, or a blast
-- zone that doesn't hold the solids and platforms are errors naming the entry.
function F.check_stage(stage, body)
	body = body or { w = 80, h = 200 }
	fields(stage, "stage", "stage")
	list(stage.solids, "solid", "solids")
	list(stage.platforms, "platform", "platforms")
	list(stage.spawns, "point", "spawns")
	if not stage.spawns[1] then bad("spawns", "none") end
	fields(stage.respawn, "point", "respawn")
	fields(stage.blast, "rect", "blast")
	fields(stage.camera, "rect", "camera")
	local function clear(p, where)
		for i, s in ipairs(stage.solids) do
			if p.x + body.w / 2 > s.x and p.x - body.w / 2 < s.x + s.w and p.y > s.y and p.y - body.h < s.y + s.h then
				bad(where, "inside solids[" .. i .. "] \"" .. s.id .. "\"")
			end
		end
	end
	for i, p in ipairs(stage.spawns) do clear(p, "spawns[" .. i .. "]") end
	clear(stage.respawn, "respawn")
	for i, s in ipairs(stage.solids) do
		if not inside(s, stage.blast) then bad("solids[" .. i .. "] \"" .. s.id .. "\"", "outside the blast zone") end
	end
	for i, p in ipairs(stage.platforms) do
		if not inside(p, stage.blast) then bad("platforms[" .. i .. "] \"" .. p.id .. "\"", "outside the blast zone") end
	end
	return stage
end

-- The match's constants from its setup `{ stage, tune }`.
function F.rules(setup)
	local T = setup.tune
	return { stage = F.check_stage(setup.stage, { w = T.body_w, h = T.body_h }), tune = T }
end

-- Fighters --------------------------------------------------------------------------------------

-- What holds feet at (x, y) up: "solid", a platform's index, or nil.
local function support(stage, T, x, y)
	local hw = T.body_w / 2
	for _, s in ipairs(stage.solids) do
		if y == s.y and x - hw < s.x + s.w and x + hw > s.x then return "solid" end
	end
	for i, p in ipairs(stage.platforms) do
		if y == p.y and x - hw < p.x + p.w and x + hw > p.x then return i end
	end
	return nil
end

-- On a switch the old act and where it was (`prev_act`, `prev_t`: its act_t this tick, had it gone
-- on) are kept, so the view can blend out of it from state alone (look.lua).
local function set_act(f, act)
	if f.act == act then
		f.act_t = f.act_t + 1
	else
		f.prev_act, f.prev_t = f.act, f.act_t + 1
		f.act, f.act_t = act, 0
	end
end

-- At (x, y), at rest, everything per airtime refreshed.
local function place(f, rules, x, y)
	local T = rules.tune
	local under = support(rules.stage, T, x, y)
	f.x, f.y, f.vx, f.vy = x, y, 0, 0
	f.grounded = under ~= nil
	f.on = type(under) == "number" and under or 0
	f.wall, f.drop, f.out = 0, 0, 0
	f.jumps2, f.dashes, f.wall_jumps = T.air_jumps, T.air_dashes, T.wall_jumps
	f.take_y, f.top_y, f.peak = y, y, 0
	f.coyote, f.buf, f.cut, f.ff = f.grounded and T.coyote or 0, -1, false, false
	f.lock, f.lock_side, f.dash_t, f.dash_g = 0, 0, 0, false
	f.act, f.act_t = f.grounded and "idle" or "apex", 0 -- at rest in the air: the top of an arc
	f.prev_act, f.prev_t = f.act, 0
end

local function fresh(rules, i, seen)
	local sp = rules.stage.spawns[(i - 1) % #rules.stage.spawns + 1]
	local f = { face = 1, stocks = rules.tune.stocks, seen = seen or { jump = 0, down = 0, dash = 0, restart = 0 } }
	place(f, rules, sp.x, sp.y)
	return f
end

function F.new(rules, n)
	local s = { tick = 0, fighters = {} }
	for i = 1, n or 1 do s.fighters[i] = fresh(rules, i) end
	return s
end

-- Every fighter back at its spawn with full stocks. Press counts are kept: they are the inputs'.
function F.reset(s, rules)
	for i, f in ipairs(s.fighters) do s.fighters[i] = fresh(rules, i, f.seen) end
end

-- A press counted in an input (`jump_n`, ...): an input used for several ticks, or again after a
-- rewind, acts once per press.
local function pressed(f, input, key)
	local n = input[key .. "_n"] or 0
	if n > f.seen[key] then
		f.seen[key] = n
		return true
	end
	return false
end

local function move_x(f, stage, T)
	local hw, h = T.body_w / 2, T.body_h
	local nx = f.x + f.vx
	for _, s in ipairs(stage.solids) do
		if f.y - h < s.y + s.h and f.y > s.y then
			if f.vx > 0 and f.x + hw <= s.x and nx + hw > s.x then
				nx, f.vx = math.min(nx, s.x - hw), 0
			elseif f.vx < 0 and f.x - hw >= s.x + s.w and nx - hw < s.x + s.w then
				nx, f.vx = math.max(nx, s.x + s.w + hw), 0
			end
		end
	end
	f.x = nx
end

-- A platform catches only a fighter moving down whose feet were at or above its top when the
-- tick began, and not while it drops through.
local function move_y(f, stage, T)
	local hw, h = T.body_w / 2, T.body_h
	local ny = f.y + f.vy
	local landed, on = false, 0
	for _, s in ipairs(stage.solids) do
		if f.x - hw < s.x + s.w and f.x + hw > s.x then
			if f.vy > 0 and f.y <= s.y and ny >= s.y then
				ny, landed, on = s.y, true, 0
			elseif f.vy < 0 and f.y - h >= s.y + s.h and ny - h < s.y + s.h then
				ny, f.vy = math.max(ny, s.y + s.h + h), 0
			end
		end
	end
	if f.drop == 0 then
		for i, p in ipairs(stage.platforms) do
			if f.vy > 0 and f.y <= p.y and ny >= p.y and f.x - hw < p.x + p.w and f.x + hw > p.x then
				ny, landed, on = p.y, true, i
			end
		end
	end
	f.y = ny
	if landed then f.vy = 0 end
	f.grounded, f.on = landed, on
end

-- Which side a solid touches the box on: 1 right, -1 left, 0 none.
local function wall_side(f, stage, T)
	local hw, h = T.body_w / 2, T.body_h
	for _, s in ipairs(stage.solids) do
		if f.y - h < s.y + s.h and f.y > s.y then
			if f.x + hw == s.x then return 1 end
			if f.x - hw == s.x + s.w then return -1 end
		end
	end
	return 0
end

local function blasted(f, stage, T)
	local b, cy = stage.blast, f.y - T.body_h / 2
	return f.x < b.x or f.x > b.x + b.w or cy < b.y or cy > b.y + b.h
end

-- A press that can jump now does, and says which kind: the ground jump (also within `coyote` of a
-- ledge), a wall jump (touching a wall and holding toward it, with any left), or the double jump.
local function try_jump(f, T, raw)
	local kind
	if f.grounded or f.coyote > 0 then
		kind = "ground"
		f.vy = -T.jump_speed
	elseif f.wall ~= 0 and raw == f.wall and f.wall_jumps > 0 then
		kind = "wall"
		f.vx, f.vy = -f.wall * T.wall_jump_x, -T.wall_jump_y
		f.wall_jumps = f.wall_jumps - 1
		f.lock, f.lock_side, f.face = T.wall_lock, f.wall, -f.wall
	elseif f.jumps2 > 0 then
		kind = "double"
		f.vy = -T.double_jump_speed
		f.jumps2 = f.jumps2 - 1
	else
		return nil
	end
	f.grounded, f.on, f.coyote, f.buf, f.ff = false, 0, 0, -1, false
	f.cut, f.dash_t = kind == "ground", 0 -- a jump ends a dash and keeps its speed
	return kind
end

local function dash(f, T, raw, ydir)
	local g = f.grounded
	local dx, dy = f.face * T.dash_speed, 0
	if g then
		if raw ~= 0 then dx = raw * T.dash_speed end
	else
		f.dashes = f.dashes - 1
		if raw ~= 0 and ydir ~= 0 then
			dx, dy = raw * T.dash_diag, ydir * T.dash_diag
		elseif raw ~= 0 then
			dx = raw * T.dash_speed
		elseif ydir ~= 0 then
			dx, dy = 0, ydir * T.dash_speed
		end
	end
	f.vx, f.vy, f.dash_t, f.dash_g, f.ff, f.cut = dx, dy, T.dash_ticks, g, false, false
	if dx ~= 0 then f.face = sign(dx) end
end

local function tick(f, input, rules)
	local T, stage = rules.tune, rules.stage
	local jump_p, down_p, dash_p = pressed(f, input, "jump"), pressed(f, input, "down"), pressed(f, input, "dash")
	if f.act == "ko" then
		f.act_t = f.act_t + 1
		return
	end
	if f.act == "out" then
		f.out = f.out - 1
		if f.out > 0 then
			f.act_t = f.act_t + 1
		else
			local rp = stage.respawn
			place(f, rules, rp.x, rp.y)
		end
		return
	end
	if f.drop > 0 then f.drop = f.drop - 1 end
	local raw, ydir = sign(input.x), sign(input.y)
	local dir = raw
	if f.lock > 0 then
		f.lock = f.lock - 1
		if dir == f.lock_side then dir = 0 end
	end
	local start_y, was_grounded = f.y, f.grounded

	-- Velocities, in this order: jump cut, steering, jump, dash, drop, fast fall, gravity.
	if f.cut and not input.jump and f.vy < -T.jump_cut then f.vy = -T.jump_cut end
	local skid = false
	if f.dash_t > 0 then
		-- a dash steers nothing
	elseif f.grounded then
		local vs = sign(f.vx)
		if dir ~= 0 and vs ~= 0 and dir ~= vs and (f.act == "turn" or math.abs(f.vx) >= T.turn_min) then
			skid = true
			f.vx = approach(f.vx, 0, T.skid)
			if f.vx == 0 then f.face = dir end
		else
			if dir ~= 0 then f.face = dir end
			f.vx = approach(f.vx, dir * T.run_speed, dir ~= 0 and T.ground_accel or T.ground_friction)
		end
	else
		if dir ~= 0 then f.face = dir end
		f.vx = approach(f.vx, dir * T.run_speed, dir ~= 0 and T.air_accel or T.air_friction)
	end

	-- A press waits `buffer` ticks for something to jump from; one used at once is not kept.
	if jump_p then
		f.buf = 0
	elseif f.buf >= 0 then
		f.buf = f.buf + 1
		if f.buf > T.buffer then f.buf = -1 end
	end
	local jumped = f.buf >= 0 and try_jump(f, T, raw) or nil
	local dashed = false
	if dash_p and not jumped and f.dash_t == 0 and (f.grounded or f.dashes > 0) then
		dash(f, T, raw, ydir)
		dashed = true
	end
	if down_p and not jumped and not dashed then
		if was_grounded then
			if f.grounded and f.on ~= 0 then f.drop, f.grounded, f.on, f.coyote = T.drop_ticks, false, 0, 0 end
		elseif f.dash_t == 0 and f.vy >= 0 then
			f.ff, f.vy = true, T.fast_fall
		end
	end

	local sliding = false
	if f.dash_t == 0 or f.dash_g then
		f.vy = math.min(f.vy + T.gravity, f.ff and T.fast_fall or T.max_fall)
		if not f.grounded and f.wall ~= 0 and dir == f.wall and f.vy >= 0 then
			sliding = true
			f.vy = math.min(f.vy, T.wall_slide)
		end
	end
	local before_vx = f.vx
	move_x(f, stage, T)
	local blocked = before_vx ~= 0 and f.vx == 0
	move_y(f, stage, T)
	f.wall = wall_side(f, stage, T)

	if blasted(f, stage, T) then
		f.stocks = f.stocks - 1
		f.vx, f.vy, f.grounded, f.on, f.wall = 0, 0, false, 0, 0
		f.out = T.respawn_ticks
		set_act(f, f.stocks > 0 and "out" or "ko")
		return
	end

	-- The dash ends on its last tick, on a wall it ran into, or when it lands: velocity x dash_keep.
	local dashing = f.dash_t > 0
	if dashing then
		f.dash_t = f.dash_t - 1
		if f.dash_t == 0 or blocked or (f.grounded and not f.dash_g) then
			f.dash_t = 0
			f.vx, f.vy = f.vx * T.dash_keep, f.vy * T.dash_keep
		end
	end

	-- The highest point of this airtime, above where it began.
	if f.grounded then
		f.jumps2, f.dashes, f.wall_jumps = T.air_jumps, T.air_dashes, T.wall_jumps
		f.coyote, f.ff, f.cut, f.lock = T.coyote, false, false, 0
	elseif was_grounded then
		f.take_y, f.top_y = start_y, math.min(start_y, f.y)
		f.peak = f.take_y - f.top_y
	else
		f.top_y = math.min(f.top_y, f.y)
		f.peak = f.take_y - f.top_y
		if f.coyote > 0 then f.coyote = f.coyote - 1 end
	end
	if not f.grounded and f.wall ~= 0 then f.dashes = T.air_dashes end

	if dashing then
		set_act(f, "dash")
	elseif f.grounded then
		-- Landing with no direction held is `land` for land_ticks, until a direction is held.
		local landing = dir == 0 and not skid
			and (not was_grounded or (f.act == "land" and f.act_t + 1 < T.land_ticks))
		set_act(f, skid and "turn" or landing and "land" or (f.vx ~= 0 or dir ~= 0) and "run" or "idle")
	elseif jumped == "double" or (f.act == "jump2" and f.vy < 0) then
		set_act(f, "jump2")
	elseif sliding then
		set_act(f, "wall")
	else
		set_act(f, f.vy < -T.apex_vy and "rise" or f.vy > T.apex_vy and "fall" or "apex")
	end
end

-- One tick of the match: `inputs[i]` is fighter i's input (nil before one arrived). A restart
-- press while any fighter is ko resets them all.
function F.step(s, inputs, rules)
	s.tick = s.tick + 1
	local restart = false
	for i, f in ipairs(s.fighters) do
		if pressed(f, inputs[i] or EMPTY, "restart") and f.act == "ko" then restart = true end
	end
	if restart then
		F.reset(s, rules)
		return
	end
	for i, f in ipairs(s.fighters) do tick(f, inputs[i] or EMPTY, rules) end
end

-- The `fighter:state` readout: numbers by %.17g, so a reader gets them back exactly.
function F.describe(f, tick)
	local function n(v) return string.format("%.17g", v) end
	return "tick=" .. n(tick) .. " x=" .. n(f.x) .. " y=" .. n(f.y) .. " vx=" .. n(f.vx) .. " vy=" .. n(f.vy)
		.. " act=" .. f.act .. " act_t=" .. n(f.act_t) .. " face=" .. n(f.face) .. " jumps2=" .. n(f.jumps2)
		.. " dashes=" .. n(f.dashes) .. " wall_jumps=" .. n(f.wall_jumps) .. " stocks=" .. n(f.stocks)
		.. " peak=" .. n(f.peak) .. " wall=" .. n(f.wall) .. " drop=" .. n(f.drop)
end

return F
