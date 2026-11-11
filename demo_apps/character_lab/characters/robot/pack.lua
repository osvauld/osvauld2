-- The jetpack (docs/design/brawl-fighting.md decision 8): two parts every robot gets (build.lua) and
-- clips may key (author.lua). `pack` rides the chassis bone (`body`) and `flame` hangs in its nozzle;
-- both are drawn just before `slot:body`, flame first, so the chassis covers the pack's front and the
-- pack the flame's root. At rest the flame is a stub inside the pack's outline, out of sight: a clip's
-- `scale_y` (about the flame's pivot, inside the tank) stretches it out of the nozzle, and Brawl's view
-- multiplies that by the thrust (look.lua's FLAME) or hides it.
--
-- Points are in the setup drawing (the robot standing, facing right: its back is to the left). The
-- chassis bone is unrotated in the setup pose, so these are its own space too.
local P = {}

P.before = "slot:body"
P.NOZZLE = { 141, 199 } -- the middle of the nozzle's mouth
P.parts = {
	{ id = "flame", parent = "pack", pivot = { 141, 172 } },
	{ id = "pack", parent = "body", pivot = { 147, 158 } },
}

-- The shapes, from build.lua's kit (its materials and path helpers), by part id.
function P.pieces(k)
	local trim, plate, ink = k.trim, k.plate, k.ink
	return {
		pack = k.cat(
			k.box(trim, 132, 120, 22, 12, 4, 3, 1.6), -- the filler cap
			k.box(plate, 125, 127, 44, 62, 13, 7),
			{
				{ path = k.rrect(130, 136, 5, 40, 2.5), fill = plate.dark },
				{ path = k.poly(134, 185, 148, 185, 152, 199, 130, 199), fill = trim.base, stroke = { 1.8, ink } },
				{ path = k.rrect(130.5, 196, 21, 3.5, 1.75), fill = k.glow.dark },
			},
			k.box(trim, 123, 150, 48, 9, 4.5, 3, 1.6) -- the strap
		),
		flame = {
			{ path = { { "move", 131, 172 }, { "line", 151, 172 }, { "line", 151, 184 },
				{ "cubic", 151, 189, 145, 193, 141, 196 }, { "cubic", 137, 193, 131, 189, 131, 184 }, { "close" } },
				fill = "#ff8f2ee6" },
			{ path = { { "move", 136, 172 }, { "line", 146, 172 }, { "line", 146, 183 },
				{ "cubic", 146, 187, 143, 190, 141, 192 }, { "cubic", 139, 190, 136, 187, 136, 183 }, { "close" } },
				fill = "#fff2bf" },
		},
	}
end

return P
