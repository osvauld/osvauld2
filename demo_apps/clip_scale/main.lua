-- S-C1's repro (docs/design/2d-characters.md): a square squashed by a `scale_y` key, in a world
-- and on a plain frame.
local square = gfx.drawing({
	size = { 200, 200 },
	parts = {
		{ id = "sq", pivot = { 100, 100 }, shapes = {
			{ path = { {"move",50,50}, {"line",150,50}, {"line",150,150}, {"line",50,150}, {"close"} },
			  fill = "#ff00ff" },
		} },
	},
})

-- Full height until 0.5 s, then half: a step key.
local squash = gfx.clip({
	length = 1,
	tracks = { sq = { scale_y = { {0, 1}, {0.5, 0.5, "step"} } } },
})

local narrow = square:pose({ sq = { scale_x = 0.5 } })

local hot = nil

return function()
	return ui.col({
		full = true, center = true, gap = 16, fill = "#1b1b3a",
		ui.row({ gap = 16,
			ui.world({
				id = "stage", width = 400, height = 300, fill = "#202020",
				on_hover = function(e) hot = e.phase ~= "leave" and e.shape or nil end,
				{ id = "box", pos = { 100, 50 }, drawing = square, clip = squash },
			}),
			-- A handler makes it a hit target, so `rpc.rects()` reports where it is.
			ui.frame({ id = "narrow", visual = narrow, on_hover = function() end }),
		}),
		ui.text({ "entity: " .. (hot or "—"), color = "#c0c0c0", font_size = 13, no_wrap = true }),
	})
end
