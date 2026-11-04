-- Held pedals from the keyboard (→/D gas, ←/A brake) and the on-screen pedals (a held primary
-- button over one). Gives drive.lua its `{ gas, brake }`; knows nothing of the car.
local K = {}
K.__index = K

local GAS = { ArrowRight = true, KeyD = true }
local BRAKE = { ArrowLeft = true, KeyA = true }

function K.new() return setmetatable({ keys = {}, held = {} }, K) end

-- Root `on_key`: a cancel (blur, focus moving) lets everything go.
function K:key(e)
	if e.cancelled then self.keys = {} return end
	if e.repeated or not e.code then return end
	if GAS[e.code] or BRAKE[e.code] then self.keys[e.code] = e.down or nil end
end

-- A pedal's `on_hover`: held while the button is down over it; leaving lets go.
function K:hover(pedal, e)
	self.held[pedal] = (e.phase ~= "leave" and e.down) or nil
end

function K:reset() self.keys, self.held = {}, {} end

local function any(keys, set)
	for code in pairs(keys) do if set[code] then return true end end
	return false
end

function K:state()
	return {
		gas = (self.held.gas or any(self.keys, GAS)) and 1 or 0,
		brake = (self.held.brake or any(self.keys, BRAKE)) and 1 or 0,
	}
end

return K
