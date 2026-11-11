-- Held keys into the match's input. A press is a count (fighter.lua acts when one grows), so an
-- input used for several ticks, or again after a rewind, acts once per press.
local I = {}
I.__index = I

local LEFT = { KeyA = true, ArrowLeft = true }
local RIGHT = { KeyD = true, ArrowRight = true }
local UP = { KeyW = true, ArrowUp = true }
local DOWN = { KeyS = true, ArrowDown = true }
local JUMP = { Space = true }
local DASH = { ShiftLeft = true, ShiftRight = true, KeyK = true }
local RESTART = { Enter = true, NumpadEnter = true }

function I.new()
	return setmetatable({ held = {}, jump_n = 0, down_n = 0, dash_n = 0, restart_n = 0 }, I)
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

-- This tick's input: a fresh plain table.
function I:take()
	local h = self.held
	return {
		x = (any(h, RIGHT) and 1 or 0) - (any(h, LEFT) and 1 or 0),
		y = (any(h, DOWN) and 1 or 0) - (any(h, UP) and 1 or 0),
		jump = any(h, JUMP),
		jump_n = self.jump_n, down_n = self.down_n, dash_n = self.dash_n, restart_n = self.restart_n,
	}
end

return I
