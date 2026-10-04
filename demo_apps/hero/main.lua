local C = require("theme")
local hero = gfx.drawing(require("hero"))

-- Poses are overrides from rest, about each part's pivot; children follow their parent.
local poses = {
	{ "rest", {} },
	{ "wave", { arm_near = { rot = -150 }, head = { rot = 8 } } },
	{ "step", { leg_l = { rot = 22 }, leg_r = { rot = -22 }, arm_far = { rot = -25 },
		arm_near = { rot = 25 }, body = { y = -4 } } },
	{ "lean", { body = { rot = -12 }, head = { rot = 14 } } },
}

for _, p in ipairs(poses) do
	p[3] = hero:pose(p[2]) -- compiled once; a pose is an ordinary frame
end

local hot = nil -- the part under the pointer, as "pose / part"

return function()
	local cards = {}
	for _, p in ipairs(poses) do
		local name = p[1]
		table.insert(cards, ui.col({
			align_center = true, gap = 8, pad = 16, radius = 12, fill = C.card,
			ui.frame({ id = "pose:" .. name, visual = p[3], on_hover = function(e)
				hot = e.phase ~= "leave" and e.shape and (name .. " / " .. e.shape) or nil
			end }),
			ui.text({ name, color = C.muted, font_size = 13, no_wrap = true }),
		}))
	end
	return ui.col({
		full = true, center = true, gap = 20, fill = C.bg,
		ui.text({ "One drawing, four poses", color = C.text, font_size = 22, no_wrap = true }),
		ui.row({ gap = 16, cards }),
		ui.text({ "part: " .. (hot or "—"), color = C.muted, font_size = 13, no_wrap = true }),
	})
end
