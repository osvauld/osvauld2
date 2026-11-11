-- The fight timeline (P-F13, S-F3): 1 800 ticks against the dummy at spawn 2 that start every
-- move, a function of the tick alone, as script.lua is. Segments are `{ ticks, x =, y =, hold =
-- jump held, J/D/K = a jump/dash/down press, L/H/O = a light/heavy/dodge press on the segment's
-- first tick, KH = K held (H holds it on its own segment) }`.
local S = { length = 1800 }

-- Walks and waits were found by a search that simulates each move against the dummy and keeps the first
-- approach that lands it (fighting step 6: stronger launches had moved the dummy out of the old timeline's
-- reach); the last shair rings it out, the closing sheavy swings at its respawn.
local SEGS = {
	{ 20, x = 1 }, { 1, x = 1, J = true, hold = true }, { 40, x = 1, hold = true }, { 90 },
	{ 1, y = 1, K = true }, { 40 }, { 10, x = -1 }, { 10 }, { 1, L = true }, { 45 },
	{ 1, x = -1, L = true }, { 50 }, { 20, x = -1 }, { 10 }, { 1, y = 1, L = true }, { 45, y = 1 },
	{ 20, x = -1 }, { 10 }, { 1, H = true }, { 75 }, { 20 }, { 22, x = -1 }, { 10 },
	{ 1, y = 1, H = true }, { 75, y = 1 }, { 20 }, { 80, x = -1 }, { 10 }, { 1, J = true }, { 16 },
	{ 1, y = 1, H = true }, { 80 }, { 1, J = true, hold = true }, { 40, hold = true }, { 70 },
	{ 3, x = 1 }, { 10 }, { 1, J = true }, { 2 }, { 1, L = true }, { 70 }, { 1, y = 1, K = true },
	{ 40 }, { 17 }, { 52, x = -1 }, { 3 }, { 2, x = 1 }, { 3 }, { 1, J = true }, { 2 },
	{ 1, x = 1, L = true }, { 74 }, { 82, x = 1 }, { 3 }, { 2, x = 1 }, { 3 }, { 1, J = true }, { 4 },
	{ 1, y = 1, L = true }, { 72 }, { 1, O = true }, { 30 }, { 26, x = 1 }, { 3 }, { 2, x = 1 }, { 3 },
	{ 1, J = true }, { 10 }, { 1, H = true }, { 12, KH = true }, { 5, x = -1, KH = true }, { 4 },
	{ 1, O = true }, { 69 }, { 1, x = 1, O = true }, { 32 }, { 6, x = 1 }, { 3 }, { 2, x = -1 }, { 3 },
	{ 1, J = true }, { 24 }, { 1, x = -1, H = true }, { 64 }, { 1, x = 1, H = true }, { 72 },
}

local inputs = {}
do
	local c = { jump = 0, dash = 0, down = 0, light = 0, heavy = 0, dodge = 0 }
	local t = 0
	for _, seg in ipairs(SEGS) do
		for i = 1, seg[1] do
			t = t + 1
			if i == 1 then
				if seg.J then c.jump = c.jump + 1 end
				if seg.D then c.dash = c.dash + 1 end
				if seg.K then c.down = c.down + 1 end
				if seg.L then c.light = c.light + 1 end
				if seg.H then c.heavy = c.heavy + 1 end
				if seg.O then c.dodge = c.dodge + 1 end
			end
			inputs[t] = { x = seg.x or 0, y = seg.y or 0, jump = seg.hold or false, heavy = seg.H or seg.KH or false,
				jump_n = c.jump, down_n = c.down, dash_n = c.dash, light_n = c.light, heavy_n = c.heavy, dodge_n = c.dodge }
		end
	end
	assert(t <= S.length, "script_fight: " .. t .. " ticks")
	local last = inputs[t]
	for i = t + 1, S.length do inputs[i] = last end
end

-- Tick t's input (1-based), a fresh table. After the last segment the last input is held.
function S.at(t)
	local i = inputs[t < 1 and 1 or t > S.length and S.length or t]
	return { x = i.x, y = i.y, jump = i.jump, heavy = i.heavy, jump_n = i.jump_n, down_n = i.down_n, dash_n = i.dash_n,
		light_n = i.light_n, heavy_n = i.heavy_n, dodge_n = i.dodge_n, restart_n = 0 }
end

return S
