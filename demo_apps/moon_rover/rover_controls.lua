local Controls = {}
Controls.__index = Controls
local actions = {KeyW="gas",KeyS="reverse",KeyA="left",KeyD="right",Space="brake"}

function Controls.new()
	return setmetatable({keys = {}, pointer = {}}, Controls)
end

function Controls:clear()
	self.keys, self.pointer = {}, {}
end

function Controls:key(e)
	if e.cancelled then self:clear(); return end
	if e.repeated or not actions[e.code] then return end
	self.keys[actions[e.code]] = e.down or nil
end

function Controls:hover(action, e)
	self.pointer[action] = e.phase ~= "leave" and e.down or nil
end

function Controls:input()
	local function on(action) return (self.keys[action] or self.pointer[action]) and 1 or 0 end
	return {throttle = on("gas") - on("reverse"), steering = on("right") - on("left"), brake = on("brake")}
end

return Controls
