local C = require("theme")
local M = require("level")
local S = M.state
local title = "Tilt Maze"

local function label(id, value, color)
	return ui.text({ id = id, value, color = color or C.text, font_size = 15, no_wrap = true })
end
local function button(id, value, handler)
	return ui.button({
		id = id, h = 34, px = 12, center = true, radius = 7,
		fill = C.button, hover_fill = C.hover, no_shrink = true,
		ui.text({value, color = C.text, font_size = 14, no_wrap = true}),
		on_click = handler,
	})
end

return function()
	return ui.col({
		id = "tilt-root", full = true, stretch = true, pad = 14, gap = 8, fill = C.bg,
		ui.row({gap = 18, align_center = true,
			label("title", title, C.accent), label("status", S.paused and "Paused" or S.status),
			label("attempt", "Attempt " .. tostring(S.attempt)),
			label("progress", S.checkpoint and "Checkpoint saved" or "Find the checkpoint"),
		}),
		label("instructions", "East gap > cyan checkpoint > west gap > green goal. Red loses; south edge is open.", C.muted),
		ui.scene3d({
			id = "maze-view", grow = true, min_h = 220, w_full = true,
			scene = M.game:scene(nil, {running = S.status == "Playing" and not S.paused}),
			on_zone = M.zone,
		}),
		ui.row({ gap = 8, center = true,
			button("north", "North", function() M.tilt("North") end),
			button("west", "West", function() M.tilt("West") end),
			button("level", "Brake / level", function() M.tilt("Level") end),
			button("east", "East", function() M.tilt("East") end),
			button("south", "South", function() M.tilt("South") end),
			label("tilt", S.tilt),
		}),
		ui.row({ gap = 8, center = true,
			button("release", "Release / new run", M.release),
			button("retry", S.checkpoint and "Retry checkpoint" or "Retry start", M.retry),
			button("pause", S.paused and "Resume" or "Pause", M.pause),
		}),
		label("brake-help", "Tilt stays set until changed. Brake clears momentum. Retry keeps cyan progress; new run clears it.", C.muted),
		ui.text({id = "events", table.concat(S.events, " | "), color = C.muted, font_size = 12, h = 32}),
	})
end
