local C = require("theme")
local Sticks = require("sticks")
local H = {}

local function text(value,id,color,font)
	return ui.text({value,id=id,color=color or C.text,font_size=font or 12,no_wrap=true})
end
local function button(value,id,action)
	return ui.button({id=id,h=40,min_w=44,px=14,radius=8,center=true,fill=C.panel,hover_fill=C.steel,
		stroke={1,C.edge},on_click=action,text(value,nil,C.text,13)})
end

-- `g` is main.lua's game table; `on` supplies start/pause/resume/restart.
function H.bar(g, on, portrait)
	local m = g.m
	local ammo, mag, reloading = m:ammo()
	return ui.row({h=42,w_full=true,px=10,gap=8,align_center=true,fill=C.bg,
		g.size.w>=500 and text("SKYWARD",nil,C.player,16) or false,text("KILLS "..m.kills,"hud:kills"),
		text("HP "..m.player.hp,"hud:health"),text("FUEL "..math.floor(m.player.fuel),"hud:fuel"),
		text(ammo.."/"..mag,"hud:ammo",reloading and C.danger or C.text),
		reloading and text("RELOADING","hud:reload",C.danger) or false,
		ui.col({grow=true}),
		-- the rotate card owns the playfield in portrait: nothing to press until the phone is turned
		not portrait and button(m.state=="Playing" and "II" or "···","match:pause",function()
			if m.state=="Playing" then on.pause() elseif m.state=="Paused" then on.resume() end
		end) or false,
		not portrait and button("↻","match:restart",on.restart) or false,
	})
end

function H.overlay(g, on)
	local m = g.m
	if m.state~="Ready" and m.state~="Paused" then return false end
	-- a landscape phone (360 px tall) drops the flavour lines so the arena picker below has room
	local compact = g.size.h < 420
	local tight = compact and g.size.w < 600 -- a small desktop window: the arena picker wraps, so the stick hints go too
	return ui.col({absolute=true,full=true,center=true,fill=C.scrim,gap=compact and 8 or 14,
		text(m.state=="Ready" and "SKYWARD" or "FLIGHT PAUSED",nil,C.player,28),
		text("JETPACK ARENA  /  SOLO TRAINING",nil,C.muted,11),
		not compact and text("Two airborne rivals. One training yard.",nil,C.text,12) or false,
		button(m.state=="Ready" and "DEPLOY" or "RESUME",m.state=="Ready" and "match:start" or "match:resume",
			m.state=="Ready" and on.start or on.resume),
		m.state=="Paused" and button("NEW MATCH","match:new",on.restart) or false,
		not tight and text("Left stick: move  ·  push up to fly",nil,C.muted,11) or false,
		not tight and text("Right stick: aim  ·  push to the ring to fire",nil,C.muted,11) or false,
		not compact and text("Keyboard: A/D · Space · mouse + F · R reload · Q swap · E pick up · Z / wheel zoom",nil,C.muted,11) or false,
	})
end

-- Small readouts and warnings over the playfield; returned as a list.
function H.status(g)
	local m, map, size = g.m, g.map, g.size
	local live=0
	for _,b in ipairs(m.bots) do if b.hp>0 then live=live+1 end end
	local p = m.player
	local gun = m.gun_by_id[p.gun] or m.guns[1]
	local zoom = g.controls and g.controls:zoom_info() or 1
	local back, back_ammo = m:other()
	local near = m.state=="Playing" and p.hp>0 and m:nearby() or nil
	local lying = near and m.gun_by_id[near.gun]
	return {
		ui.row({absolute=true,top=4,left=10,gap=8,align_center=true,px=7,py=3,radius=6,fill=C.jet_bg,
			text(string.upper(gun.name),"hud:gun",C.text,13),
			zoom>1 and text(Sticks.zoom_label(zoom),"hud:zoom",C.flame,11) or false,
			back and text("Q  "..string.upper(back.name).." "..back_ammo,"hud:other",C.muted,10) or false,
		}),
		ui.col({absolute=true,top=32,left=10,gap=3,
			ui.row({gap=12,
				text(m.state,"match:state",C.muted,10),
				text(map.name,"map:name",C.muted,10),
				text("BOTS "..live,"hud:bots",C.muted,10),
				text("DEATHS "..m.deaths,"hud:deaths",C.muted,10),
				text("SHOTS "..m.fired,"hud:fired",C.muted,10),
				text("ALT "..math.floor(map.h-C.ry-m.player.y+0.5),"hud:altitude",C.muted,10),
			}),
		}),
		lying and ui.col({absolute=true,top=64,w_full=true,align_center=true,
			ui.row({px=9,py=4,radius=8,fill=C.jet_bg,
				text("PICK UP "..string.upper(lying.name),"hud:pickup",C.flame,13)})}) or false,
		m.player.hp<=0 and ui.col({absolute=true,top=40,w_full=true,center=true,
			text("DOWNED · deploying in "..math.ceil(m.player.respawn).."s",nil,C.danger,18)}) or false,
		m.player.hp>0 and m.player.fuel<=0 and ui.col({absolute=true,top=40,w_full=true,center=true,
			text("FUEL EMPTY · release thrust to recharge",nil,C.danger,12)}) or false,
	}
end

return H
