-- Held keys into the match's input. A press is a count (fighter.lua acts when one grows), so an
-- input used for several ticks, or again after a rewind, acts once per press.
local I = {}
I.__index = I

local LEFT = { KeyA = true, ArrowLeft = true }
local RIGHT = { KeyD = true, ArrowRight = true }
local UP = { KeyW = true, ArrowUp = true }
local DOWN = { KeyS = true, ArrowDown = true }
local JUMP = { Space = true }
local DASH = { ShiftLeft = true, ShiftRight = true } -- Shift alone: K is heavy (brawl-fighting.md decision 5)
local LIGHT = { KeyJ = true }
local HEAVY = { KeyK = true }
local DODGE = { KeyL = true }
local RESTART = { Enter = true, NumpadEnter = true }

function I.new()
	return setmetatable({ held = {}, jump_n = 0, down_n = 0, dash_n = 0, restart_n = 0, light_n = 0, heavy_n = 0,
		dodge_n = 0, pct_n = 0, pct = 0 }, I)
end

-- An `on_key` event. A cancel (blur, surface change) lets every key go.
function I:key(e)
	if e.cancelled then
		self.held = {}
		return
	end
	local code = e.code
	if not code then return end
	if e.down then
		if not e.repeated and not self.held[code] then
			if JUMP[code] then self.jump_n = self.jump_n + 1 end
			if DOWN[code] then self.down_n = self.down_n + 1 end
			if DASH[code] then self.dash_n = self.dash_n + 1 end
			if RESTART[code] then self.restart_n = self.restart_n + 1 end
			if LIGHT[code] then self.light_n = self.light_n + 1 end
			if HEAVY[code] then self.heavy_n = self.heavy_n + 1 end
			if DODGE[code] then self.dodge_n = self.dodge_n + 1 end
		end
		self.held[code] = true
	else
		self.held[code] = nil
	end
end

local function any(held, set)
	for code in pairs(set) do
		if held[code] then return true end
	end
	return false
end

-- Training: set every dummy's damage to `pct` (a press, like the keys).
function I:train(pct)
	self.pct_n, self.pct = self.pct_n + 1, pct
end

-- This tick's input: a fresh plain table.
function I:take()
	local h = self.held
	return {
		x = (any(h, RIGHT) and 1 or 0) - (any(h, LEFT) and 1 or 0),
		y = (any(h, DOWN) and 1 or 0) - (any(h, UP) and 1 or 0),
		jump = any(h, JUMP), heavy = any(h, HEAVY),
		jump_n = self.jump_n, down_n = self.down_n, dash_n = self.dash_n, restart_n = self.restart_n,
		light_n = self.light_n, heavy_n = self.heavy_n, dodge_n = self.dodge_n, pct_n = self.pct_n, pct = self.pct,
	}
end

return I
