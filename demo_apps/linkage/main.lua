local C = require("theme")
local arm = require("arm")
local W = require("ui/widgets")

local function rest()
	return { shoulder = C.rest.shoulder, elbow = C.rest.elbow, wrist = C.rest.wrist }
end

local S = {
	angles = rest(),
	hot = nil,
	sx = 0,
	sy = 0,
	wave = false,
	t = 0,
	drag = nil,
}

local function pose()
	if not S.wave then
		return S.angles
	end
	local t = S.t * C.wave_speed
	local amp = C.wave_amp
	return {
		shoulder = S.angles.shoulder + amp.shoulder * math.sin(t * 1.7),
		elbow = S.angles.elbow + amp.elbow * math.sin(t * 2.4 + 0.9),
		wrist = S.angles.wrist + amp.wrist * math.sin(t * 3.3 + 1.7),
	}
end

local function nudge(key, d)
	S.angles[key] = S.angles[key] + d
end

-- The grabbed shape's space is frozen at the press, so the pointer's angle in it is measured
-- against a frame that does not move as the joint turns: the swing is `pressed angle + swept
-- angle`, with no feedback and nothing to converge. It only goes soft while the wave runs, which
-- turns the parent joints under that frozen frame.
local function swept(d, sx, sy)
	return math.deg(math.atan2(sy - d.pivot[2], sx - d.pivot[1]) - d.at)
end

-- atan2 jumps a whole turn at its branch cut, and a hand swung past straight-back crosses it.
local function unwrap(prev, a)
	while a - prev > 180 do
		a = a - 360
	end
	while a - prev < -180 do
		a = a + 360
	end
	return a
end

local function swing(phase, shape, sx, sy)
	if phase == "start" then
		local g = shape and arm.grips[shape]
		if not g then
			return
		end
		local dx, dy = sx - g.pivot[1], sy - g.pivot[2]
		if dx * dx + dy * dy < C.grab_min * C.grab_min then
			return
		end
		S.drag = {
			shape = shape,
			joint = g.joint,
			pivot = g.pivot,
			from = S.angles[g.joint],
			at = math.atan2(dy, dx),
			swept = 0,
		}
	end
	local d = S.drag
	if not d then
		return
	end
	S.hot, S.sx, S.sy = d.shape, sx, sy
	d.swept = unwrap(d.swept, swept(d, sx, sy))
	S.angles[d.joint] = d.from + d.swept
	if phase == "end" then
		S.drag = nil
	end
end

local function angle_text(key, live)
	if not S.wave then
		return string.format("%.0f deg", S.angles[key])
	end
	return string.format("%.0f deg (%.0f)", S.angles[key], live[key])
end

local function readout()
	local span = S.hot and arm.spans[S.hot]
	if not span then
		return "pointer is on no named shape - hover a segment to read it, drag one to swing its joint"
	end
	local pct = (S.sx - span.origin) / span.len * 100
	return string.format(
		"%s %s  [%s]  local %.1f, %.1f  -  %.0f%% %s  -  %d transforms undone",
		S.drag and "holding" or "on",
		span.label,
		S.hot,
		S.sx,
		S.sy,
		pct,
		span.axis,
		arm.depth[S.hot]
	)
end

return function()
	local live = pose()
	local controls = ui.col({
		gap = 8,
		W.label("JOINTS", C.dim, 11),
		W.joint("shoulder", "shoulder", angle_text("shoulder", live), nudge),
		W.joint("elbow", "elbow", angle_text("elbow", live), nudge),
		W.joint("wrist", "wrist", angle_text("wrist", live), nudge),
		ui.row({
			gap = 8,
			mt = 6,
			W.toggle("wave", S.wave and "Waving" or "Wave", S.wave, function()
				S.wave = not S.wave
			end),
			W.toggle("reset", "Reset", false, function()
				S.angles = rest()
			end),
		}),
		ui.col({ grow = true }),
		W.label("drag a segment to swing its joint", C.dim, 11),
		W.label("groups nest 4 deep: shoulder > elbow > wrist > hand", C.dim, 11),
	})

	local root = {
		ui.text({ "Linkage - a kinematic arm of nested rotations", color = C.text, font_size = 15, no_wrap = true }),
		ui.row({
			gap = 18,
			ui.frame({
				id = "arm",
				visual = arm.build(live, S.hot),
				on_hover = function(e)
					local phase, x, y, shape, sx, sy = e.phase, e.x, e.y, e.shape, e.sx, e.sy
					if S.drag then
						return -- a held segment keeps the readout, whatever the pointer has slid over
					end
					if phase == "leave" or not shape then
						S.hot = nil
					else
						S.hot, S.sx, S.sy = shape, sx, sy
					end
				end,
				on_drag = function(e)
					local phase, x, y, dx, dy, scale, origin_x, origin_y, shape, sx, sy = e.phase, e.x, e.y, e.dx, e.dy, e.scale, e.origin_x, e.origin_y, e.shape, e.sx, e.sy
					swing(phase, shape, sx, sy)
				end,
			}),
			controls,
		}),
		ui.row({
			h = 34,
			px = 12,
			radius = 8,
			align_center = true,
			fill = C.panel,
			W.label(readout(), S.hot and C.text or C.dim),
		}),
		id = "linkage",
		grow = true,
		fill = C.bg,
		pad = 18,
		gap = 14,
	}
	if S.wave then
		root.on_frame = function(e)
			local dt, elapsed = e.dt, e.elapsed
			S.t = elapsed
		end
	end
	return ui.col(root)
end
