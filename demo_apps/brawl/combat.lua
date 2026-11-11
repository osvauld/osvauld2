-- Attacks, hits and knockback (docs/design/brawl-fighting.md decisions 1-4, 11). Pure, plain state,
-- stock Lua 5.4 and Luau alike, and no trig, `^`, `pairs` or clock (P-H5): a hit reads circles baked
-- per move tick (moves_baked.lua) and launch vectors from the baked angle table, so every number is
-- `+ - * /` on dyadic values and exact on every machine. fighter.lua calls in at its seams.
--
-- A fighter's combat state: `move` (a move's name, or "") and `mk` (its 1-based move tick), `mhit`
-- (window * 64 + victim index, for each victim a window has hit), `lag` (land lag ticks left),
-- `abuf`/`abx`/`aby` (a buffered press: 1 light, 2 heavy, and the direction held at the press),
-- `dmg`, `hitlag`, `hitstun`, `inv`, the launch (`kb` left along `kux, kuy`; `kbx, kby` its velocity),
-- `dodge` ("spot", "dir" or ""), `dk` (its 1-based tick), `ddir` (the side held at the press), `dcd` (ground
-- cooldown ticks left), `adodges` (air dodges left; fighter.lua refreshes it), and the training readouts (`combo`, `last_dmg`, `last_kb`, `last_angle`, `last_stun`).
-- The jetpack's state is jet.lua's.
local K = require("checks")
local Jet = require("jet")
local C = {}

C.moves, C.angles = K.moves(require("moves"), require("moves_baked"))

-- Every field, as a fighter is made.
function C.fresh(f)
	f.dmg, f.outs, f.combo = 0, 0, 0
	f.last_dmg, f.last_kb, f.last_angle, f.last_stun = 0, 0, 0, 0
	C.clear(f)
end

-- What a ring-out or a respawn ends: the move, lag, buffer, freeze, stun and launch.
function C.clear(f)
	f.move, f.mk, f.mhit, f.lag = "", 0, {}, 0
	f.abuf, f.abx, f.aby = 0, 0, 0
	f.dodge, f.dk, f.ddir, f.dcd = "", 0, 0, 0
	f.hitlag, f.hitstun, f.inv = 0, 0, 0
	f.kb, f.kux, f.kuy, f.kbx, f.kby = 0, 0, 0, 0, 0
	Jet.clear(f)
end

local function sign(v)
	if v > 0 then return 1 end
	if v < 0 then return -1 end
	return 0
end

-- The move a press picks: kind 1 light, 2 heavy; the direction held at the press. Down beats a side
-- held with it; Up is neutral (there are no up moves). Heavy in the air with no side or Down is the
-- jet's ignite.
function C.pick(kind, x, y, grounded)
	local side, down = sign(x) ~= 0, sign(y) > 0
	if grounded then
		local p = kind == 1 and "light" or "heavy"
		return (down and "d" or side and "s" or "n") .. p
	end
	if kind == 1 then return down and "dair" or side and "sair" or "nair" end
	if down then return "dhair" end
	if side then return "shair" end
	return "jet"
end

local function start(f, name, x, T)
	local m = C.moves[name]
	f.move, f.mk, f.mhit, f.abuf = name, 1, {}, 0
	if name:sub(1, 1) == "s" and sign(x) ~= 0 then f.face = sign(x) end
	f.dash_t = 0 -- a move ends a dash where it is
	if not m.air then f.ff = false end
	Jet.stop(f)
	if name == "jet" then Jet.ignite(f, T) end
end

-- A press while busy: kept if this tick is one of the last `atk_buffer` (`remaining` counts this
-- one), else dropped. The latest press wins.
local function buffer(f, T, remaining, light_p, heavy_p, input)
	if not (light_p or heavy_p) then return end
	if remaining <= T.atk_buffer then
		f.abuf, f.abx, f.aby = light_p and 1 or 2, sign(input.x or 0), sign(input.y or 0)
	end
end

