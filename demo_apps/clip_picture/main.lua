-- S-C3 and S-C5's repro (docs/design/2d-characters.md). `pics/quads.png` is 64 x 64 in four
-- colours and `pics/crate.png` a 40 x 24 prop; the smoke writes both.

-- The green quadrant (top right) turned 90° and scaled ×3: x' = 96 - 3y, y' = 3x.
local tile = gfx.drawing({
	size = { 96, 96 },
	parts = {
		{ id = "tile", pivot = { 48, 48 }, shapes = {
			{ image = "pics/quads.png", src = { 32, 0, 32, 32 }, transform = { 0, 3, -3, 0, 96, 0 } },
		} },
	},
})
local turned = tile:pose()

-- A prop at its natural size, no drawing needed.
local crate = gfx.load_texture("pics/crate.png")
local prop = gfx.frame({ width = 40, height = 24, gfx.image({ texture = crate, id = "crate" }) })

local hot = nil

return function()
	return ui.col({
		full = true, pad = 40, gap = 40, fill = "#202020", align_center = true, -- frames at their own size
		ui.frame({
			id = "turned", visual = turned,
			on_hover = function(e) hot = e.phase ~= "leave" and e.shape or nil end,
		}),
		-- A handler makes it a hit target, so `rpc.rects()` reports where it is.
		ui.frame({ id = "prop", visual = prop, on_hover = function() end }),
		ui.text({ "part: " .. (hot or "—"), color = "#c0c0c0", font_size = 13, no_wrap = true }),
	})
end
