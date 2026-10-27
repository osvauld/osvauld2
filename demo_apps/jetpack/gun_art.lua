-- A gun asset (format.lua) drawn as the game draws it: compiled once per gun table,
-- placed so its `grip` lands on the rig's gun point at the aim, its y flipped facing left so the handle
-- stays below. The pure placement (points, transform) is rig.lua's; this module only owns the gfx handles.
-- Shared by the game and the editors' previews (each app ships an identical copy;
-- scripts/test_jetpack_format.lua checks they match).
local F = require("format")
local R = require("rig")
local G = {}

-- Compiled once per gun table: an edit makes a new table and compiles afresh. Guns are one palette, the
-- same for every team (a gun's colours are its own vars).
local compiled = {}
local function frame(gun)
	local entry = compiled[gun.id]
	if not entry or entry.gun ~= gun then
		entry = { gun = gun, frame = gfx.drawing(F.gun_drawing(gun)):pose({}) }
		compiled[gun.id] = entry
	end
	return entry.frame
end

-- The gfx item drawing `gun` with its grip at `mount` (game units, the actor's frame).
function G.item(gun, mount, aim, facing)
	return gfx.instance({ visual = frame(gun), transform = R.gun_transform(gun, mount, aim, facing) })
end

-- Where the gun's named points are (muzzle, eject, mag, support, grip) in the same space.
function G.points(gun, mount, aim, facing)
	local out = {}
	for _, name in ipairs(F.gun_points) do out[name] = R.gun_point(gun, name, mount, aim, facing) end
	return out
end

return G