-- The start of a fighter's tick. Returns what it is doing this tick: "frozen" (hitlag: nothing
-- moves, nothing counts down but the freeze), "stun" (hitstun: no input), "dodge", "move", "jet"
-- (the tank burning, jet.lua), "lag", or nil (free: movement as before).
function C.begin(f, input, rules, light_p, heavy_p, dodge_p, jump_p, dash_p)
	local T = rules.tune
	if f.hitlag > 0 then
		f.hitlag = f.hitlag - 1
		if f.move ~= "" then buffer(f, T, C.moves[f.move].total - f.mk, light_p, heavy_p, input) end
		return "frozen"
	end
	if f.dcd > 0 then f.dcd = f.dcd - 1 end
	if f.inv > 0 then f.inv = f.inv - 1 end
	if f.hitstun > 0 then
		f.hitstun = f.hitstun - 1
		return "stun"
	end
	if f.dodge ~= "" then
		local spot = f.dodge == "spot"
		if f.dk < (spot and T.dodge_spot_total or T.dodge_dir_total) then
			f.dk = f.dk + 1
			buffer(f, T, (spot and T.dodge_spot_total or T.dodge_dir_total) - f.dk + 1, light_p, heavy_p, input)
			if C.dodge_inv(f, T) and f.inv < 1 then f.inv = 1 end
			return "dodge"
		end
		f.dodge, f.dk = "", 0
	end
	local ignited = false
	if f.move ~= "" then
		local m = C.moves[f.move]
		if f.mk < m.total then
			f.mk = f.mk + 1
			buffer(f, T, m.total - f.mk + 1, light_p, heavy_p, input)
			return "move"
		end
		ignited = f.move == "jet"
		f.move, f.mk = "", 0
	end
	if Jet.begin(f, input, T, ignited, jump_p or dash_p or dodge_p or light_p) then return "jet" end
	if f.lag > 0 then
		buffer(f, T, f.lag, light_p, heavy_p, input)
		f.lag = f.lag - 1
		return "lag"
	end
	if dodge_p and (f.grounded and f.dcd == 0 or not f.grounded and f.adodges > 0) then
		local side = sign(input.x or 0)
		f.dodge, f.dk, f.ddir, f.abuf = side == 0 and "spot" or "dir", 1, side, 0
		if f.grounded then f.dcd = T.dodge_cd else f.adodges = f.adodges - 1 end
		f.dash_t, f.ff = 0, false -- a dodge ends a dash
		Jet.stop(f)
		return "dodge"
	end
	local kind, x, y = 0, 0, 0
	if light_p or heavy_p then
		kind, x, y = light_p and 1 or 2, input.x or 0, input.y or 0
	elseif f.abuf > 0 then
		kind, x, y = f.abuf, f.abx, f.aby
	end
	f.abuf = 0
	if kind > 0 then
		local name = C.pick(kind, x, y, f.grounded)
		if name == "jet" and f.fuel < T.jet_cost then name = nil end
		if name then
			start(f, name, x, T)
			return "move"
		end
	end
	return nil
end

-- Is this dodge tick one of its invulnerable ones?
function C.dodge_inv(f, T)
	if f.dodge == "spot" then return f.dk >= T.dodge_spot_from and f.dk <= T.dodge_spot_to end
	return f.dk >= T.dodge_dir_from and f.dk <= T.dodge_dir_to
end

-- After steering: a spot dodge stands still; a directional one runs dodge_speed for dodge_run ticks
-- (the facing is kept), then keeps dodge_keep of it for a tick and friction does the rest.
function C.dodge_steer(f, T)
	if f.dodge == "spot" then
		f.vx = 0
	elseif f.dk <= T.dodge_run then
		f.vx = f.ddir * T.dodge_speed
	elseif f.dk == T.dodge_run + 1 then
		f.vx = f.ddir * T.dodge_speed * T.dodge_keep
	end
end

-- After gravity: in the air the invulnerable ticks have none.
function C.dodge_fall(f, T)
	if not f.grounded and C.dodge_inv(f, T) then f.vy = 0 end
end

-- After steering: a ground move's vx is its lunge table, exactly; an aerial's lunge overrides drift.
function C.steer(f)
	local m = C.moves[f.move]
	local l = m.lunge
	local lunging = l and f.mk >= l.from and f.mk <= l.to
	if lunging then
		f.vx = f.face * l.speed
	elseif not m.air then
		f.vx = 0
	end
end

-- After gravity: dhair stalls for its startup, then falls at its own speed.
function C.fall(f)
	local m = C.moves[f.move]
	if m.stall and f.mk <= m.startup then
		f.vy = 0
	elseif m.fall and f.mk > m.startup then
		f.vy = m.fall
	end
end

