local C = require("theme")

local function path(commands)
	return gfx.path(commands)
end

local function rect(x, y, w, h)
	return path({
		{ "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h },
		{ "line", x, y + h }, { "close" },
	})
end

local function polygon(points)
	local commands = { { "move", points[1], points[2] } }
	for i = 3, #points, 2 do
		commands[#commands + 1] = { "line", points[i], points[i + 1] }
	end
	commands[#commands + 1] = { "close" }
	return path(commands)
end

local function ellipse(cx, cy, rx, ry, steps)
	local commands = { { "move", cx + rx, cy } }
	for i = 1, steps or 36 do
		local angle = math.pi * 2 * i / (steps or 36)
		commands[#commands + 1] = { "line", cx + rx * math.cos(angle), cy + ry * math.sin(angle) }
	end
	commands[#commands + 1] = { "close" }
	return path(commands)
end

local function fill(items, shape, color)
	items[#items + 1] = gfx.fill({ path = shape, brush = gfx.solid(color) })
end

local function stroke(items, shape, color, width, dashes)
	items[#items + 1] = gfx.stroke({
		path = shape,
		brush = gfx.solid(color),
		width = width or 3,
		cap = "round",
		join = "round",
		dashes = dashes or {},
	})
end

local function sun(items)
	fill(items, ellipse(80, 73, 28, 28), C.yellow)
	local rays = {}
	for i = 0, 7 do
		local a = math.pi * 2 * i / 8
		rays[#rays + 1] = { "move", 80 + 38 * math.cos(a), 73 + 38 * math.sin(a) }
		rays[#rays + 1] = { "line", 80 + 51 * math.cos(a), 73 + 51 * math.sin(a) }
	end
	stroke(items, path(rays), C.saffron, 4)
end

local function water(items)
	fill(items, path({
		{ "move", 80, 28 }, { "cubic", 62, 54, 49, 71, 49, 92 },
		{ "cubic", 49, 113, 63, 126, 80, 126 },
		{ "cubic", 98, 126, 112, 113, 112, 92 },
		{ "cubic", 112, 71, 98, 52, 80, 28 }, { "close" },
	}), "#55b8dd")
	stroke(items, path({ { "move", 62, 94 }, { "cubic", 67, 106, 75, 110, 85, 109 } }), "#dff8ff", 4)
end

local function ball(items)
	fill(items, ellipse(80, 78, 42, 42), "#e96c59")
	stroke(items, path({ { "move", 39, 70 }, { "cubic", 60, 64, 81, 72, 98, 94 } }), "#fff4da", 5)
	stroke(items, path({ { "move", 70, 38 }, { "cubic", 66, 58, 76, 82, 112, 90 } }), "#fff4da", 5)
end

local function pond(items)
	fill(items, ellipse(80, 94, 58, 27), "#77c9d4")
	fill(items, ellipse(80, 87, 38, 10), "#bce7dc")
	stroke(items, path({
		{ "move", 35, 95 }, { "line", 29, 55 }, { "move", 34, 69 }, { "line", 21, 61 },
		{ "move", 124, 95 }, { "line", 132, 52 }, { "move", 129, 70 }, { "line", 143, 61 },
	}), C.green, 4)
end

local function desert(items)
	fill(items, path({
		{ "move", 20, 112 }, { "cubic", 50, 82, 78, 103, 103, 89 },
		{ "cubic", 122, 80, 137, 91, 145, 112 }, { "close" },
	}), "#efc879")
	fill(items, rect(75, 49, 12, 52), C.green)
	fill(items, rect(58, 64, 21, 10), C.green)
	fill(items, rect(58, 53, 9, 20), C.green)
	fill(items, rect(84, 70, 20, 10), C.green)
	fill(items, rect(96, 59, 9, 19), C.green)
end

local function cupboard(items)
	fill(items, rect(43, 32, 74, 92), "#c48a57")
	stroke(items, rect(43, 32, 74, 92), "#7a5137", 3)
	stroke(items, path({ { "move", 80, 34 }, { "line", 80, 122 } }), "#7a5137", 3)
	fill(items, ellipse(71, 80, 3, 3), "#fff1cc")
	fill(items, ellipse(89, 80, 3, 3), "#fff1cc")
end

local function roots(items)
	fill(items, rect(75, 28, 10, 57), C.green)
	stroke(items, path({
		{ "move", 80, 78 }, { "line", 80, 118 },
		{ "move", 80, 87 }, { "line", 55, 111 }, { "move", 64, 102 }, { "line", 50, 101 },
		{ "move", 80, 94 }, { "line", 104, 117 }, { "move", 96, 109 }, { "line", 112, 106 },
	}), "#8b633e", 5)
	stroke(items, path({ { "move", 28, 79 }, { "line", 132, 79 } }), "#d4a96f", 3, { 6, 5 })
end

local function stem(items)
	stroke(items, path({ { "move", 80, 122 }, { "cubic", 77, 92, 84, 61, 80, 28 } }), C.green, 9)
	fill(items, path({
		{ "move", 79, 70 }, { "cubic", 55, 47, 37, 60, 43, 83 },
		{ "cubic", 60, 88, 71, 80, 79, 70 }, { "close" },
	}), "#53a86e")
end

local function leaf(items)
	fill(items, path({
		{ "move", 31, 89 }, { "cubic", 47, 38, 105, 31, 132, 52 },
		{ "cubic", 117, 106, 63, 124, 31, 89 }, { "close" },
	}), "#56aa66")
	stroke(items, path({ { "move", 39, 91 }, { "cubic", 66, 77, 92, 65, 125, 55 } }), "#d9f3c8", 4)
end

local function cell(items)
	fill(items, rect(45, 57, 70, 45), "#4f86a6")
	fill(items, rect(39, 67, 6, 25), "#263f4c")
	fill(items, rect(115, 64, 8, 31), C.saffron)
	stroke(items, path({ { "move", 57, 79 }, { "line", 71, 79 }, { "move", 103, 72 }, { "line", 103, 87 }, { "move", 95, 79 }, { "line", 111, 79 } }), C.white, 3)
end

local function switch(items)
	stroke(items, path({ { "move", 34, 101 }, { "line", 63, 101 }, { "move", 96, 101 }, { "line", 128, 101 } }), "#52655f", 5)
	fill(items, ellipse(65, 101, 8, 8), C.saffron)
	fill(items, ellipse(95, 101, 8, 8), C.saffron)
	stroke(items, path({ { "move", 65, 94 }, { "line", 99, 58 } }), C.green, 7)
end

local function bulb(items)
	fill(items, ellipse(80, 64, 34, 34), "#ffe388")
	stroke(items, path({ { "move", 58, 87 }, { "line", 65, 110 }, { "line", 95, 110 }, { "line", 102, 87 } }), "#6e766f", 5)
	stroke(items, path({ { "move", 66, 117 }, { "line", 94, 117 }, { "move", 70, 125 }, { "line", 90, 125 } }), "#6e766f", 5)
end

local function mouth(items)
	fill(items, ellipse(80, 77, 48, 48), "#efb48d")
	stroke(items, path({ { "move", 56, 84 }, { "cubic", 70, 97, 91, 97, 105, 84 } }), "#9f4e4e", 6)
	fill(items, ellipse(62, 68, 4, 5), C.ink)
	fill(items, ellipse(98, 68, 4, 5), C.ink)
end

local function stomach(items)
	fill(items, path({
		{ "move", 70, 31 }, { "cubic", 68, 60, 86, 57, 98, 69 },
		{ "cubic", 119, 90, 99, 127, 69, 121 },
		{ "cubic", 40, 115, 42, 83, 60, 74 }, { "cubic", 72, 67, 65, 46, 70, 31 }, { "close" },
	}), "#df7f72")
	stroke(items, path({ { "move", 72, 33 }, { "cubic", 67, 59, 84, 66, 96, 70 } }), "#9c4c4c", 4)
end

local function intestine(items)
	fill(items, rect(42, 35, 76, 91), "#edb69e")
	local coils = {
		{ "move", 55, 50 }, { "cubic", 106, 42, 107, 62, 58, 65 },
		{ "cubic", 42, 67, 44, 82, 62, 81 }, { "line", 98, 79 },
		{ "cubic", 119, 79, 118, 96, 97, 97 }, { "line", 61, 98 },
		{ "cubic", 44, 99, 47, 114, 101, 111 },
	}
	stroke(items, path(coils), "#b75f59", 8)
end

local icon = {
	sun = sun, water = water, ball = ball,
	pond = pond, desert = desert, cupboard = cupboard,
	root = roots, stem = stem, leaf = leaf,
	cell = cell, switch = switch, bulb = bulb,
	mouth = mouth, stomach = stomach, intestine = intestine,
}

local diagrams = {
	["plant-needs"] = { "sun", "water", "ball" },
	["frog-home"] = { "pond", "desert", "cupboard" },
	["plant-parts"] = { "root", "stem", "leaf" },
	["circuit-parts"] = { "cell", "switch", "bulb" },
	["digestion"] = { "mouth", "stomach", "intestine" },
}

local visuals = {}
for name, choices in pairs(diagrams) do
	local items = { width = 520, height = 160 }
	for i, key in ipairs(choices) do
		local group = {
			id = "choice:" .. key,
			transform = { 1, 0, 0, 1, (i - 1) * 180, 0 },
		}
		fill(group, rect(3, 3, 154, 150), C.paper_warm)
		stroke(group, rect(3, 3, 154, 150), C.line, 2)
		icon[key](group)
		items[#items + 1] = gfx.group(group)
	end
	visuals[name] = gfx.frame(items)
end

function visuals.circuit_lab(amount)
	local p = math.max(0, math.min(1, amount or 0))
	local lit = p >= 0.96
	local items = { width = 520, height = 210 }
	local wire = path({
		{ "move", 70, 105 }, { "line", 185, 105 },
		{ "move", 275, 105 }, { "line", 372, 105 },
		{ "move", 428, 105 }, { "line", 470, 105 }, { "line", 470, 170 },
		{ "line", 70, 170 }, { "line", 70, 105 },
	})
	stroke(items, wire, lit and C.green or "#7b8984", 5)
	fill(items, rect(54, 121, 32, 35), "#4f86a6")
	fill(items, rect(59, 115, 22, 6), C.saffron)
	fill(items, ellipse(400, 105, lit and 52 or 27, lit and 52 or 27), lit and "rgba(244,201,93,0.36)" or "rgba(244,201,93,0.04)")
	fill(items, ellipse(400, 105, 24, 24), lit and "#ffe388" or "#d8ddd8")
	stroke(items, ellipse(400, 105, 24, 24), "#6e766f", 4)
	for i = 0, 7 do
		local a = math.pi * 2 * i / 8
		local inner = 31
		local outer = lit and 47 or 31
		stroke(items, path({
			{ "move", 400 + inner * math.cos(a), 105 + inner * math.sin(a) },
			{ "line", 400 + outer * math.cos(a), 105 + outer * math.sin(a) },
		}), C.saffron, 3)
	end
	fill(items, ellipse(195, 105, 8, 8), C.saffron)
	fill(items, ellipse(275, 105, 8, 8), C.saffron)
	local hx = 195 + 80 * p
	local hy = 105 - 48 * (1 - p)
	stroke(items, path({ { "move", 195, 105 }, { "line", hx, hy } }), C.green, 8)
	local handle = { id = "switch-handle", transform = { 1, 0, 0, 1, hx, hy } }
	fill(handle, ellipse(0, 0, 14, 14), C.green)
	stroke(handle, ellipse(0, 0, 14, 14), C.white, 3)
	items[#items + 1] = gfx.group(handle)
	return gfx.frame(items)
end

return visuals
