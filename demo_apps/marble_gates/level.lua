local C = require("theme")

return function(sphere)
	local L = { angle = -20, status = "Ready", attempt = 0, tick = 0 }
	local function rotation(angle)
		return { 0, 0, math.sin(math.rad(angle) / 2), math.cos(math.rad(angle) / 2) }
	end
	L.world = gfx.world3d({
		id = "marble-level",
		scene = gfx.scene3d({
			camera = { eye = {7,5,7}, target = {0,1,0} },
			objects = {
				{ id = "platform", scale = {6,0.5,4}, color = C.platform },
				{ id = "marble", mesh = sphere, position = {0,3,0}, color = C.marble },
				{ id = "gate-a", position = {2.6,-0.2,-0.7}, scale = {0.12,1.6,0.12}, color = C.goal },
				{ id = "gate-b", position = {2.6,-0.2,0.7}, scale = {0.12,1.6,0.12}, color = C.goal },
				{ id = "gate-top", position = {2.6,0.6,0}, scale = {0.12,0.12,1.5}, color = C.goal },
				{ id = "fall-marker", position = {0,-4.5,0}, scale = {10,0.04,8}, color = C.fall },
			},
		}),
		bodies = {
			{ id = "platform", box = {6,0.5,4}, position = {0,0,0}, rotation = rotation(L.angle) },
			{ id = "marble", sphere = 0.25, position = {0,3,0}, dynamic = true },
			{ id = "goal", box = {0.6,2,1.4}, position = {2.6,-0.4,0}, sensor = true },
			{ id = "fall", box = {30,1,30}, position = {0,-4,0}, sensor = true },
		},
	})

	function L.release()
		if L.status ~= "Ready" then return end
		L.world:set("platform", { rotation = rotation(L.angle) })
		L.world:reset("marble")
		L.attempt += 1
		L.status, L.tick = "Playing", 0
	end

	function L.retry()
		L.world:reset("marble")
		L.status, L.tick = "Ready", 0
	end

	function L.tilt(delta)
		L.angle = math.clamp(L.angle + delta, -30, 30)
		L.world:set("platform", { rotation = rotation(L.angle) })
	end

	function L.zone(e)
		if L.status ~= "Playing" or e.who ~= "marble" or e.phase ~= "enter" then return end
		if e.id == "goal" then L.status, L.tick = "Won", e.tick end
		if e.id == "fall" then L.status, L.tick = "Lost", e.tick end
	end

	function L.controls(button)
		return {
			ui.row({ gap = 8,
				button("gates-release", "Release", L.release),
				button("gates-retry", "Retry", L.retry),
				button("gates-tilt-left", "Tilt left", function() L.tilt(10) end),
				button("gates-tilt-right", "Tilt right", function() L.tilt(-10) end),
				ui.text({ "Ramp: " .. L.angle .. "°", id = "gates-angle", color = C.text, font_size = 13 }),
			}),
			ui.row({ gap = 16,
				ui.text({ L.status .. (L.tick > 0 and " — tick " .. L.tick or ""), id = "gates-status",
					color = L.status == "Won" and C.goal or (L.status == "Lost" and C.danger or C.text), font_size = 15 }),
				ui.text({ "Attempt " .. L.attempt, id = "gates-attempt", color = C.muted, font_size = 13 }),
			}),
		}
	end

	return L
end
