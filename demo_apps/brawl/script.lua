-- The scripted timeline (S-9, P-M12): 1 200 ticks of input that use every move once or more, a
-- function of the tick alone so every run feeds the same inputs. Segments are `{ ticks, x =, y =,
-- hold = jump held, J/D/K/R = a jump/dash/down/restart press on the segment's first tick }`.
-- Plan: docs/design/brawl-movement.md §0.
local S = { length = 1200 }

local SEGS = {
	{ 40, x = 1 }, -- run right
	{ 30, x = -1 }, -- reverse: a skid, then run left
	{ 20 }, -- stop
	{ 1, J = true }, { 59 }, -- a tap: the short hop
	{ 1, J = true, hold = true }, { 14, hold = true }, -- full jump
	{ 1, J = true, hold = true }, { 12, hold = true }, -- the double jump
	{ 36 }, { 1, K = true, y = 1 }, { 18, y = 1 }, -- fast fall
	{ 1, J = true }, { 66 }, -- a press 4 ticks before landing: buffered, then a short hop
	{ 1, D = true, x = 1 }, { 12, x = 1 }, -- ground dash right
	{ 1, D = true, x = -1 }, { 12, x = -1 }, -- and left
	{ 1, J = true, hold = true }, { 8, hold = true }, -- jump, then an air dash up and right
	{ 1, D = true, x = 1, y = -1, hold = true }, { 25, x = 1 },
	{ 12 },
	{ 51, x = 1 }, -- run off the right platform
	{ 1, J = true, hold = true, x = 1 }, { 20, hold = true, x = -1 }, -- 5 ticks late: still the ground jump
	{ 241, x = -1 }, -- fly left, land, run left off the edge
	{ 110, x = 1 }, -- turn back to the wall and slide down it
	{ 1, J = true, hold = true, x = 1 }, { 48, hold = true, x = 1 }, -- wall jump away and back to the wall
	{ 1, J = true, hold = true, x = 1 }, { 10, hold = true, x = -1 }, -- and again, then let it go
	{ 40, x = -1 },
	{ 1, K = true, x = -1, y = 1 }, { 300, x = -1, y = 1 }, -- fast fall out of the stage
}

local inputs = {}
do
	local c = { jump = 0, dash = 0, down = 0, restart = 0 }
	local t = 0
	for _, seg in ipairs(SEGS) do
		for i = 1, seg[1] do
			t = t + 1
			if i == 1 then
				if seg.J then c.jump = c.jump + 1 end
				if seg.D then c.dash = c.dash + 1 end
				if seg.K then c.down = c.down + 1 end
				if seg.R then c.restart = c.restart + 1 end
			end
			inputs[t] = { x = seg.x or 0, y = seg.y or 0, jump = seg.hold or false,
				jump_n = c.jump, down_n = c.down, dash_n = c.dash, restart_n = c.restart }
		end
	end
	assert(t <= S.length, "script: " .. t .. " ticks")
	local last = inputs[t]
	for i = t + 1, S.length do inputs[i] = last end
end

-- Tick t's input (1-based), a fresh table. After the last segment the last input is held.
function S.at(t)
	local i = inputs[t < 1 and 1 or t > S.length and S.length or t]
	return { x = i.x, y = i.y, jump = i.jump, jump_n = i.jump_n, down_n = i.down_n, dash_n = i.dash_n, restart_n = i.restart_n }
end

return S
