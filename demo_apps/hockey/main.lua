-- Table hockey: two paddles walk, the puck is loose. Rust moves everything every frame; Lua only
-- hears that the puck went into a goal, keeps the score and puts a new puck on the spot.
local C = {
	bg = "#12131c", ice = "#dfeef4", line = "#c0485a", rim = "#3d5a80", net = "#9fb3c8",
	left = "#2f80ed", right = "#eb5757", puck = "#1d1d24", text = "#e7e9f3", muted = "#8a8fa8",
	button = "#2c3048", button_hover = "#3a3f5e",
}
local W, H = 900, 500 -- the world
local L, T = 40, 16 -- goal depth behind each end wall, wall thickness
local MOUTH_TOP, MOUTH_BOTTOM = 170, 330

local function rect(w, h, fill)
	return { path = { {"move",0,0}, {"line",w,0}, {"line",w,h}, {"line",0,h}, {"close"} }, fill = fill }
end
-- A circle as four cubics, in an r-by-r box's own units.
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
-- A goal: the pocket behind a mouth, its zone reporting what comes in.
local function goal(id, x)
	local w, h = L - 8, MOUTH_BOTTOM - MOUTH_TOP
	return { id = id, pos = { x, MOUTH_TOP }, drawing = drawing(w, h, { rect(w, h, C.net) }),
		sensor = { rect = { w, h } } }
end

-- The painted ice: centre line and circle. No collider, so nothing feels it.
local markings = drawing(W, H, {
	{ path = { {"move",W/2-2,0}, {"line",W/2+2,0}, {"line",W/2+2,H}, {"line",W/2-2,H}, {"close"} },
		fill = C.line },
})
local spot = drawing(120, 120, { disc(60, nil, { 3, C.line }) })

local PADDLE, PUCK = 28, 16
local function paddle(id, x, color, keys)
	return { id = id, pos = { x - PADDLE, H / 2 - PADDLE },
		drawing = drawing(2 * PADDLE, 2 * PADDLE, { disc(PADDLE, color, { 3, C.puck }) }),
		collider = { circle = PADDLE, at = { PADDLE, PADDLE } }, group = "paddle",
		controller = { speed = 420, axis_x = { neg = keys[1], pos = keys[2] },
			axis_y = { neg = keys[3], pos = keys[4] } } }
end
local puck_drawing = drawing(2 * PUCK, 2 * PUCK, { disc(PUCK, C.puck) })

local score = { left = 0, right = 0 }
local pucks = 1 -- a goal puts down a fresh puck: a new id spawns at the centre spot

local function button(id, label, on_click)
	return ui.button({
		id = id, px = 14, py = 8, radius = 8, fill = C.button, hover_fill = C.button_hover,
		ui.text({ label, color = C.text, font_size = 13, no_wrap = true }),
		on_click = on_click,
	})
end

return function()
	return ui.col({
		full = true, center = true, gap = 14, fill = C.bg,
		ui.text({ score.left .. "  :  " .. score.right, color = C.text, font_size = 28, no_wrap = true }),
		ui.text({ "Blue: WASD · Red: arrow keys · hit the puck into the other goal", color = C.muted,
			font_size = 13, no_wrap = true }),
		ui.world({
			id = "rink", width = W, height = H, fill = C.ice, radius = 12, stroke = { 3, C.rim },
			-- A puck in a goal: score for the other side and face off again.
			on_zone = function(e)
				if e.phase ~= "enter" or e.who ~= "puck:" .. pucks then return end
				if e.id == "goal:left" then
					score.right = score.right + 1
				else
					score.left = score.left + 1
				end
				pucks = pucks + 1
			end,
			{ id = "markings", pos = { 0, 0 }, drawing = markings },
			{ id = "spot", pos = { W / 2 - 60, H / 2 - 60 }, drawing = spot },
			-- Each paddle keeps to its own half: the centre line stops paddles, and only paddles.
			{ id = "centre", pos = { W / 2 - 2, 0 }, drawing = drawing(4, H, {}),
				collider = { rect = { 4, H } }, blocks = { "paddle" } },
			goal("goal:left", 8), goal("goal:right", W - L),
			block("wall:top", L, 0, W - 2 * L, T),
			block("wall:bottom", L, H - T, W - 2 * L, T),
			-- The end walls fill the corners behind them, so a pocket is closed but for its mouth.
			block("wall:left_top", 0, 0, L + T, MOUTH_TOP, C.bg),
			block("wall:left_bottom", 0, MOUTH_BOTTOM, L + T, H - MOUTH_BOTTOM, C.bg),
			block("wall:right_top", W - L - T, 0, L + T, MOUTH_TOP, C.bg),
			block("wall:right_bottom", W - L - T, MOUTH_BOTTOM, L + T, H - MOUTH_BOTTOM, C.bg),
			block("net:left", 0, MOUTH_TOP, 8, MOUTH_BOTTOM - MOUTH_TOP),
			block("net:right", W - 8, MOUTH_TOP, 8, MOUTH_BOTTOM - MOUTH_TOP),
			paddle("blue", 150, C.left, { "KeyA", "KeyD", "KeyW", "KeyS" }),
			paddle("red", W - 150, C.right, { "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown" }),
			{ id = "puck:" .. pucks, pos = { W / 2 - PUCK, H / 2 - PUCK }, drawing = puck_drawing,
				collider = { circle = PUCK, at = { PUCK, PUCK } }, loose = { bounce = 0.95, friction = 0.2 } },
		}),
		button("reset", "new game", function()
			score.left, score.right, pucks = 0, 0, pucks + 1 -- and a fresh puck on the spot
		end),
	})
end
