-- A day/night switch played from an asset's clips on a plain frame: `day` and `night` hold a
-- state, `to_night` and `to_day` move between them. No world — `clip:at` gives each pose.
local Toggle = {}

-- One switch over an opened asset. Nothing is drawn until the asset's mirror has its parts.
function Toggle.new(asset, view)
	return { asset = asset, view = view or "front", state = "day", t = 0 }
end

local function ready(s)
	if not s.drawing then
		local a = s.asset
		local parts = a.views and a.views[s.view] and a.views[s.view].parts
		if not parts or not a.clips then
			return false
		end
		s.drawing = gfx.drawing({ size = a.size, vars = a.vars, parts = parts })
		s.clips = {}
		for name, c in pairs(a.clips) do
			s.clips[name] = gfx.clip(c)
		end
	end
	return true
end

-- The state it rests on, or the move it is making.
function Toggle.label(s)
	return s.moving or s.state
end

-- The switch as an element; a click mid-move is ignored. `on_frame` is set only while it moves,
-- so a resting switch asks for no frames.
function Toggle.view(s, id)
	if not ready(s) then
		return ui.text({ "no toggle asset yet — open it in the asset editor", no_wrap = true })
	end
	local clip = s.clips[s.moving or s.state]
	local spec = {
		id = id,
		visual = s.drawing:pose(clip:at(s.t)),
		on_click = function()
			if not s.moving then
				s.moving, s.t = s.state == "day" and "to_night" or "to_day", 0
			end
		end,
	}
	if s.moving then
		spec.on_frame = function(e)
			s.t += e.dt
			if s.t >= clip.length then
				s.state = s.moving == "to_night" and "night" or "day"
				s.moving, s.t = nil, 0
			end
		end
	end
	return ui.frame(spec)
end

return Toggle
