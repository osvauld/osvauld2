-- Table hockey for two devices (jetpack-mp.md M6): a `net.match` driving a `ui.world`. Each
-- player steers their own paddle; the platform exchanges inputs, rewinds the rink on a late one,
-- and hashes it with the score every second. The rink is a function of the match's state, so a
-- rewind puts the puck back exactly where it was.
local C = {
	bg = "#12131c", ice = "#dfeef4", line = "#c0485a", rim = "#3d5a80", net = "#9fb3c8",
	left = "#2f80ed", right = "#eb5757", puck = "#1d1d24", text = "#e7e9f3", muted = "#8a8fa8",
}
local W, H = 900, 500
local L, T = 40, 16
local MOUTH_TOP, MOUTH_BOTTOM = 170, 330
local PADDLE, PUCK = 28, 16
local SPOT = { W / 2 - PUCK, H / 2 - PUCK }
local TICK = 1 / 120
local PAUSE, DROP = 180, 60 -- ticks a goal is shown, ticks before a new puck is in play
local WIN = 5

local function rect(w, h, fill)
	return { path = { {"move",0,0}, {"line",w,0}, {"line",w,h}, {"line",0,h}, {"close"} }, fill = fill }
end
local function disc(r, fill, stroke)
	local k = r * 0.5523
	return { path = { {"move",0,r}, {"cubic",0,r-k,r-k,0,r,0}, {"cubic",r+k,0,2*r,r-k,2*r,r},
		{"cubic",2*r,r+k,r+k,2*r,r,2*r}, {"cubic",r-k,2*r,0,r+k,0,r}, {"close"} },
		fill = fill, stroke = stroke }
end
local function drawing(w, h, shapes)
	return gfx.drawing({ size = { w, h }, parts = { { id = "it", pivot = { 0, 0 }, shapes = shapes } } })
end
local function block(id, x, y, w, h, fill)
	return { id = id, pos = { x, y }, drawing = drawing(w, h, { rect(w, h, fill or C.rim) }),
		collider = { rect = { w, h } } }
end
local function goal(id, x)
	local w, h = L - 8, MOUTH_BOTTOM - MOUTH_TOP
	return { id = id, pos = { x, MOUTH_TOP }, drawing = drawing(w, h, { rect(w, h, C.net) }),
		sensor = { rect = { w, h } } }
end
local markings = drawing(W, H, {
	{ path = { {"move",W/2-2,0}, {"line",W/2+2,0}, {"line",W/2+2,H}, {"line",W/2-2,H}, {"close"} },
		fill = C.line },
})
local KEYS = { neg = "ArrowLeft", pos = "ArrowRight" } -- never read: a driven world takes no keys
local function paddle(id, x, color)
	return { id = id, pos = { x - PADDLE, H / 2 - PADDLE },
		drawing = drawing(2 * PADDLE, 2 * PADDLE, { disc(PADDLE, color, { 3, C.puck }) }),
		collider = { circle = PADDLE, at = { PADDLE, PADDLE } }, group = "paddle",
		controller = { speed = 420, axis_x = KEYS } }
end
local puck_drawing = drawing(2 * PUCK, 2 * PUCK, { disc(PUCK, C.puck) })

local keys = ui.state("keys", { left = false, right = false, up = false, down = false })
local mode = ui.state("mode", { scripted = false })
local match

local function held(k)
	return keys[k] and 1 or 0
end

local function puck_id(s)
	return "puck:" .. s.gen
end

-- The rink at a state: everything fixed, the paddles, and the puck once it is in play.
local function rink(s)
	local live = s.t >= s.live_at and not s.winner
	return {
		{ id = "markings", pos = { 0, 0 }, drawing = markings },
		{ id = "centre", pos = { W / 2 - 2, 0 }, drawing = drawing(4, H, {}),
			collider = { rect = { 4, H } }, blocks = { "paddle" } },
		goal("goal:left", 8), goal("goal:right", W - L),
		block("wall:top", L, 0, W - 2 * L, T),
		block("wall:bottom", L, H - T, W - 2 * L, T),
		block("wall:left_top", 0, 0, L + T, MOUTH_TOP, C.bg),
		block("wall:left_bottom", 0, MOUTH_BOTTOM, L + T, H - MOUTH_BOTTOM, C.bg),
		block("wall:right_top", W - L - T, 0, L + T, MOUTH_TOP, C.bg),
		block("wall:right_bottom", W - L - T, MOUTH_BOTTOM, L + T, H - MOUTH_BOTTOM, C.bg),
		block("net:left", 0, MOUTH_TOP, 8, MOUTH_BOTTOM - MOUTH_TOP),
		block("net:right", W - 8, MOUTH_TOP, 8, MOUTH_BOTTOM - MOUTH_TOP),
		paddle("paddle:1", 150, C.left),
		paddle("paddle:2", W - 150, C.right),
		live and { id = puck_id(s), pos = SPOT, drawing = puck_drawing,
			collider = { circle = PUCK, at = { PUCK, PUCK } },
			loose = { bounce = 0.95, friction = 0.2, grip = 0.3 } } or false,
	}