-- After moving: an aerial that landed is over, and the landing is lag for its land_lag ticks (this
-- tick the first). Returns true then.
function C.landed(f)
	if f.move == "" or not f.grounded then return false end
	local m = C.moves[f.move]
	if not m.air then return false end
	f.move, f.mk = "", 0
	f.lag = m.land_lag - 1
	return true
end

local function set_launch(f, kb)
	if kb > 0 then
		f.kb, f.kbx, f.kby = kb, kb * f.kux, kb * f.kuy
	else
		f.kb, f.kux, f.kuy, f.kbx, f.kby = 0, 0, 0, 0, 0
	end
end

-- The launch decays by kb_decay along its direction, to exactly 0.
function C.decay(f, T)
	if f.kb > 0 then set_launch(f, f.kb > T.kb_decay and f.kb - T.kb_decay or 0) end
end

-- A wall or the ground stopped the launch on one axis.
function C.stop_x(f)
	f.kux, f.kbx = 0, 0
end
function C.stop_y(f)
	f.kuy, f.kby = 0, 0
end

local function overlaps(cx, cy, r, v, T)
	local hw = T.body_w / 2
	local nx = cx < v.x - hw and v.x - hw or cx > v.x + hw and v.x + hw or cx
	local ny = cy < v.y - T.body_h and v.y - T.body_h or cy > v.y and v.y or cy
	local dx, dy = cx - nx, cy - ny
	return dx * dx + dy * dy < r * r
end

local function has(list, v)
	for _, x in ipairs(list) do
		if x == v then return true end
	end
	return false
end

local function in_play(f) return f.act ~= "out" and f.act ~= "ko" end

local function apply(a, v, w, rules)
	local T = rules.tune
	v.dmg = math.min(T.dmg_max, v.dmg + w.dmg)
	local kb = w.base + w.growth * v.dmg
	local deg = w.angle
	if v.grounded and deg > 180 and deg < 360 then deg = 360 - deg end
	local dir = C.angles[deg]
	local freeze = T.hitlag_base + math.floor(w.dmg / 2)
	v.combo = (v.hitstun > 0 or v.hitlag > 0) and v.combo + 1 or 1
	v.kux, v.kuy = a.face * dir.x, -dir.y
	set_launch(v, kb)
	v.hitstun = math.floor(kb * T.hitstun_k)
	v.hitlag = math.max(v.hitlag, freeze)
	a.hitlag = math.max(a.hitlag, freeze)
	v.last_dmg, v.last_kb, v.last_angle, v.last_stun = w.dmg, kb, deg, v.hitstun
	-- the hit ends whatever the victim was doing, and its own velocity: the launch is all of it
	v.move, v.mk, v.lag, v.abuf = "", 0, 0, 0
	v.dodge, v.dk, v.adodges = "", 0, T.air_dodges -- a hit ends a dodge and refreshes the air dodge
	Jet.clear(v) -- and the jet, with no cut; the fuel is kept
	v.vx, v.vy, v.dash_t, v.ff, v.cut, v.buf, v.coyote = 0, 0, 0, false, false, -1, 0
	if v.act == "hurt" then
		v.act_t = 0
	else
		v.prev_act, v.prev_t = v.act, v.act_t + 1
		v.act, v.act_t = "hurt", 0
	end
end

-- The hit phase, after every fighter has moved: all live circles against all boxes are gathered
-- (not your own box, not an invulnerable one, not one this window has hit), then every hit is
-- applied at once, so a trade is symmetric. `frozen[i]`: fighter i was in hitlag this tick.
function C.hits(s, rules, frozen)
	local T = rules.tune
	local found = {}
	for i, a in ipairs(s.fighters) do
		if a.move ~= "" and not frozen[i] and in_play(a) then
			local m = C.moves[a.move]
			for _, c in ipairs(m.ticks[a.mk]) do
				local cx, cy, r, w = a.x + a.face * c[1], a.y + c[2], c[3], c[4]
				for j, v in ipairs(s.fighters) do
					local key = w * 64 + j
					if j ~= i and in_play(v) and v.inv == 0 and not has(a.mhit, key) and overlaps(cx, cy, r, v, T) then
						a.mhit[#a.mhit + 1] = key
						found[#found + 1] = { i, j, m.windows[w] } -- the window now: a trade clears the move before it applies
					end
				end
			end
		end
	end
	for _, h in ipairs(found) do
		apply(s.fighters[h[1]], s.fighters[h[2]], h[3], rules)
	end
end

return C
