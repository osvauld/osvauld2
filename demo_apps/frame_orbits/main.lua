local geom = require("geom")

local function path(commands)
	return gfx.path(commands)
end

local bg = path({
	{ "move", 0, 0 }, { "line", 800, 0 },
	{ "line", 800, 520 }, { "line", 0, 520 }, { "close" },
})
local sky = gfx.linear_gradient({
	from = { 0, 0 }, to = { 800, 520 }, extend = "pad",
	stops = {
		{ 0, "#080b1d" }, { 0.45, "#172a56" }, { 1, "#4a174f" },
	},
})
local orbit_brush = gfx.solid("#b7d5ff38")
local star_brush = gfx.solid("#ffffffcc")

local function disc(size, colors)
	local shape = path(geom.circle_path(size / 2, size / 2, size / 2 - 1))
	local brush = gfx.linear_gradient({
		from = { 0, 0 }, to = { size, size },
		stops = { { 0, colors[1] }, { 1, colors[2] } },
	})
	return gfx.frame({
		width = size, height = size,
		gfx.fill({ path = shape, brush = brush }),
	})
end

local sun = disc(104, { "#fff5a5", "#ff6b35" })
local planet = disc(44, { "#72e4ff", "#4957d6" })
local moon = disc(16, { "#ffffff", "#7e92ad" })
local star = disc(5, { "#ffffff", "#a7c8ff" })
local orbit1 = path(geom.ellipse_path(0, 0, 220, 132))
local orbit2 = path(geom.ellipse_path(0, 0, 308, 194))
local moon_orbit = path(geom.ellipse_path(0, 0, 48, 31))

local stars = {}
for i = 1, 34 do
	local x = (i * 137) % 780 + 10
	local y = (i * 83) % 500 + 10
	local scale = 0.45 + (i % 4) * 0.2
	table.insert(stars, gfx.instance({
		visual = star,
		transform = geom.compose({ geom.translate(x, y), geom.scale(scale) }),
	}))
end

local angle = 0

local function build_scene()
	local planet_x, planet_y = 220 * math.cos(angle), 132 * math.sin(angle)
	local moon_angle = angle * 4
	local moon_x, moon_y = 48 * math.cos(moon_angle), 31 * math.sin(moon_angle)
	local outer_angle = angle * 0.55 - 0.8
	local outer_x, outer_y = 308 * math.cos(outer_angle), 194 * math.sin(outer_angle)
	return gfx.frame({
	width = 800, height = 520,
	gfx.fill({ path = bg, brush = sky }),
	gfx.group(stars),
	gfx.group({
		transform = geom.translate(400, 260),
		gfx.stroke({ path = orbit1, brush = orbit_brush, width = 2, cap = "round" }),
		gfx.stroke({
			path = orbit2, brush = orbit_brush, width = 2,
			cap = "round", dashes = { 10, 7 }, dash_offset = 3,
		}),
		gfx.instance({ visual = sun, transform = geom.translate(-52, -52) }),
		gfx.group({
			transform = geom.translate(planet_x, planet_y),
			gfx.stroke({ path = moon_orbit, brush = orbit_brush, width = 1.5 }),
			gfx.instance({ visual = planet, transform = geom.translate(-22, -22) }),
			-- Instances use their top-left origin, so subtract the moon's 8pt radius.
			gfx.instance({ visual = moon, transform = geom.translate(moon_x - 8, moon_y - 8) }),
		}),
		gfx.instance({ visual = moon, transform = geom.translate(outer_x - 8, outer_y - 8) }),
	}),
	})
end

local function tick(dt)
	angle = (angle + dt * 0.42) % (math.pi * 2)
end

return function()
	return ui.col({
		full = true, fill = "#080b1d",
		ui.row({
			h = 48, px = 18, center = true, fill = "#11162c",
			ui.text({ "Lua Frame · orbital study", color = "#eef5ff", no_wrap = true }),
		}),
		ui.col({
			id = "orbit-camera", grow = true, zoomable = true,
			-- A zoom viewport stabilizes its direct child's content origin. Center inside a
			-- viewport-sized child rather than asking the camera to move that origin.
			ui.col({
				full = true, center = true,
				ui.frame({ id = "orbit-frame", visual = build_scene(), on_frame = tick }),
			}),
		}),
	})
end
