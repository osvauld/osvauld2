-- Physics playground: a top-down table where Rapier does all the moving. Lua describes bodies,
-- hears pockets and drags, and commands velocities; nothing runs per frame. The world has no
-- gravity, so "falling" is a launched velocity and pegs are fixed colliders.
local C = {
	bg = "#12131c", felt = "#1f5c46", rim = "#4a2f1d", pocket = "#0a0b10", peg = "#e9a23b",
	cue = "#f4f1e8", text = "#e7e9f3", muted = "#8a8fa8", button = "#2c3048", button_hover = "#3a3f5e",
	aim = "#f4f1e8",
}
local W, H, T = 880, 500, 14 -- the table, wall thickness
local POCKET = 30
local CUE_POS = { 150, H / 2 - 14 }

-- Park-Miller, so a recording of the same clicks is the same chaos.
local seed = 20261018
local function rnd()
	seed = (seed * 16807) % 2147483647
	return seed / 2147483647
end

local function disc(r, fill, stroke)
	local k = r * 0.5523
	return { path = { {"move",0,r}, {"cubic",0,r-k,r-k,0,r,0}, {"cubic",r+k,0,2*r,r-k,2*r,r},
		{"cubic",2*r,r+k,r+k,2*r,r,2*r}, {"cubic",r-k,2*r,0,r+k,0,r}, {"close"} },
		fill = fill, stroke = stroke }
end
local function rect(w, h, fill)
	return { path = { {"move",0,0}, {"line",w,0}, {"line",w,h}, {"line",0,h}, {"close"} }, fill = fill }
end
local function drawing(w, h, shapes)
	return gfx.drawing({ size = { w, h }, parts = { { id = "it", pivot = { w / 2, h / 2 }, shapes = shapes } } })
end

-- A ball carries a light tick so its spin is visible; drawings are made once per size and colour.
local balls_cache = {}
local function ball_drawing(r, fill)
	local key = r .. fill
	if not balls_cache[key] then
		local tick = { path = { {"move",r-1.5,2}, {"line",r+1.5,2}, {"line",r+1.5,r*0.7}, {"line",r-1.5,r*0.7},
			{"close"} }, fill = "#ffffff55" }
		balls_cache[key] = drawing(2 * r, 2 * r, { disc(r, fill, { 1.5, "#00000055" }), tick })
	end
	return balls_cache[key]
end

local KINDS = {
	{ r = 9, bounce = 0.98, friction = 0.08, colors = { "#ff6b6b", "#ffd166", "#ff9f43" } }, -- quick and bouncy
	{ r = 14, bounce = 0.85, friction = 0.25, colors = { "#4dabf7", "#63e6be", "#b197fc" } },
	{ r = 22, bounce = 0.45, friction = 0.8, colors = { "#868e96", "#5c677d" } }, -- heavy, dies fast
}

local function block(id, x, y, w, h)
	return { id = id, pos = { x, y }, drawing = drawing(w, h, { rect(w, h, C.rim) }), collider = { rect = { w, h } } }
end
local function peg(id, cx, cy, r)
	return { id = id, pos = { cx - r, cy - r }, drawing = drawing(2 * r, 2 * r, { disc(r, C.peg, { 3, "#fff3" }) }),
		collider = { circle = r, at = { r, r } } }
end
-- Corner pockets: the sensor reaches past the drawn hole so a ball at the wall still counts.
local function pocket(id, cx, cy)
	local R = POCKET
	return { id = id, pos = { cx - R, cy - R }, drawing = drawing(2 * R, 2 * R, { disc(R, C.pocket) }),
		sensor = { circle = R + 8, at = { R, R } } }
end

local balls = {} -- { id, x, y, r, bounce, friction, color }
local launches = {} -- { id, vx, vy }, set once the entities exist
local serial = 0
local pocketed, scratches = 0, 0
local aim = nil -- { x, y } where the cue was grabbed

