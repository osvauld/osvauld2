-- A map many screens wide, seen through a camera that follows the hero. WASD walks; the world
-- draws only the stones the box can show.

local function square(size, fill)
	return gfx.drawing({
		size = { size, size },
		parts = {
			{ id = "body", pivot = { size / 2, size / 2 }, shapes = { {
				path = { { "move", 0, 0 }, { "line", size, 0 }, { "line", size, size }, { "line", 0, size }, { "close" } },
				fill = fill,
			} } },
		},
	})
end

local hero = square(32, "#e0453a")
local stone = square(24, "#3a6fe0")

-- 30 × 20 stones, 100 apart: stone_i_j's box is (100 i + 38, 100 j + 38), 24 square.
local stones = {}
for j = 0, 19 do
	for i = 0, 29 do
		stones[#stones + 1] = { id = "stone_" .. i .. "_" .. j, pos = { 100 * i + 38, 100 * j + 38 }, drawing = stone }
	end
end

local wasd = { speed = 250, axis_x = { neg = "KeyA", pos = "KeyD" }, axis_y = { neg = "KeyW", pos = "KeyS" } }
local hot

return function()
	local world = {
		id = "map", width = 640, height = 400, fill = "#2a3b33", radius = 8,
		camera = { follow = "hero", bounds = { 0, 0, 3000, 2000 } },
		on_hover = function(e) hot = e.phase ~= "leave" and e.shape or nil end,
	}
	for i, s in ipairs(stones) do
		world[i] = s
	end
	world[#world + 1] = { id = "hero", pos = { 1484, 984 }, drawing = hero, controller = wasd }
	return ui.col({
		full = true, center = true, gap = 12, fill = "#14181c",
		ui.text({ "WASD walks; the camera follows", color = "#c9d1d9", font_size = 15, no_wrap = true }),
		ui.world(world),
		ui.text({ "entity: " .. (hot or "—"), color = "#8b949e", font_size = 13, no_wrap = true }),
	})
end
