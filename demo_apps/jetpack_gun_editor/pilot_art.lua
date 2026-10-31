-- A pilot asset (format.lua) drawn as the game draws it: rig.lua poses the arms around the gun (gun_art.lua),
-- which draws between two drawings of the pilot (free arm, legs, jetpack, torso, head behind; the gun arm
-- over the grip), the flame at `jet`, mirrored with facing. The pilot is drawn at 2×: this module
-- works in game units, `pilot.scale` per drawing unit. Shared by the game and the pilot editor's preview
-- (each app ships an identical copy; scripts/test_jetpack_format.lua checks they match).
local F = require("format")
local R = require("rig")
local Gun = require("gun_art")
local P = {}

P.colors = {
	ink = "#182e34", edge = "#729a87", flame = "#fbd484", flame_edge = "#e99563", shot = "#ffe8a3",
	flash_outer = "#ff9a2e", flash_core = "#fffbd6",
	visor = "#7fdddc", sky = "#d7ded0", -- sky: the game's, behind previews
}
P.teams = { player = { main = "#d5e3a8", dark = "#709a78" }, bot = { main = "#e99563", dark = "#a65e4d" } }
local K = P.colors

local brushes = {}
local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end
local function poly(points, color)
	local commands = { { "move", points[1][1], points[1][2] } }
	for i = 2, #points do table.insert(commands, { "line", points[i][1], points[i][2] }) end
	table.insert(commands, { "close" })
	return gfx.fill({ path = gfx.path(commands), brush = brush(color) })
end
local function instance(visual, x, y, angle, scale)
	local c, s = math.cos(angle or 0) * (scale or 1), math.sin(angle or 0) * (scale or 1)
	return gfx.instance({ visual = visual, transform = { c, s, -s, c, x, y } })
end

