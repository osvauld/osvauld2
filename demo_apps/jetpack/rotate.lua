local C = require("theme")
local R = {}

-- Jetpack Arena is played in landscape: a playfield taller than it is wide gets this card instead of the game.
function R.portrait(size) return size.h > size.w end

local function rounded(x, y, w, h, r)
	return gfx.path({{"move", x + r, y}, {"line", x + w - r, y}, {"quad", x + w, y, x + w, y + r}, {"line", x + w, y + h - r},
		{"quad", x + w, y + h, x + w - r, y + h}, {"line", x + r, y + h}, {"quad", x, y + h, x, y + h - r}, {"line", x, y + r},
		{"quad", x, y, x + r, y}, {"close"}})
end

local icon
-- An upright phone, a curved arrow, and the phone on its side.
local function art()
	if icon then return icon end
	local ink, dim = gfx.solid(C.player), gfx.solid(C.cross)
	icon = gfx.frame({width = 150, height = 100,
		gfx.stroke({path = rounded(14, 18, 36, 64, 7), brush = ink, width = 3, join = "round"}),
		gfx.stroke({path = gfx.path({{"move", 26, 71}, {"line", 38, 71}}), brush = ink, width = 3, cap = "round"}),
		gfx.stroke({path = rounded(92, 32, 64, 36, 7), brush = dim, width = 3, join = "round"}),
		gfx.stroke({path = gfx.path({{"move", 64, 22}, {"quad", 96, 6, 124, 24}}), brush = ink, width = 3, cap = "round"}),
		gfx.stroke({path = gfx.path({{"move", 112, 20}, {"line", 125, 25}, {"line", 121, 12}}), brush = ink, width = 3, cap = "round", join = "round"}),
	})
	return icon
end

-- The card fills the playfield. `m` is the world: the invisible `rotate:clock` lets smokes see that the match clock
-- (m.time, only moved by Playing steps) stands still while the card shows.
function R.card(m)
	return ui.col({id="rotate:card",absolute=true,full=true,center=true,fill=C.bg,gap=14,on_hover=function() end, -- a hit region: smokes find the card through rects, and it takes the pointer
		
		ui.frame({visual=art()}),
		ui.text({"Rotate your phone to play",color=C.player,font_size=20,no_wrap=true}),
		ui.text({"Jetpack Arena is played in landscape",color=C.muted,font_size=12,no_wrap=true}),
		ui.text({string.format("%s %.3f", m.state, m.time),id="rotate:clock",absolute=true,left=0,top=0,
			font_size=1,opacity=0,no_wrap=true}),
	})
end

return R
