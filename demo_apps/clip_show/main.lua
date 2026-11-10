-- S-C2's repro (docs/design/2d-characters.md): a part hidden by a `show` track from 0.20 s to
-- 0.40 s while its child stays drawn, in a world and on a plain frame.
local pair = gfx.drawing({
	size = { 200, 100 },
	parts = {
		{ id = "sq", pivot = { 50, 50 }, shapes = {
			{ path = { {"move",10,10}, {"line",90,10}, {"line",90,90}, {"line",10,90}, {"close"} },
			  fill = "#ff00ff" },
		} },
		{ id = "kid", parent = "sq", pivot = { 150, 50 }, shapes = {
			{ path = { {"move",110,10}, {"line",190,10}, {"line",190,90}, {"line",110,90}, {"close"} },
			  fill = "#00ffff" },
		} },
	},
})

local blink = gfx.clip({
	length = 1,
	tracks = { sq = { show = { {0, true}, {0.2, false}, {0.4, true} } } },
})

-- The plain frame draws the pose the clip gives at 0.3 s: the square hidden, the child there.
local frozen = pair:pose(blink:at(0.3))

return function()
	return ui.col({
		full = true, center = true, gap = 16, fill = "#1b1b3a",
		-- The no-op handlers make both hit targets, so `rpc.rects()` reports where they are.
		ui.world({
			id = "stage", width = 200, height = 100, fill = "#202020", on_hover = function() end,
			{ id = "pair", pos = { 0, 0 }, drawing = pair, clip = blink },
		}),
		ui.frame({ id = "frozen", visual = frozen, on_hover = function() end }),
	})
end
