-- The jetpack, the recovery (docs/design/brawl-fighting.md decision 7, P-J1-J6). Pure, plain state,
-- stock Lua 5.4 and Luau alike, no trig, `^`, `pairs` or clock (P-H5); combat.lua and fighter.lua call
-- in at their seams.
--
-- The ignite is the air move `jet` (heavy, neutral or Up; combat.lua picks and starts it). When it
-- runs out with K held, the tank burns: `jet` = "on", gravity off, one fuel a tick (hover, climb with
-- Up, thrust with a side). Let go, or empty, and it is "cut" for cut_ticks: an ordinary free fall
-- whose first tick stops the rise. A jump, dash, dodge or light press ends a burning jet with no cut,
-- as do a landing and a hit. The tank refills only on the ground.
--
-- State: `fuel`, `jet` ("", "on" or "cut"), `jk` (the cut's 1-based tick) and `jet_pow`, the flame
-- the view draws (0 none, 1 hover, 2 climb or thrust, 3 the ignite).
local J = {}

local function approach(v, target, by)
	if v < target then return math.min(v + by, target) end
	if v > target then return math.max(v - by, target) end
	return v
end

function J.clear(f)
	f.jet, f.jk, f.jet_pow = "", 0, 0
end

-- A move or dodge starting ends a jet or a cut where it is.
function J.stop(f)
	f.jet, f.jk = "", 0
end

-- The ignite's press tick (before gravity): the kick and the cost.
function J.ignite(f, T)
	f.fuel = f.fuel - T.jet_cost
	f.vy, f.cut, f.ff = -T.jet_kick, false, false
end

local function cut(f)
	f.jet, f.jk = "cut", 0
	if f.vy < 0 then f.vy = 0 end
end

-- After hitlag, hitstun, dodges and moves (combat.lua): `ended` is true on the tick after the ignite's
-- last, `press` when a jump, dash, dodge or light press came this tick. Returns "jet" while it burns
-- (this tick's fuel taken), else nil: the fighter is free.
function J.begin(f, input, T, ended, press)
	if ended then
		if input.heavy and f.fuel > 0 then f.jet = "on" else cut(f) end
	end
	if f.jet == "on" then
		if press then
			f.jet = ""
		elseif not input.heavy or f.fuel <= 0 then
			cut(f)
		else
			f.fuel = f.fuel - 1
			return "jet"
		end
	end
	if f.jet == "cut" then
		f.jk = f.jk + 1
		if f.jk > T.cut_ticks then f.jet, f.jk = "", 0 end
	end
	return nil
end

-- In place of air steering while it burns: a side thrusts (facing follows), else air friction.
function J.steer(f, dir, T)
	if dir ~= 0 then
		f.face = dir
		f.vx = approach(f.vx, dir * T.jet_side, T.jet_accel)
	else
		f.vx = approach(f.vx, 0, T.air_friction)
	end
end

-- In place of gravity while it burns: Up climbs, anything else hovers.
function J.lift(f, input, T)
	f.vy = approach(f.vy, (input.y or 0) < 0 and -T.jet_rise or 0, T.jet_accel)
end

-- After moving: a landing, jump or dash ends a jet or a cut; the ground refills; the flame is set.
function J.after(f, T, jumped, dashed, dir, input)
	if f.jet ~= "" and (f.grounded or jumped or dashed) then f.jet, f.jk = "", 0 end
	if f.grounded then f.fuel = math.min(T.jet_fuel, f.fuel + T.jet_refill) end
	if f.move == "jet" then
		f.jet_pow = 3
	elseif f.jet == "on" then
		f.jet_pow = (dir ~= 0 or (input.y or 0) < 0) and 2 or 1
	else
		f.jet_pow = 0
	end
end

return J
