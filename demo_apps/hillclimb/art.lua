-- The look: drawings for the car's bodies (sized from setup.lua's boxes), the pickups, the ground,
-- the wall at the start and the hills behind. Built once at module scope by main.lua, so the
-- handles are stable. Nothing here changes how anything moves.
local A = {}

A.colours = {
	sky = "#9fd3f0", sky_top = "#3d8fd6", sky_low = "#cdebf7", text = "#14202e", panel = "#ffffffcc", muted = "#4b5b6b",
	fuel = "#f0a000", fuel_low = "#e03030",
	pedal = "#2b2f3a", pedal_held = "#4a8ff0", pedal_text = "#ffffff",
	-- buggy
	body = "#e8533f", body_light = "#ff8a6e", body_dark = "#a83224", trim = "#f4f1ea",
	cage = "#2b2f3a", cage_shine = "#6b7280", metal = "#9aa3ad", metal_dark = "#4b525c",
	spring = "#f2c230", engine = "#3a3f48", light = "#fff6b0",
	jacket = "#2f6fd6", jacket_dark = "#1d4596", glove = "#22262e",
	-- wheels, head
	tyre = "#1d1f24", tyre_edge = "#0e0f12", tread = "#2c2f36", rim = "#d5dae1", rim_dark = "#7d8590",
	helmet = "#ffd23f", helmet_dark = "#d39a00", stripe = "#e8533f", visor = "#22324a",
	-- pickups
	can = "#d7262e", can_light = "#ff5a5f", can_dark = "#8e1218",
	coin = "#ffc93c", coin_light = "#fff1b0", coin_dark = "#c98a00",
	-- land
	grass = "#4caf50", grass_light = "#7fd36b", grass_dark = "#2f7d32",
	dirt_top = "#8a5f3a", dirt = "#6b4a2f", dirt_deep = "#3e2a1a", rock = "#8d8478", rock_dark = "#5c554c",
	hills = "#b9dcc0", hills_far = "#cfe7e6", cloud = "#ffffff", dust = "#b08a62cc",
}
local C = A.colours