local flame = gfx.frame({ width = 12, height = 32,
	poly({ { 0, 0 }, { 12, 0 }, { 9, 17 }, { 6, 32 }, { 3, 17 } }, K.flame_edge),
	poly({ { 3, 0 }, { 9, 0 }, { 8, 12 }, { 6, 23 }, { 4, 12 } }, K.flame),
})
-- Muzzle flashes by style (fire_fx.lua's flash_style), pointing +x from the muzzle, in game units. Each has a
-- hot outer shape and a pale core. `rocket` is the front puff; `blast` is its back-blast at the vent.
local function flash_frame(outer, core, w, h)
	return gfx.frame({ width = w, height = h, poly(outer, K.flash_outer), poly(core, K.flash_core) })
end
local flashes = {
	small = flash_frame({ { 0, -3 }, { 6, -6 }, { 5, -1.5 }, { 14, 0 }, { 5, 1.5 }, { 6, 6 }, { 0, 3 } },
		{ { 0, -1.8 }, { 3, -3 }, { 7, 0 }, { 3, 3 }, { 0, 1.8 } }, 14, 12),
	long = flash_frame({ { 0, -3 }, { 8, -5.5 }, { 10, -1.6 }, { 28, 0 }, { 10, 1.6 }, { 8, 5.5 }, { 0, 3 } },
		{ { 0, -1.8 }, { 15, 0 }, { 0, 1.8 } }, 28, 12),
	wide = flash_frame({ { 0, -3 }, { 13, -15 }, { 18, -8 }, { 25, -4 }, { 28, 0 }, { 25, 4 }, { 18, 8 }, { 13, 15 }, { 0, 3 } },
		{ { 0, -2 }, { 12, -6 }, { 19, 0 }, { 12, 6 }, { 0, 2 } }, 28, 30),
	rocket = flash_frame({ { 0, -4 }, { 7, -8 }, { 9, -3 }, { 17, 0 }, { 9, 3 }, { 7, 8 }, { 0, 4 } },
		{ { 0, -2.4 }, { 4, -4 }, { 8, 0 }, { 4, 4 }, { 0, 2.4 } }, 17, 16),
	blast = flash_frame({ { 0, -4 }, { 10, -10 }, { 14, -3.5 }, { 36, 0 }, { 14, 3.5 }, { 10, 10 }, { 0, 4 } },
		{ { 0, -2.5 }, { 8, -5 }, { 20, 0 }, { 8, 5 }, { 0, 2.5 } }, 36, 20),
}
-- The old 0/1 flash of the editors' previews.
local legacy = { style = "small", t = 0 }

-- Two compiled drawings per pilot table and team (the gun draws between them), their poses cached by
-- whole degrees of every part (posing builds a frame). A new pilot table (an edit) compiles afresh.
local compiled = {}
local function posed(pilot, team, pose)
	local key = pilot.id .. "/" .. team
	local entry = compiled[key]
	if not entry or entry.pilot ~= pilot or entry.count > 2000 then
		local vars = { vars = P.teams[team] }
		entry = { pilot = pilot, back = gfx.drawing(F.drawing(pilot, "back"), vars),
			front = gfx.drawing(F.drawing(pilot, "front"), vars), poses = {}, count = 0 }
		compiled[key] = entry
	end
	local buf, back, front = {}, {}, {}
	for i, id in ipairs(F.parts) do
		local deg = math.floor(pose.parts[id].rot + 0.5)
		buf[i] = deg
		if deg ~= 0 then (i <= F.split and back or front)[id] = { rot = deg } end
	end
	local at = table.concat(buf, ",")
	local made = entry.poses[at]
	if not made then
		made = { entry.back:pose(back), entry.front:pose(front) }
		entry.poses[at] = made
		entry.count = entry.count + 1
	end
	return made[1], made[2]
end

-- Where the actor's frame goes: the pilot's feet line sits where the 40×52 scout's did.
function P.anchor(pilot)
	return pilot.size[1] * pilot.scale / 2, pilot.size[2] * pilot.scale - 24
end

-- `a`: grounded, vx, thrusting, aim (radians), flash and shield (> 0 shows them); `gun` is a checked gun
-- (format.lua) the soldier holds, or none: empty hands. The free hand holds the gun's support point; a
-- one-handed gun lets that arm hang and swing with the walk. `fx` (fire_fx.lua's state; none: the editors'
-- previews, where `a.flash` > 0 squeezes the trigger and shows a small flash): { back, climb, trigger, flash } -
-- the gun kicks `back` game units along the barrel and its muzzle climbs `climb` radians with both hands
-- following, the finger squeezes, and the flash `{ style, t }` shows at the muzzle (a rocket's also at its vent).
-- Returns the frame and, in the actor's frame, { muzzle, eject, aim (the gun's real angle), facing } for effects.
function P.actor(a, time, pilot, team, gun, fx)
	local walk = a.grounded and math.sin(time * 17) * math.min(1, math.abs(a.vx) / 100) or 0.3
	local s = pilot.scale
	local w, h = pilot.size[1] * s, pilot.size[2] * s
	local hold = gun and R.gun_hold(gun, pilot) or {}
	hold.walk, hold.swing = walk, walk * 0.25
	if not fx and a.flash and a.flash > 0 then fx = { back = 0, climb = 0, trigger = math.min(1, a.flash), flash = legacy } end
	fx = fx or { back = 0, climb = 0, trigger = 0 }
	hold.trigger = fx.trigger
	hold.kick, hold.climb = fx.back / s, fx.climb
	local pose = R.pose(pilot, a.aim, hold)
	local facing = pose.facing
	local jet = pilot.points.jet
	local result = { width = w, height = h }
	if a.thrusting then
		local scale = 0.65 + math.sin(time * 55) * 0.08
		local jx = (facing < 0 and pilot.size[1] - jet[1] or jet[1]) * s
		table.insert(result, instance(flame, jx - 6 * scale, jet[2] * s, 0, scale))
	end
	local back, front = posed(pilot, team, pose)
	local stand = facing < 0 and { -s, 0, 0, s, w, 0 } or { s, 0, 0, s, 0, 0 }
	table.insert(result, gfx.instance({ visual = back, transform = stand }))
	local mount = { pose.gun.x * s, pose.gun.y * s }
	local angle = pose.gun.angle
	if gun then table.insert(result, Gun.item(gun, mount, angle, facing)) end
	table.insert(result, gfx.instance({ visual = front, transform = stand }))
	local info
	if gun then
		info = { muzzle = R.gun_point(gun, "muzzle", mount, angle, facing), eject = R.gun_point(gun, "eject", mount, angle, facing),
			aim = angle, facing = facing }
		local f = fx.flash
		if f then
			local k = 1 - 0.35 * f.t
			local m = info.muzzle
			table.insert(result, instance(flashes[f.style], m[1], m[2], angle, k))
			if f.style == "rocket" then
				local v = info.eject
				table.insert(result, instance(flashes.blast, v[1], v[2], angle + math.pi, k))
			end
		end
	end
	if a.shield and a.shield > 0 then
		table.insert(result, gfx.stroke({ path = gfx.path({ { "move", 4, 1 }, { "line", w - 4, 1 } }), brush = brush(K.visor),
			width = 2, cap = "round" }))
	end
	return gfx.frame(result), info
end

return P