local function add_ball(kind, x, y, color)
	serial = serial + 1
	local id = "b" .. serial
	balls[#balls + 1] = { id = id, x = x, y = y, r = kind.r, bounce = kind.bounce, friction = kind.friction,
		color = color or kind.colors[1 + math.floor(rnd() * #kind.colors)] }
	return id
end

local function clear()
	balls, launches, pocketed, scratches = {}, {}, 0, 0
	world("table"):cancel("launch")
end

-- Fifteen same-size balls in a triangle: one break shot scatters them.
local function rack()
	clear()
	local kind = KINDS[2]
	local d = 2 * kind.r + 1
	local palette = { "#ff6b6b", "#ffd166", "#4dabf7", "#63e6be", "#b197fc", "#ff9f43", "#f783ac" }
	for row = 0, 4 do
		for i = 0, row do
			local n = #balls
			add_ball(kind, 640 + row * d * 0.87, H / 2 - row * d / 2 + i * d - kind.r, palette[1 + n % #palette])
		end
	end
	world("table"):set("cue", { pos = CUE_POS, velocity = { 0, 0 } })
end

-- A volley from the top edge, aimed down at the pegs: mixed sizes, speeds and bounce.
local function burst()
	for i = 0, 13 do
		local kind = KINDS[1 + math.floor(rnd() * #KINDS)]
		local id = add_ball(kind, 250 + (i % 7) * 60, 40 + math.floor(i / 7) * 50)
		launches[#launches + 1] = { id = id, vx = (rnd() - 0.5) * 500, vy = 350 + rnd() * 450 }
	end
	world("table"):after(0.03, "launch")
end

local function button(id, label, on_click)
	return ui.button({
		id = id, px = 18, py = 9, radius = 8, fill = C.button, hover_fill = C.button_hover, press_scale = 0.97,
		ui.text({ label, color = C.text, font_size = 14, no_wrap = true }),
		on_click = on_click,
	})
end

local PIPS = 6
local function hide_aim()
	aim = nil
	for i = 1, PIPS do world("table"):set("pip" .. i, { pos = { -50, -50 } }) end
end

local pip = drawing(8, 8, { disc(4, C.aim) })

return function()
	local entities = {
		block("wall:top", 0, 0, W, T), block("wall:bottom", 0, H - T, W, T),
		block("wall:left", 0, 0, T, H), block("wall:right", W - T, 0, T, H),
		pocket("pocket:tl", 0, 0), pocket("pocket:tr", W, 0), pocket("pocket:bl", 0, H), pocket("pocket:br", W, H),
		peg("peg:mid", 440, 260, 26), peg("peg:tl", 350, 200, 18), peg("peg:tr", 530, 200, 18),
		peg("peg:bl", 350, 320, 18), peg("peg:br", 530, 320, 18),
		{ id = "cue", pos = CUE_POS, drawing = ball_drawing(14, C.cue),
			collider = { circle = 14, at = { 14, 14 } }, loose = { bounce = 0.92, friction = 0.3, grip = 0.25 } },
	}
	for _, b in ipairs(balls) do
		entities[#entities + 1] = { id = b.id, pos = { b.x - b.r, b.y - b.r }, drawing = ball_drawing(b.r, b.color),
			collider = { circle = b.r, at = { b.r, b.r } }, loose = { bounce = b.bounce, friction = b.friction, grip = 0.2 } }
	end
	for i = 1, PIPS do entities[#entities + 1] = { id = "pip" .. i, pos = { -50, -50 }, drawing = pip } end

	local table_el = {
		id = "table", width = W, height = H, fill = C.felt, radius = 10, stroke = { 3, C.rim },
		on_zone = function(e)
			if e.phase ~= "enter" then return end
			if e.who == "cue" then
				scratches = scratches + 1
				world("table"):set("cue", { pos = CUE_POS, velocity = { 0, 0 } })
				return
			end
			for i, b in ipairs(balls) do
				if b.id == e.who then
					table.remove(balls, i)
					pocketed = pocketed + 1
					break
				end
			end
		end,
		on_timer = function(e)
			if e.name ~= "launch" then return end
			for _, l in ipairs(launches) do world("table"):set(l.id, { velocity = { l.vx, l.vy } }) end
			launches = {}
		end,
		-- Pull the cue back like a slingshot: dots show the line it will take.
		on_drag = function(e)
			if e.phase == "start" then
				aim = e.shape == "cue" and { x = e.x, y = e.y } or nil
				return
			end
			if not aim then return end
			local px, py = aim.x - e.x, aim.y - e.y
			if e.phase == "move" then
				for i = 1, PIPS do
					world("table"):set("pip" .. i, { pos = { aim.x + px * i / PIPS - 4, aim.y + py * i / PIPS - 4 } })
				end
			else
				local len = math.sqrt(px * px + py * py)
				local k = len > 1400 / 6 and 1400 / len or 6
				if len > 6 then world("table"):set("cue", { velocity = { px * k, py * k } }) end
				hide_aim()
			end
		end,
	}
	for _, en in ipairs(entities) do table_el[#table_el + 1] = en end

	return ui.col({
		full = true, center = true, gap = 12, fill = C.bg,
		ui.row({
			gap = 24, align_center = true,
			ui.text({ "Physics playground", color = C.text, font_size = 22, no_wrap = true }),
			ui.text({ "pocketed " .. pocketed .. " · scratches " .. scratches, id = "score", color = C.muted,
				font_size = 14, no_wrap = true }),
		}),
		ui.world(table_el),
		ui.row({
			gap = 12,
			button("rack", "Rack", rack),
			button("burst", "Burst", burst),
			button("clear", "Clear", clear),
		}),
	})
end