local function rect(x, y, w, h)
	return { { "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } }
end

-- A circle as four cubics, appended to `into` (so many can share one path) or as a new path.
local function disc(cx, cy, r, into)
	local k = r * 0.5523
	local p = into or {}
	for _, c in ipairs({
		{ "move", cx, cy - r }, { "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy },
		{ "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r }, { "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
		{ "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r }, { "close" },
	}) do table.insert(p, c) end
	return p
end

-- A closed polygon from flat x, y pairs.
local function poly(pts)
	local p = { { "move", pts[1], pts[2] } }
	for i = 3, #pts, 2 do table.insert(p, { "line", pts[i], pts[i + 1] }) end
	table.insert(p, { "close" })
	return p
end

local function polyline(pts)
	local p = { { "move", pts[1], pts[2] } }
	for i = 3, #pts, 2 do table.insert(p, { "line", pts[i], pts[i + 1] }) end
	return p
end

local function vertical(y0, y1, stops)
	return { linear = { from = { 0, y0 }, to = { 0, y1 }, stops = stops } }
end

local function radial(cx, cy, r, stops)
	return { radial = { center = { cx, cy }, radius = r, stops = stops } }
end

local function one_part(w, h, shapes)
	return gfx.drawing({ size = { w, h }, parts = { { id = "it", pivot = { w / 2, h / 2 }, shapes = shapes } } })
end

-- A coil-over: spring zigzag round a damper, from the chassis mount down towards the axle.
local function shock(x, top, bottom, into)
	table.insert(into, { path = rect(x - 3, top, 6, bottom - top), fill = C.metal_dark })
	local coils, zig = 6, {}
	for i = 0, coils * 2 do
		local y = top + 3 + (bottom - top - 6) * i / (coils * 2)
		table.insert(zig, x + ((i % 2 == 0) and -6 or 6))
		table.insert(zig, y)
	end
	table.insert(into, { path = polyline(zig), stroke = { 2.5, C.spring } })
	table.insert(into, { path = rect(x - 5, top - 2, 10, 5), fill = C.metal })
end

-- A buggy seen from the side, front to the right: shocks behind, then the cage, the driver's
-- body (the head is its own body, on the neck), the tub, fenders over each wheel, the engine at
-- the back. `m` holds the axles and the neck in the chassis box's space.
local function chassis(w, h, m)
	local s = {}
	for _, a in ipairs(m.axles) do shock(a[1], h * 0.35, a[2] - 4, s) end

	-- roll cage: dark tube with a lit edge
	local hoop = { w * 0.33, h * 0.1, w * 0.37, -h * 1.5, w * 0.64, -h * 1.5, w * 0.78, h * 0.1 }
	local brace = { w * 0.37, -h * 1.5, w * 0.55, h * 0.1 }
	for _, line in ipairs({ hoop, brace }) do
		table.insert(s, { path = polyline(line), stroke = { 5, C.cage } })
		table.insert(s, { path = polyline(line), stroke = { 1.5, C.cage_shine } })
	end

	-- seat back, then the driver: torso, arm to the steering wheel
	local nx, ny = m.neck[1], m.neck[2]
	table.insert(s, { path = poly({ nx - 20, ny - 4, nx - 10, ny - 8, nx - 6, h * 0.4, nx - 18, h * 0.4 }), fill = C.engine })
	table.insert(s, {
		path = poly({ nx - 11, ny + 1, nx + 9, ny + 1, nx + 14, h * 0.45, nx - 10, h * 0.45 }),
		fill = vertical(ny, h * 0.45, { { 0, C.jacket }, { 1, C.jacket_dark } }), stroke = { 1.5, C.jacket_dark },
	})
	local wheel_at = { nx + 38, ny + 6 }
	table.insert(s, { path = polyline({ wheel_at[1] + 10, h * 0.35, wheel_at[1], wheel_at[2] }), stroke = { 3, C.metal_dark } })
	table.insert(s, { path = polyline({ nx + 4, ny + 6, nx + 22, ny + 18, wheel_at[1] - 2, wheel_at[2] + 1 }), stroke = { 7, C.jacket } })
	table.insert(s, { path = disc(wheel_at[1] - 2, wheel_at[2] + 1, 4), fill = C.glove })
	table.insert(s, { path = polyline({ wheel_at[1] - 6, wheel_at[2] - 9, wheel_at[1] + 5, wheel_at[2] + 9 }), stroke = { 4, C.cage } })

	-- engine and exhaust at the back
	table.insert(s, { path = rect(w * 0.05, -h * 0.35, w * 0.17, h * 0.5), fill = vertical(-h * 0.35, h * 0.15, { { 0, C.metal }, { 1, C.engine } }), stroke = { 1.5, C.metal_dark } })
	for i = 0, 3 do
		local x = w * 0.07 + i * w * 0.035
		table.insert(s, { path = polyline({ x, -h * 0.3, x, -h * 0.05 }), stroke = { 1.5, C.metal_dark } })
	end
	table.insert(s, { path = { { "move", w * 0.08, h * 0.15 }, { "quad", -w * 0.02, h * 0.12, -w * 0.04, -h * 0.2 } }, stroke = { 5, C.metal_dark } })
	table.insert(s, { path = disc(-w * 0.04, -h * 0.22, 3), fill = C.engine })

	-- the tub: lit from above, a dark sill, a white stripe
	table.insert(s, {
		path = poly({ 0, h * 0.15, w * 0.05, 0, w * 0.68, 0, w * 0.9, h * 0.18, w, h * 0.42, w * 0.98, h, w * 0.03, h }),
		fill = vertical(0, h, { { 0, C.body_light }, { 0.45, C.body }, { 1, C.body_dark } }), stroke = { 2, C.body_dark },
	})
	table.insert(s, { path = polyline({ w * 0.06, h * 0.08, w * 0.67, h * 0.08 }), stroke = { 2, "#ffffff66" } })
	table.insert(s, { path = rect(w * 0.04, h * 0.5, w * 0.93, h * 0.11), fill = C.trim })
	table.insert(s, { path = rect(w * 0.04, h * 0.78, w * 0.94, h * 0.22), fill = C.body_dark })
	for i = 0, 6 do table.insert(s, { path = disc(w * (0.1 + i * 0.13), h * 0.89, 1.6), fill = C.metal }) end

	-- headlight and rear light
	table.insert(s, { path = poly({ w * 0.93, h * 0.24, w * 0.99, h * 0.36, w * 0.98, h * 0.48, w * 0.93, h * 0.44 }), fill = C.light, stroke = { 1.5, C.metal_dark } })
	table.insert(s, { path = rect(w * 0.005, h * 0.25, w * 0.02, h * 0.18), fill = "#ff3b30" })

	-- fenders: a crescent over each wheel
	for _, a in ipairs(m.axles) do
		local x, r = a[1], (a.r or 32) + 7
		table.insert(s, {
			path = {
				{ "move", x - r - 4, h * 0.95 }, { "cubic", x - r, h * 0.95 - r * 1.15, x + r, h * 0.95 - r * 1.15, x + r + 4, h * 0.95 },
				{ "line", x + r - 3, h * 0.95 }, { "cubic", x + r - 6, h * 0.95 - r * 0.95, x - r + 6, h * 0.95 - r * 0.95, x - r + 3, h * 0.95 },
				{ "close" },
			},
			fill = vertical(h * 0.95 - r, h * 0.95, { { 0, C.cage_shine }, { 1, C.cage } }),
		})
	end
	return one_part(w, h, s)
end

-- A knobbly tyre, an alloy rim, spokes and hub: a turning wheel shows that it turns.
local function wheel(d)
	local r = d / 2
	local s = {}
	local knobs = {}
	for k = 0, 17 do
		local a, half = k * math.pi * 2 / 18, 0.07
		local function at(rr, da) return r + math.cos(a + da) * rr, r + math.sin(a + da) * rr end
		local x1, y1 = at(r * 0.88, -half)
		local x2, y2 = at(r * 1.0, -half * 0.8)
		local x3, y3 = at(r * 1.0, half * 0.8)
		local x4, y4 = at(r * 0.88, half)
		for _, c in ipairs({ { "move", x1, y1 }, { "line", x2, y2 }, { "line", x3, y3 }, { "line", x4, y4 }, { "close" } }) do
			table.insert(knobs, c)
		end
	end
	table.insert(s, { path = knobs, fill = C.tyre_edge })
	table.insert(s, { path = disc(r, r, r * 0.92), fill = radial(r, r, r * 0.92, { { 0.55, C.tread }, { 1, C.tyre } }) })
	table.insert(s, { path = disc(r, r, r * 0.78), stroke = { 1.5, "#3a3e46" } })
	table.insert(s, { path = disc(r, r, r * 0.6), fill = radial(r * 0.8, r * 0.75, r * 0.7, { { 0, "#ffffff" }, { 0.4, C.rim }, { 1, C.rim_dark } }), stroke = { 2, C.tyre_edge } })
	for k = 0, 4 do
		local a = k * math.pi * 2 / 5
		local dx, dy = math.cos(a), math.sin(a)
		table.insert(s, {
			path = poly({ r + dx * r * 0.16 - dy * 3, r + dy * r * 0.16 + dx * 3, r + dx * r * 0.55 - dy * 2, r + dy * r * 0.55 + dx * 2,
				r + dx * r * 0.55 + dy * 2, r + dy * r * 0.55 - dx * 2, r + dx * r * 0.16 + dy * 3, r + dy * r * 0.16 - dx * 3 }),
			fill = C.rim_dark,
		})
	end
	table.insert(s, { path = disc(r, r, r * 0.2), fill = C.metal_dark, stroke = { 1.5, C.tyre_edge } })
	local nuts = {}
	for k = 0, 4 do
		local a = k * math.pi * 2 / 5 + 0.6
		disc(r + math.cos(a) * r * 0.11, r + math.sin(a) * r * 0.11, 1.4, nuts)
	end
	table.insert(s, { path = nuts, fill = C.rim })
	return one_part(d, d, s)
end

-- A helmet facing right: lit dome, a stripe, a visor with a glint, a chin guard.
local function head(d)
	local r = d / 2
	return one_part(d, d, {
		{ path = disc(r, r, r), fill = radial(r * 0.7, r * 0.6, r * 1.3, { { 0, "#fff3b8" }, { 0.45, C.helmet }, { 1, C.helmet_dark } }), stroke = { 1.5, C.dirt_deep } },
		{ path = { { "move", r * 0.25, r * 0.35 }, { "quad", r, -r * 0.15, r * 1.75, r * 0.35 } }, stroke = { 3, C.stripe } },
		{ path = { { "move", r * 0.95, r * 0.6 }, { "line", r * 1.95, r * 0.65 }, { "quad", r * 2.02, r * 0.95, r * 1.92, r * 1.25 }, { "line", r * 0.95, r * 1.2 }, { "close" } },
			fill = vertical(r * 0.6, r * 1.25, { { 0, "#3d5a80" }, { 1, C.visor } }) },
		{ path = { { "move", r * 1.15, r * 0.72 }, { "line", r * 1.65, r * 0.74 } }, stroke = { 1.5, "#ffffffaa" } },
		{ path = { { "move", r * 1.1, r * 1.45 }, { "quad", r * 1.6, r * 1.6, r * 1.85, r * 1.4 } }, stroke = { 2.5, C.helmet_dark } },
	})
end

-- A jerrycan: shaded body, a carry handle, the spout cap, the X pressed into its side.
local function fuel_can(w, h)
	return one_part(w, h, {
		{ path = rect(w * 0.12, h * 0.05, w * 0.42, h * 0.14), stroke = { 4, C.can_dark } },
		{ path = rect(w * 0.64, h * 0.02, w * 0.2, h * 0.16), fill = C.metal_dark, stroke = { 1.5, C.engine } },
		{ path = poly({ w * 0.08, h * 0.2, w * 0.92, h * 0.2, w * 0.95, h * 0.26, w * 0.95, h * 0.95, w * 0.05, h * 0.95, w * 0.05, h * 0.26 }),
			fill = { linear = { from = { 0, 0 }, to = { w, 0 }, stops = { { 0, C.can_dark }, { 0.35, C.can_light }, { 0.6, C.can }, { 1, C.can_dark } } } },
			stroke = { 2, C.can_dark } },
		{ path = { { "move", w * 0.22, h * 0.36 }, { "line", w * 0.78, h * 0.82 }, { "move", w * 0.78, h * 0.36 }, { "line", w * 0.22, h * 0.82 } }, stroke = { 3.5, C.can_dark } },
		{ path = polyline({ w * 0.14, h * 0.3, w * 0.14, h * 0.86 }), stroke = { 2, "#ffffff55" } },
	})
end

local function star(cx, cy, outer, inner)
	local pts = {}
	for k = 0, 9 do
		local a = -math.pi / 2 + k * math.pi / 5
		local rr = (k % 2 == 0) and outer or inner
		table.insert(pts, cx + math.cos(a) * rr)
		table.insert(pts, cy + math.sin(a) * rr)
	end
	return poly(pts)
end

local function coin(d)
	local r = d / 2
	return one_part(d, d, {
		{ path = disc(r, r, r), fill = radial(r * 0.7, r * 0.65, r * 1.2, { { 0, C.coin_light }, { 0.5, C.coin }, { 1, C.coin_dark } }), stroke = { 1.5, C.coin_dark } },
		{ path = disc(r, r, r * 0.72), stroke = { 1.5, C.coin_dark } },
		{ path = star(r, r, r * 0.5, r * 0.22), fill = C.coin_dark },
		{ path = { { "move", r * 0.45, r * 0.6 }, { "quad", r * 0.55, r * 0.35, r * 0.85, r * 0.3 } }, stroke = { 1.5, "#ffffffcc" } },
	})
end

-- Park–Miller again, so the ground's tufts and stones are the same every run.
local function rng(seed)
	local state = seed
	return function()
		state = (state * 16807) % 2147483647
		return state / 2147483647
	end
end

-- The ground under the stage's line, `points` already shifted into the drawing (y from its top):
-- dirt darkening with depth, a buried layer, stones, the grass edge and tufts on it.
local function ground(points, w, h)
	local rand = rng(12345)
	local outline = { { "move", points[1], h } }
	for i = 1, #points, 2 do table.insert(outline, { "line", points[i], points[i + 1] }) end
	table.insert(outline, { "line", points[#points - 1], h })
	table.insert(outline, { "close" })
	local function along(dy)
		local p = { { "move", points[1], points[2] + dy } }
		for i = 3, #points, 2 do table.insert(p, { "line", points[i], points[i + 1] + dy }) end
		return p
	end
	local stones, tufts = {}, {}
	for i = 1, #points - 2, 2 do
		local x, y = points[i], points[i + 1]
		if rand() < 0.35 then
			local rr = 4 + rand() * 9
			disc(x + rand() * 50, y + 30 + rand() * 220, rr, stones)
		end
		if rand() < 0.6 then
			local tx = x + rand() * 40
			local ty = y + (points[i + 3] - y) * (tx - x) / 50
			local hgt = 6 + rand() * 8
			for _, c in ipairs({ { "move", tx - 5, ty }, { "line", tx - 2, ty - hgt }, { "line", tx, ty - 1 }, { "line", tx + 3, ty - hgt * 0.8 }, { "line", tx + 5, ty }, { "close" } }) do
				table.insert(tufts, c)
			end
		end
	end
	return one_part(w, h, {
		{ path = outline, fill = vertical(0, h * 0.6, { { 0, C.dirt_top }, { 0.4, C.dirt }, { 1, C.dirt_deep } }) },
		{ path = along(90), stroke = { 26, "#00000018" } },
		{ path = along(200), stroke = { 14, "#00000014" } },
		{ path = stones, fill = C.rock, stroke = { 2, C.rock_dark } },
		{ path = along(6), stroke = { 10, C.grass_dark } },
		{ path = along(0), stroke = { 8, C.grass } },
		{ path = along(-3), stroke = { 2.5, C.grass_light } },
		{ path = tufts, fill = C.grass },
	})
end

-- Hills behind the road: two pale silhouettes riding a smoothed copy of the ground, and clouds.
local function backdrop(points, w, h)
	local rand = rng(777)
	local function ridge(lift, wave, period, colour)
		local p = { { "move", points[1], h } }
		for i = 1, #points, 8 do
			local x = points[i]
			table.insert(p, { "line", x, points[i + 1] - lift - wave * (0.5 + 0.5 * math.sin(x / period)) })
		end
		table.insert(p, { "line", points[#points - 1], h })
		table.insert(p, { "close" })
		return { path = p, fill = colour }
	end
	local clouds = {}
	for x = 400, w, 900 do
		local cx, cy = x + rand() * 500, points[math.max(2, math.floor(x / 50) * 2)] - 300 - rand() * 150
		for k = 0, 3 do disc(cx + k * 34, cy + ((k % 2 == 0) and 0 or -14), 26 + rand() * 10, clouds) end
	end
	return one_part(w, h, {
		{ path = clouds, fill = C.cloud },
		ridge(150, 90, 900, C.hills_far),
		ridge(70, 60, 520, C.hills),
	})
end

-- A rock face at the start: the road's left end, so nothing reverses off the world.
local function cliff(w, h)
	local rand = rng(99)
	local cracks = {}
	for k = 1, 14 do
		local x, y = 8 + rand() * (w - 16), rand() * h
		table.insert(cracks, { "move", x, y })
		table.insert(cracks, { "line", x + 6 - rand() * 12, y + 20 + rand() * 30 })
	end
	return one_part(w, h, {
		{ path = rect(0, 0, w, h), fill = { linear = { from = { 0, 0 }, to = { w, 0 }, stops = { { 0, C.rock_dark }, { 0.7, C.rock }, { 1, C.rock_dark } } } } },
		{ path = cracks, stroke = { 2, C.rock_dark } },
	})
end

A.PICKUP_R = { fuel = 26, coin = 15 } -- sensor radius, units
A.CLIFF = { 80, 2400 }                -- the wall at x < 0: width, height (units)

-- Every drawing the game needs, by entity id or item kind, plus the ground's placement.
-- `mounts` = { axles = { {x, y}, … }, neck = {x, y} } in the chassis box's space, units.
function A.build(sizes, stage, mounts)
	local looks = {
		chassis = chassis(sizes.chassis[1], sizes.chassis[2], mounts),
		head = head(sizes.head[1]),
		fuel = fuel_can(2 * A.PICKUP_R.fuel, 2 * A.PICKUP_R.fuel),
		coin = coin(2 * A.PICKUP_R.coin),
		cliff = cliff(A.CLIFF[1], A.CLIFF[2]),
		-- Under the rear tyre, in the chassis box: where its dust leaves from.
		dust_at = { mounts.axles[1][1], mounts.axles[1][2] + mounts.axles[1].r - 6 },
	}
	for id, size in pairs(sizes) do
		if id:sub(1, 5) == "wheel" then looks[id] = wheel(size[1]) end
	end
	-- The ground entity sits at the line's highest point, so its drawing's y never goes negative.
	-- Its numbers are the physics' too: the hills get their own origin rather than moving this one.
	local p, top, bottom = stage.points, math.huge, -math.huge
	for i = 2, #p, 2 do top, bottom = math.min(top, p[i]), math.max(bottom, p[i]) end
	top = top - 40
	local function shift(by)
		local out = {}
		for i = 1, #p, 2 do
			table.insert(out, p[i])
			table.insert(out, p[i + 1] - by)
		end
		return out
	end
	local width, depth = p[#p - 1], bottom - top + 800
	local shifted = shift(top)
	looks.ground = ground(shifted, width, depth)
	looks.ground_at = { 0, top }
	looks.ground_line = shifted
	local sky = 1200 -- room above the highest point for the hills and clouds
	looks.backdrop = backdrop(shift(top - sky), width, depth + sky)
	looks.backdrop_at = { 0, top - sky }
	looks.cliff_at = { -A.CLIFF[1], p[2] - A.CLIFF[2] + 400 }
	-- The camera stays over the ground: no sky past its ends, none below it.
	looks.bounds = { -A.CLIFF[1], top - 1500, width + A.CLIFF[1], depth + 1500 }
	return looks
end

return A
