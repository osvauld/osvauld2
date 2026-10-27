-- A map's scenery, built once per map: a sky gradient, parallax silhouette layers, and the
-- platforms. Palettes live in `S.themes` and are chosen by the map's `theme` (unknown names and maps
-- without one use "yard"). Drawing only -- no rules read this file.
local C = require("theme")
local S = {}

S.themes = {
	yard = {
		sky = { "#d7ded0", "#b8c9b4" }, rock = "#26383a", sun = { x = 0.8, y = 0.2, r = 48, color = C.sun },
		layers = {
			{ kind = "hills", f = 0.25, h = 330, color = C.haze, step = 110 },
			{ kind = "hills", f = 0.5, h = 210, color = C.hills, step = 120 },
			{ kind = "trees", f = 0.8, h = 120, color = C.mountain },
		},
		plat = { ground = C.sand, top = C.edge, ledge = C.steel, edge = C.edge, cover = C.steel, mark = C.sand, ink = C.ink },
	},
	cliffs = {
		sky = { "#7cb0d8", "#f0d9b0" }, rock = "#3a3430", sun = { x = 0.72, y = 0.12, r = 60, color = "#fff1c4" },
		layers = {
			{ kind = "spires", f = 0.2, h = 520, color = "#9db6c9", step = 150 },
			{ kind = "spires", f = 0.45, h = 360, color = "#7b97ad", step = 130 },
			{ kind = "hills", f = 0.7, h = 200, color = "#5f7d6c", step = 160 },
		},
		plat = { ground = "#7a6a58", top = "#86a35c", ledge = "#4c5a66", edge = "#9db3c0", cover = "#6a5c4c", mark = "#a8906a", ink = "#2a2622" },
	},
	underground = {
		sky = { "#2a2038", "#120d1c" }, rock = "#0a0810",
		layers = {
			{ kind = "columns", f = 0.25, h = 700, color = "#1d1629", step = 260 },
			{ kind = "columns", f = 0.55, h = 520, color = "#291f3a", step = 200 },
		},
		plat = { ground = "#40384f", top = "#8a7fa0", ledge = "#5a5170", edge = "#9d92b4", cover = "#4a4260", mark = "#8a7fa0", ink = "#1a1424" },
	},
	city = {
		sky = { "#26335f", "#eaa87c" }, rock = "#15181f", sun = { x = 0.3, y = 0.1, r = 70, color = "#ffe2b8" },
		layers = {
			{ kind = "towers", f = 0.2, h = 620, color = "#3d4a78", lit = "#7e8cb8" },
			{ kind = "towers", f = 0.45, h = 440, color = "#2e3a62", lit = "#f4d58a" },
			{ kind = "towers", f = 0.7, h = 290, color = "#212a49", lit = "#f4d58a" },
		},
		plat = { ground = "#2f343d", top = "#7d8794", ledge = "#59616f", edge = "#a3adba", cover = "#6a727f", mark = "#d9b44a", ink = "#1a1d24" },
	},
}

function S.theme(map) return S.themes[map.theme] or S.themes.yard end

