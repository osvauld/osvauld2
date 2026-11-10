-- The single-player World surface the stock-Lua tests were written against, over the match-era World
-- (jetpack-mp.md step 7): `input` with booleans for presses, `start`/`pause`/`resume`, `advance` on a
-- clock, `step`, `player`. In the app the platform's `net.match` does all of this; here a test drives it.
package.path = "demo_apps/jetpack/?.lua;" .. package.path
math.clamp = math.clamp or function(v, lo, hi) return math.max(lo, math.min(hi, v)) end
math.atan2 = math.atan2 or function(y, x) return math.atan(y, x) end
local World = require("world")
local F = require("format")
local C = require("theme")

local Solo = {}
local own = { w = true, clock = true, last = true, debt = true, presses = true, input = true }

local meta = {}
meta.__index = function(s, k)
	if Solo[k] then return Solo[k] end
	local w = rawget(s, "w")
	if k == "player" then return w.humans[1] end
	return w[k]
end
meta.__newindex = function(s, k, v)
	if own[k] then rawset(s, k, v) else rawget(s, "w")[k] = v end
end

-- `map` and `guns` default to the baked yard and guns; `loadout` is the `jetpack-loadout` seam.
function Solo.new(map, guns, loadout)
	map = map or F.find(F.maps(require("baked/maps")), "yard")
	guns = guns or F.guns(require("baked/guns"))
	local s = setmetatable({}, meta)
	s.w = World.new({ map = map, guns = guns, pilots = {}, loadout = loadout }, 1)
	s.clock, s.last, s.debt = 0, nil, 0
	s.presses = { reload = 0, swap = 0, pickup = 0 }
	s:clear_input()
	return s
end

function Solo:clear_input()
	self.input = { move = 0, thrust = false, fire = false, reload = false, swap = false, pickup = false, aim = 0 }
end
function Solo:start() self.w.state = "Playing"; self:clear_input(); self.last = nil; self.debt = 0 end
function Solo:pause() self.w.state = "Paused"; self:clear_input(); self.debt = 0 end
function Solo:resume() self.w.state = "Playing"; self:clear_input(); self.last = nil; self.debt = 0 end

-- One tick on the current input: a press is counted once, then let go, as the app's controls do.
function Solo:step(dt)
	local input, presses = self.input, self.presses
	for _, k in ipairs({ "reload", "swap", "pickup" }) do
		if input[k] then presses[k] = presses[k] + 1; input[k] = false end
	end
	World.step(self.w, dt, { { move = input.move, thrust = input.thrust, fire = input.fire, aim = input.aim,
		reload = presses.reload, swap = presses.swap, pickup = presses.pickup } })
	if self.w.humans[1].hp <= 0 then self:clear_input() end
end

-- Whole ticks up to `elapsed`, at most eight per call, as the app's frame clock ran them.
function Solo:advance(elapsed)
	self.clock = elapsed
	local delta = self.last and math.max(0, elapsed - self.last) or 0
	self.last = elapsed
	if self.w.state ~= "Playing" then self.debt = 0; return end
	self.debt = math.min(self.debt + delta, 8 * C.step)
	while self.debt + 1e-10 >= C.step do
		self:step(C.step)
		self.debt = math.max(0, self.debt - C.step)
	end
end

return Solo
