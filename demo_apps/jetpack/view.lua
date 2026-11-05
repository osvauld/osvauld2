local Art = require("art")
local Scenery = require("scenery")
local F = require("format")
local Pickups = require("pickup_art")
local V = {}

-- The camera's geometry for the current playfield size, framed on the player.
function V.geometry(g)
	local p = g.m.player
	return g.camera:geometry(g.size.w, g.size.h, p.x, p.y)
end

-- The clipped map viewport: themed background (parallax layers), actors and effects. `g.pilots` is the
-- validated pilot list (the player flies the first, bots cycle it) and `g.guns` the gun list; a soldier holds
-- the gun its `gun` field names (the first gun if it names none).
function V.scene(g)
	local m, map = g.m, g.map
	local geo = V.geometry(g)
	local scale = geo.scale
	local actors={}
	local humans=#m.humans
	for index,a in ipairs(m.all(m)) do
		if a.hp>0 then
			local team=a.team
			-- Each human flies their own pilot (the first one alone, as ever); bots cycle the list.
			local pilot=index<=humans and g.pilots[(index-1)%math.max(1,#g.pilots)+1] or F.pilot_for(g.pilots,team,index-humans)
			local gun=F.find(g.guns,a.gun)
			local ax,ay=Art.anchor(pilot)
			table.insert(actors,ui.frame({id=a.id,visual=Art.soldier(m,a,pilot,team,gun),absolute=true,
				left=(a.x-ax-geo.x)*scale+geo.ox,top=(a.y-ay-geo.y)*scale+geo.oy,scale=scale,
				on_hover=function() end}))
		end
	end
	return ui.col({id="map:viewport",absolute=true,left=0,top=0,w=g.size.w,h=g.size.h,clip=true,
		fill=Scenery.theme(map).rock,
		Scenery.view(map, geo),
		-- step 5: guns on the floor, under the soldiers
		ui.frame({visual=Pickups.layer(g,geo),absolute=true,left=geo.ox-geo.x*scale,top=geo.oy-geo.y*scale,scale=scale}),
		actors,
		ui.frame({visual=Art.effects(m),absolute=true,left=geo.ox-geo.x*scale,top=geo.oy-geo.y*scale,scale=scale}),
		-- Smokes read the camera here ("x y scale": world units at the playfield's top-left, px per unit); never drawn.
		ui.text({string.format("%.3f %.3f %.5f",geo.x,geo.y,scale),id="camera:view",absolute=true,left=0,top=0,
			font_size=1,opacity=0,no_wrap=true}),
		-- The player's aim angle in radians, for smokes; never drawn.
		ui.text({string.format("%.4f",m.player.aim),id="aim:angle",absolute=true,left=0,top=0,font_size=1,opacity=0,no_wrap=true}),
	})
end

return V