local brushes = {}
local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end
local function poly(points, color)
	local commands = { { "move", points[1][1], points[1][2] } }
	for i = 2, #points do commands[#commands + 1] = { "line", points[i][1], points[i][2] } end
	commands[#commands + 1] = { "close" }
	return gfx.fill({ path = gfx.path(commands), brush = brush(color) })
end
local function rect(x, y, w, h, color) return poly({ { x, y }, { x + w, y }, { x + w, y + h }, { x, y + h } }, color) end
local function circle(x, y, r, color)
	local points = {}
	for i = 0, 23 do
		local a = i * math.pi / 12
		points[#points + 1] = { x + math.cos(a) * r, y + math.sin(a) * r }
	end
	return poly(points, color)
end

-- Deterministic noise, so a map always looks the same.
local function noise(seed)
	local s = seed
	return function()
		s = (s * 1103515245 + 12345) % 2147483648
		return s / 2147483648
	end
end

-- One silhouette layer as a frame `w` wide and `L.h` tall (its bottom edge is the frame's).
local function layer_items(L, w, seed)
	local r, H, items = noise(seed), L.h, {}
	if L.kind == "hills" or L.kind == "spires" then
		local ridge = { { 0, H } }
		local x = 0
		while x <= w + L.step do
			local top = L.kind == "spires" and (0.15 + 0.85 * r() ^ 2) or (0.35 + 0.65 * r())
			ridge[#ridge + 1] = { x, H - H * top * (L.kind == "spires" and 1 or 0.8) }
			x = x + L.step * (0.6 + 0.8 * r())
		end
		ridge[#ridge + 1] = { x, H }
		items[#items + 1] = poly(ridge, L.color)
	elseif L.kind == "trees" then
		for i = 0, math.ceil(w / 57) do
			local x, y = i * 57 - 25, H - 76 + (i % 3) * 14 - r() * 18
			items[#items + 1] = rect(x, y, 28, H - y, L.color)
			items[#items + 1] = poly({ { x - 9, y + 10 }, { x + 14, y - 19 }, { x + 37, y + 10 } }, L.color)
		end
	elseif L.kind == "towers" then
		local x = 0
		while x < w do
			local bw, bh = 60 + r() * 90, H * (0.25 + 0.75 * r())
			items[#items + 1] = rect(x, H - bh, bw, bh, L.color)
			for wy = H - bh + 18, H - 20, 34 do
				for wx = x + 12, x + bw - 18, 28 do
					if r() < 0.3 then items[#items + 1] = rect(wx, wy, 9, 13, L.lit) end
				end
			end
			x = x + bw + r() * 30
		end
	elseif L.kind == "columns" then
		local x = 0
		while x < w do
			local cw = 36 + r() * 50
			local top = H * (0.2 + 0.8 * r())
			items[#items + 1] = rect(x, H - top, cw, top, L.color)
			items[#items + 1] = rect(x - 8, H - top - 10, cw + 16, 12, L.color) -- capital
			x = x + L.step * (0.6 + 0.8 * r())
		end
	end
	return items
end

local built = {}
-- {rock, sky = frame (map-sized, world-locked), layers = { {visual, f, h} }, platforms = frame}
function S.build(map)
	if built[map] then return built[map] end
	local theme, W, H = S.theme(map), map.w, map.h
	local sky = gfx.linear_gradient({ from = { 0, 0 }, to = { 0, H }, stops = { { 0, theme.sky[1] }, { 1, theme.sky[2] } } })
	local sky_items = { width = W, height = H,
		gfx.fill({ path = gfx.path({ { "move", 0, 0 }, { "line", W, 0 }, { "line", W, H }, { "line", 0, H }, { "close" } }), brush = sky }) }
	if theme.sun then
		sky_items[#sky_items + 1] = circle(W * theme.sun.x, H * theme.sun.y, theme.sun.r, theme.sun.color)
	end
	local layers = {}
	for i, L in ipairs(theme.layers) do
		local items = layer_items(L, W, 7919 * i + #map.id)
		items.width, items.height = W, L.h
		layers[i] = { visual = gfx.frame(items), f = L.f, h = L.h }
	end
	local P, plats = theme.plat, { width = W, height = H }
	for _, r in ipairs(map.platforms) do
		if r.kind == "ground" then
			plats[#plats + 1] = rect(r.x, r.y, r.w, r.h, P.ground)
			plats[#plats + 1] = rect(r.x, r.y, r.w, math.min(8, r.h), P.top)
		elseif r.kind == "cover" then
			plats[#plats + 1] = rect(r.x, r.y, r.w, r.h, P.cover)
			plats[#plats + 1] = rect(r.x, r.y, r.w, 4, P.edge)
			if r.h >= 40 and r.w >= 30 then
				plats[#plats + 1] = rect(r.x + r.w / 2 - 13, r.y + 24, 26, 4, P.mark)
				plats[#plats + 1] = rect(r.x + r.w / 2 - 13, r.y + 34, 26, 4, P.mark)
			end
		else
			plats[#plats + 1] = rect(r.x, r.y, r.w, r.h, P.ledge)
			plats[#plats + 1] = rect(r.x, r.y, r.w, 4, P.edge)
			plats[#plats + 1] = rect(r.x, r.y + r.h - 3, r.w, 3, P.ink)
			for x = r.x + 10, r.x + r.w - 8, 52 do plats[#plats + 1] = rect(x, r.y + 8, 4, 3, P.edge) end
		end
	end
	built[map] = { rock = theme.rock, sky = gfx.frame(sky_items), layers = layers, platforms = gfx.frame(plats) }
	return built[map]
end

-- The scenery as elements for a camera geometry `geo` (camera.lua): a map-sized clipped box at the map's
-- place on screen holding the sky, the layers (each slower than the camera on both axes) and the platforms.
function S.view(map, geo)
	local scene, k = S.build(map), geo.scale
	local floor = map.h - geo.vh -- the camera's y when the view rests on the map's bottom
	local children = { ui.frame({ visual = scene.sky, absolute = true, left = 0, top = 0, scale = k }) }
	for _, L in ipairs(scene.layers) do
		children[#children + 1] = ui.frame({ visual = L.visual, absolute = true, scale = k,
			left = geo.x * (1 - L.f) * k, top = (map.h - L.h + (geo.y - floor) * (1 - L.f)) * k })
	end
	children[#children + 1] = ui.frame({ visual = scene.platforms, absolute = true, left = 0, top = 0, scale = k })
	return ui.col({ id = "map:scenery", absolute = true, left = geo.ox - geo.x * k, top = geo.oy - geo.y * k,
		w = map.w * k, h = map.h * k, clip = true, children })
end

return S