end

-- Scripted play, a function of the frame alone so every run feeds the same inputs: every four
-- seconds a straight run at the centre spot, a retreat, then wandering on each player's own beat.
local function scripted()
	local f, me = match.frame, match.me
	local toward = me == 1 and 1 or -1
	local phase = f % 120
	if phase < 40 then
		return { dx = toward, dy = 0 }
	elseif phase < 70 then
		return { dx = -toward, dy = ((f // 10 + me) % 3) - 1 }
	end
	return { dx = (f // 15 + me) % 2 == 0 and toward or -toward, dy = ((f // 20 + me) % 3) - 1 }
end

local function start()
	match = net.match({
		id = "hockey",
		tick = TICK,
		frame = 4,
		players = net.peers(),
		setup = {},
		state = function()
			return { t = 0, gen = 1, live_at = DROP, score = { 0, 0 } }
		end,
		input = function()
			if mode.scripted then
				return scripted()
			end
			return { dx = held("right") - held("left"), dy = held("down") - held("up") }
		end,
		step = function(s, inputs, events)
			s.t += 1
			for i = 1, 2 do
				local input = inputs[i]
				if input then
					world("rink"):set("paddle:" .. i, { move = { input.dx, input.dy } })
				end
			end
			for _, e in ipairs(events) do
				if e.kind == "zone" and e.phase == "enter" and e.who == puck_id(s) and not s.scored_at then
					local side = e.id == "goal:left" and 2 or 1
					s.score[side] += 1
					s.scored_at = s.t
					if s.score[side] >= WIN then
						s.winner = side
					end
				end
			end
			if s.scored_at and s.t - s.scored_at >= PAUSE and not s.winner then
				s.gen += 1
				s.live_at = s.t + DROP
				s.scored_at = nil
			end
		end,
		worlds = { rink = rink },
	})
end

local ARROWS = { ArrowLeft = "left", ArrowRight = "right", ArrowUp = "up", ArrowDown = "down",
	KeyA = "left", KeyD = "right", KeyW = "up", KeyS = "down" }

return function()
	local s = match and match.state
	local score = s and (s.score[1] .. "  :  " .. s.score[2]) or "0  :  0"
	local checks = {}
	if match then
		for _, c in ipairs(match.checks) do
			checks[#checks + 1] = c.frame .. "=" .. c.hash
		end
	end
	return ui.col({
		id = "hockey", full = true, center = true, gap = 10, fill = C.bg,
		on_key = function(e)
			local k = ARROWS[e.code or ""]
			if k then
				keys[k] = e.down
			end
		end,
		ui.row({
			gap = 8,
			ui.button({ "start", id = "start", on_click = start }),
			ui.button({ mode.scripted and "scripted" or "keys", id = "script", on_click = function()
				mode.scripted = not mode.scripted
			end }),
		}),
		ui.text({ score, id = "score", color = C.text, font_size = 28, no_wrap = true }),
		ui.world({ id = "rink", driven = "hockey", width = W, height = H, fill = C.ice, radius = 12,
			stroke = { 3, C.rim } }),
		ui.text({ match and ("status " .. match.status .. " frame " .. match.frame) or "not started",
			id = "status", color = C.muted, no_wrap = true }),
		ui.text({ "checks " .. table.concat(checks, " "), id = "checks", color = C.muted, font_size = 10 }),
		ui.text({ "desynced " .. (match and table.concat(match.desynced, ",") or ""), id = "desynced",
			color = C.muted, no_wrap = true }),
	})
end
