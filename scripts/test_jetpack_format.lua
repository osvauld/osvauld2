-- Jetpack content formats in plain Lua: validation, fallback, and that every app's copy matches.
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local F = require("format")
local baked = require("baked/maps")
local function check(ok, why) assert(ok, why) end
local function copy(t)
	if type(t) ~= "table" then return t end
	local out = {}
	for k, v in pairs(t) do out[k] = copy(v) end
	return out
end

local maps, err = F.maps(baked)
check(#maps == 4 and not err, "baked maps must all validate: " .. tostring(err))
local Y = 4 -- the yard's place in the baked list (outpost, catacombs, rooftops, yard)
local yard = F.find(maps, "yard")
check(baked.maps[Y].id == "yard", "yard is the fourth baked map")
check(yard.w == 2880 and yard.h == 480, "baked yard size")
check(F.spawn(yard, "player").x == 135, "player spawn")
check(F.spawn(yard, "bot", 3).id == "bot1", "bot spawns cycle")

local function broken(edit, expect)
	local t = copy(baked)
	edit(t.maps[Y])
	local m, why = F.map(t.maps[Y])
	check(m == nil and why and why:find(expect, 1, true), "expected '" .. expect .. "', got " .. tostring(why))
end
broken(function(m) m.w = 10 end, "w out of range")
broken(function(m) m.platforms[2].kind = "lava" end, "platform p1: kind")
broken(function(m) m.platforms[2].x = 2800 end, "platform p1: outside the map")
broken(function(m) m.platforms[3].id = "p1" end, "unique id")
broken(function(m) m.spawns = { m.spawns[2] } end, "needs a player spawn")
broken(function(m) m.spawns[1].team = "ghost" end, "team must be")
broken(function(m) m.platforms = "nope" end, "platforms is not a list")

-- An empty asset is silent and falls back; a broken one falls back and names the field.
local loaded, why = F.load_maps(nil, baked)
check(#loaded == 4 and why == nil, "missing asset falls back quietly")
loaded, why = F.load_maps({}, baked)
check(#loaded == 4 and why == nil, "an asset never written falls back quietly")
local bad = copy(baked)
bad.maps[Y].h = -1
loaded, why = F.load_maps(bad, baked)
check(#loaded == 4 and F.find(loaded, "yard").h == 480 and why:find("h out of range"),
	"one broken map is skipped, the rest of the asset plays and the baked one fills in")
local only = copy(baked)
only.maps = { copy(baked.maps[Y]) }
only.maps[1].platforms[2].x = 300
loaded = F.load_maps(only, baked)
check(#loaded == 4 and loaded[1].id == "yard" and loaded[1].platforms[2].x == 300,
	"an asset with one map keeps the other shipped maps")
for _, m in ipairs(bad.maps) do m.h = -1 end
loaded, why = F.load_maps(bad, baked)
check(#loaded == 4 and F.find(loaded, "yard").h == 480 and why:find("h out of range"), "a wholly broken asset falls back")
local mine = copy(baked)
mine.maps[Y].platforms[2].x = 300
loaded = F.load_maps(mine, baked)
check(F.find(loaded, "yard").platforms[2].x == 300, "a valid asset wins over the baked map")

local twice = copy(baked)
table.insert(twice.maps, copy(baked.maps[Y]))
loaded, why = F.maps(twice)
check(#loaded == 4 and why:find("a second entry with this id"), "duplicate map ids are refused")

-- Big arenas: theme and weapon spawns survive validation; bad ones are refused.
local outpost = F.find(maps, "outpost")
check(outpost.w == 4400 and outpost.h == 2000 and outpost.theme == "cliffs", "outpost keeps its theme")
check(#outpost.weapon_spawns == 12 and outpost.weapon_spawns[1].pool[1] == "sniper" and outpost.weapon_spawns[1].respawn == 60,
	"outpost keeps its weapon spawns")
check(yard.weapon_spawns == nil and yard.theme == "yard", "a map without weapon spawns has none")
local function broken_big(edit, expect)
	local t = copy(baked)
	edit(t.maps[1])
	local m, why = F.map(t.maps[1])
	check(m == nil and why and why:find(expect, 1, true), "expected '" .. expect .. "', got " .. tostring(why))
end
broken_big(function(m) m.theme = 4 end, "theme must be")
broken_big(function(m) m.weapon_spawns = "x" end, "weapon_spawns is not a list")
broken_big(function(m) m.weapon_spawns[2].id = "w1" end, "weapon spawn without a unique id")
broken_big(function(m) m.weapon_spawns[1].x = 99999 end, "weapon spawn w1: outside the map")
broken_big(function(m) m.weapon_spawns[1].pool = {} end, "pool must list")
broken_big(function(m) m.weapon_spawns[1].respawn = 0 end, "respawn out of range")
-- The starting loadout (step 5): one or two gun ids, optional.
check(outpost.loadout[1] == "rifle" and outpost.loadout[2] == "pistol" and yard.loadout == nil, "loadout survives; the yard has none")
broken_big(function(m) m.loadout = {} end, "loadout must list")
broken_big(function(m) m.loadout = { "rifle", "pistol", "smg" } end, "loadout must list")
broken_big(function(m) m.loadout = { 4 } end, "loadout holds a gun id")
broken_big(function(m) m.loadout = "rifle" end, "loadout must list")

-- Pilots (format v2): a gfx.drawing module per pilot, plus ids on shapes, a rig of parts and points.
local baked_pilots = require("baked/pilots")
check(baked_pilots.info.version == 2 and F.versions.pilots == 2 and F.versions.maps == 1, "per-kind versions")
local pilots, perr = F.pilots(baked_pilots)
check(#pilots == 1 and not perr, "baked pilots must validate: " .. tostring(perr))
local scout = F.find(pilots, "scout")
check(scout.size[1] == 104 and scout.size[2] == 104 and scout.scale == 0.5, "scout is drawn at 2x")
for _, name in ipairs(F.points) do check(scout.points[name], "scout point " .. name) end
check(#scout.parts == 13 and scout.parts[9].id == "arm_gun" and scout.parts[8].parent == "torso", "scout rig")
local d = F.drawing(scout)
check(d.size[1] == 104 and d.vars.visor and d.id == nil and d.points == nil, "drawing keeps only size, vars, parts")
check(#d.parts == 13 and d.parts[1].shapes[1].id == nil and d.parts[1].shapes[1].path, "shape ids are stripped")
check(d.parts[2].parent == "arm_free" and d.parts[1].parent == nil, "parents survive")
local back, front = F.drawing(scout, "back"), F.drawing(scout, "front")
check(#back.parts == 8 and back.parts[8].id == "head" and #front.parts == 5 and front.parts[1].id == "arm_gun", "two drawings")
check(front.parts[2].parent == "arm_gun" and front.parts[5].parent == "hand_gun", "front parents stay inside it")
local function shape_of(p, id) for _, s in ipairs(p.parts) do if s.id == id then return s end end end
check(shape_of(scout, "torso").shapes[1].fill:sub(1, 1) == "@", "fills keep their var")

local function broken_pilot(edit, expect)
	local t = copy(baked_pilots)
	edit(t.pilots[1])
	local p, why = F.pilot(t.pilots[1])
	check(p == nil and why and why:find(expect, 1, true), "expected '" .. expect .. "', got " .. tostring(why))
end
local PARTS = "parts must be arm_free, forearm_free, hand_free, leg_back, leg_front, jetpack, torso, head, arm_gun"
local function torso(p) return shape_of(p, "torso") end
broken_pilot(function(p) p.size = { 4, 52 } end, "size out of range")
broken_pilot(function(p) p.scale = nil end, "scale")
broken_pilot(function(p) p.scale = 0 end, "scale")
broken_pilot(function(p) p.vars.main = nil end, "needs vars main and dark")
broken_pilot(function(p) p.vars.ink = "red" end, "var ink: not a #hex colour")
broken_pilot(function(p) torso(p).shapes[1].fill = "@nope" end, "fill names no var")
broken_pilot(function(p) torso(p).shapes[1].fill = nil end, "needs a fill")
broken_pilot(function(p) torso(p).shapes[1].path[1] = { "line", 0, 0 } end, "path must start with move")
broken_pilot(function(p) torso(p).shapes[1].path[2] = { "cubic", 1, 2 } end, "path command 2")
broken_pilot(function(p) torso(p).shapes[1].path[2][2] = 1 / 0 end, "path command 2")
broken_pilot(function(p) table.insert(torso(p).shapes[1].path, { "line", 1, 1 }) end, "draws after close")
broken_pilot(function(p) p.vars.ink = "#abc" end, "var ink: not a #hex colour")
broken_pilot(function(p) torso(p).shapes[2].id = torso(p).shapes[1].id end, "unique id")
broken_pilot(function(p) table.remove(p.parts, 1) end, PARTS)
broken_pilot(function(p) p.parts[1], p.parts[13] = p.parts[13], p.parts[1] end, PARTS)
broken_pilot(function(p) p.parts[4], p.parts[5] = p.parts[5], p.parts[4] end, "parts must be")
broken_pilot(function(p) p.parts[2].parent = nil end, "part forearm_free: parent must be arm_free")
broken_pilot(function(p) p.parts[8].parent = "leg_back" end, "part head: parent must be torso")
broken_pilot(function(p) p.parts[1].parent = "torso" end, "part arm_free: parent must be none")
broken_pilot(function(p) p.points.hold = nil end, "points palm_gun, palm_free, hold, jet and eye")
broken_pilot(function(p) p.points.eye = nil end, "points palm_gun")
broken_pilot(function(p) p.points.palm_free = nil end, "points palm_gun")
broken_pilot(function(p) p.parts[1].pivot = nil end, "pivot")
broken_pilot(function(p) p.points.jet = nil end, "points palm_gun")
broken_pilot(function(p) p.points.palm_gun = { 199, 0 } end, "point palm_gun: outside")
broken_pilot(function(p) p.points = nil end, "points palm_gun")

check(#shape_of(scout, "arm_free").shapes > 0 and #shape_of(scout, "hand_gun").shapes > 0
	and #shape_of(scout, "finger_gun").shapes > 0 and #shape_of(scout, "thumb_gun").shapes > 0, "the scout has arms and fingers")

-- Upgrade from v1: an armed five-part pilot and an armless three-part one, both read as v2 at 2x.
local v1 = {
	id = "old", name = "Old", size = { 40, 52 }, vars = { main = "#112233", dark = "#223344", ink = "#000000" },
	points = { grip = { 30, 27 }, hand = { 34, 24 }, jet = { 8, 35 } },
	parts = {
		{ id = "arm_free", pivot = { 23, 24 }, shapes = { { id = "a", fill = "@ink", path = { { "move", 21, 21 }, { "line", 33, 21 }, { "line", 33, 27 }, { "close" } } } } },
		{ id = "leg_back", pivot = { 16, 38 }, shapes = {} },
		{ id = "leg_front", pivot = { 25, 38 }, shapes = {} },
		{ id = "body", pivot = { 20, 30 }, shapes = { { id = "b", fill = "@main", path = { { "move", 4, 22 }, { "line", 28, 22 }, { "line", 28, 39 }, { "close" } } } } },
		{ id = "arm_gun", pivot = { 19, 27 }, shapes = { { id = "g", fill = "@dark", path = { { "move", 17, 24 }, { "line", 31, 24 }, { "line", 31, 30 }, { "close" } } } } },
	},
}
local up, uerr = F.pilot(F.upgrade(v1))
check(up and not uerr, "an armed v1 pilot upgrades: " .. tostring(uerr))
check(up.size[1] == 80 and up.size[2] == 104 and up.scale == 0.5 and #up.parts == 13, "upgraded: size, scale, parts")
check(shape_of(up, "arm_free").shapes[1].id == "a" and shape_of(up, "arm_free").shapes[1].path[2][2] == 66, "arms keep shapes, doubled")
check(shape_of(up, "torso").shapes[1].id == "b" and shape_of(up, "arm_gun").shapes[1].id == "g", "body becomes torso")
check(#shape_of(up, "forearm_gun").shapes == 0 and #shape_of(up, "head").shapes == 0, "the new parts start empty")
check(up.points.palm_gun[1] == 60 and up.points.palm_gun[2] == 54 and up.points.palm_free[1] == 68 and up.points.jet[1] == 16
	and up.points.hold[1] == 60, "points derived from grip, hand and jet")
check(up.parts[9].pivot[1] == 38, "the gun shoulder doubles")
check(F.upgrade(v1) ~= v1 and v1.size[1] == 40, "upgrading copies")
local armless = copy(v1)
table.remove(armless.parts, 5)
table.remove(armless.parts, 1)
armless.points.hand = nil
local bare, berr = F.pilot(F.upgrade(armless))
check(bare and #bare.parts == 13 and #shape_of(bare, "arm_free").shapes == 0 and bare.points.palm_free[1] == 60
	and shape_of(bare, "arm_gun").pivot[1] == 60, "a pilot without arms still loads: " .. tostring(berr))
-- Strict for v2: a v1 pilot is not a v2 pilot.
check(F.pilot(v1) == nil, "F.pilot only takes v2")
-- Assets: a v1 asset is read through the upgrade, a v2 asset as is; wrong versions are refused.
local old_asset = { info = { version = 1 }, pilots = { copy(v1) } }
loaded, why = F.load_pilots(old_asset, baked_pilots)
check(#loaded == 1 and loaded[1].id == "old" and loaded[1].scale == 0.5 and not why, "a v1 pilot asset loads, upgraded")
loaded, why = F.load_pilots({ info = { version = 3 }, pilots = {} }, baked_pilots)
check(#loaded == 1 and loaded[1].id == "scout" and why:find("version"), "an unknown pilot version falls back")
loaded, why = F.load_maps({ info = { version = 2 }, maps = copy(baked.maps) }, baked)
check(#loaded == 4 and loaded[1].id == "outpost" and why:find("version"), "maps stay version 1")

loaded, why = F.load_pilots({}, baked_pilots)
check(#loaded == 1 and why == nil, "pilots: an asset never written falls back quietly")
local twin = copy(baked_pilots)
twin.pilots[1].vars.visor = "#ff00ff"
table.insert(twin.pilots, copy(twin.pilots[1]))
twin.pilots[2].id = "twin"
loaded = F.load_pilots(twin, baked_pilots)
check(#loaded == 2 and loaded[1].vars.visor == "#ff00ff", "a valid pilot asset wins")
check(F.pilot_for(loaded, "player", 1).id == "scout" and F.pilot_for(loaded, "bot", 1).id == "scout"
	and F.pilot_for(loaded, "bot", 2).id == "twin", "player flies the first pilot, bots cycle")

-- Guns (format v1): a drawing with points and stats; strict like the rest.
local baked_guns = require("baked/guns")
check(baked_guns.info.version == 1 and F.versions.guns == 1, "guns are version 1")
local guns, gerr = F.guns(baked_guns)
check(#guns == 6 and not gerr, "baked guns must validate: " .. tostring(gerr))
local rifle, pistol = F.find(guns, "rifle"), F.find(guns, "pistol")
check(rifle.id == "rifle" and rifle.points.support and pistol.points.support == nil, "pistol is one-handed")
check(rifle.stats.kind == "bullet" and F.find(guns, "rocket").stats.splash == 70, "stats read through")
local gd = F.gun_drawing(rifle)
check(gd.size[1] == 100 and gd.vars.metal and gd.id == nil and gd.points == nil and gd.stats == nil, "gun drawing keeps size, vars, parts")
check(#gd.parts == #rifle.parts and gd.parts[1].shapes[1].id == nil and gd.parts[1].shapes[1].path, "gun shape ids are stripped")
check(gd.parts[1].pivot and gd.parts[1].id, "gun parts keep id and pivot")
check(F.find(guns, "nope").id == guns[1].id, "find falls back to the first gun")

local function broken_gun(edit, expect)
	local t = copy(baked_guns)
	edit(t.guns[3])
	local g, why = F.gun(t.guns[3])
	check(g == nil and why and why:find(expect, 1, true), "expected '" .. expect .. "', got " .. tostring(why))
end
local function gpart(g, id) for _, p in ipairs(g.parts) do if p.id == id then return p end end end
broken_gun(function(g) g.id = "" end, "needs an id")
broken_gun(function(g) g.size = { 4, 30 } end, "size out of range")
broken_gun(function(g) g.scale = 0 end, "scale")
broken_gun(function(g) g.vars.ink = "red" end, "var ink: not a #hex colour")
broken_gun(function(g) gpart(g, "body").shapes[1].fill = "@nope" end, "fill names no var")
broken_gun(function(g) gpart(g, "body").shapes[1].fill = nil end, "needs a fill")
broken_gun(function(g) gpart(g, "body").shapes[1].path[1] = { "line", 0, 0 } end, "path must start with move")
broken_gun(function(g) gpart(g, "body").shapes[1].path[2] = { "cubic", 1, 2 } end, "path command 2")
broken_gun(function(g) table.insert(gpart(g, "body").shapes[1].path, { "line", 1, 1 }) end, "draws after close")
broken_gun(function(g) gpart(g, "body").shapes[2].id = gpart(g, "body").shapes[1].id end, "unique id")
broken_gun(function(g) table.remove(g.parts, 2) end, "needs the parts body and mag")
broken_gun(function(g) g.parts[1].id = "wheel" end, "part wheel")
broken_gun(function(g) g.parts[1], g.parts[2] = g.parts[2], g.parts[1] end, "parts must be in the order")
broken_gun(function(g) g.parts[1].pivot = nil end, "pivot")
broken_gun(function(g) g.points.grip = nil end, "needs points grip, muzzle, eject and mag")
broken_gun(function(g) g.points.muzzle = { 300, 4 } end, "point muzzle: outside")
broken_gun(function(g) g.points.barrel = { 3, 4 } end, "point barrel: not a gun point")
broken_gun(function(g) g.stats.damage = 5000 end, "stat damage")
broken_gun(function(g) g.stats.fire_interval = 0 end, "stat fire_interval")
broken_gun(function(g) g.stats.pellets = 2.5 end, "stat pellets")
broken_gun(function(g) g.stats.auto = "yes" end, "stat auto")
broken_gun(function(g) g.stats.kind = "laser" end, "stat kind")
broken_gun(function(g) g.stats.splash = 40 end, "splash is for rockets")
broken_gun(function(g) g.stats.mag_size = nil end, "stat mag_size")
broken_gun(function(g) g.stats = nil end, "needs stats")
-- zoom levels: `zooms` is an ascending list from 1; a v1 `zoom` number upgrades
local want_zooms = { pistol = { 1, 1.25 }, smg = { 1, 1.25 }, shotgun = { 1, 1.25 }, rifle = { 1, 1.5, 2 },
	sniper = { 1, 2, 3, 4 }, rocket = { 1, 1.5, 2 } }
for id, want in pairs(want_zooms) do
	local z = F.find(guns, id).stats.zooms
	check(#z == #want, id .. " zoom levels")
	for i, v in ipairs(want) do check(z[i] == v, id .. " zoom level " .. i) end
	check(F.find(guns, id).stats.zoom == nil, id .. " has no old zoom stat")
end
broken_gun(function(g) g.stats.zooms = { 1.5, 2 } end, "zooms")
broken_gun(function(g) g.stats.zooms = { 1, 2, 2 } end, "zooms")
broken_gun(function(g) g.stats.zooms = { 1, 3, 2 } end, "zooms")
broken_gun(function(g) g.stats.zooms = { 1, 9 } end, "zooms")
broken_gun(function(g) g.stats.zooms = { 1, 0.5 } end, "zooms")
broken_gun(function(g) g.stats.zooms = {} end, "zooms")
broken_gun(function(g) g.stats.zooms = { 1, 1.5, 2, 2.5, 3, 3.5, 4 } end, "zooms")
broken_gun(function(g) g.stats.zooms = { 1, "2" } end, "zooms")
broken_gun(function(g) g.stats.zooms = 2 end, "zooms")
broken_gun(function(g) g.stats.zooms = nil; g.stats.zoom = nil end, "zooms")
broken_gun(function(g) g.stats.zooms = nil; g.stats.zoom = 9 end, "zoom")
local function upgraded(zoom, zooms)
	local t = copy(baked_guns.guns[5])
	t.stats.zooms, t.stats.zoom = zooms, zoom
	return F.gun(t)
end
local v1 = upgraded(1.6, nil)
check(v1 and #v1.stats.zooms == 2 and v1.stats.zooms[1] == 1 and v1.stats.zooms[2] == 1.6 and v1.stats.zoom == nil, "v1 zoom 1.6 upgrades to {1, 1.6}")
v1 = upgraded(1, nil)
check(v1 and #v1.stats.zooms == 1 and v1.stats.zooms[1] == 1, "v1 zoom 1 upgrades to {1}")
v1 = upgraded(0.8, nil)
check(v1 and #v1.stats.zooms == 1, "v1 zoom below 1 upgrades to {1}")
v1 = upgraded(2.5, { 1, 3 })
check(v1 and v1.stats.zooms[2] == 3, "zooms wins over a stale zoom")
local rocket = copy(baked_guns.guns[6])
rocket.stats.splash = nil
check(F.gun(rocket) == nil, "a rocket needs splash")
local noshape = copy(baked_guns.guns[3])
gpart(noshape, "mag").shapes = {}
check(F.gun(noshape), "an empty part is fine while drawing")

loaded, why = F.load_guns(nil, baked_guns)
check(#loaded == 6 and why == nil, "guns: a missing asset falls back quietly")
loaded, why = F.load_guns({}, baked_guns)
check(#loaded == 6 and why == nil, "guns: an asset never written falls back quietly")
local badg = copy(baked_guns)
badg.guns[1].stats.damage = -1
loaded, why = F.load_guns(badg, baked_guns)
check(#loaded == 5 and why:find("stat damage"), "a broken gun is skipped and named")
local onlyg = copy(baked_guns)
onlyg.guns = { onlyg.guns[3] }
onlyg.guns[1].stats.damage = 17
loaded = F.load_guns(onlyg, baked_guns)
check(#loaded == 1 and loaded[1].stats.damage == 17, "a valid gun asset wins")
loaded, why = F.load_guns({ info = { version = 2 }, guns = copy(baked_guns.guns) }, baked_guns)
check(#loaded == 6 and why:find("version"), "an unknown gun version falls back")

-- Each editor ships the same shared modules as the game.
local function read(path)
	local f = io.open(path, "rb")
	if not f then return nil end
	local s = f:read("a")
	f:close()
	return s
end
local shared = {
	["format.lua"] = { "jetpack_map_editor", "jetpack_pilot_editor", "jetpack_gun_editor" },
	["pilot_art.lua"] = { "jetpack_pilot_editor", "jetpack_gun_editor" },
	["gun_art.lua"] = { "jetpack_pilot_editor", "jetpack_gun_editor" },
	["rig.lua"] = { "jetpack_pilot_editor", "jetpack_gun_editor" },
	["baked/guns.lua"] = { "jetpack_pilot_editor", "jetpack_gun_editor", "jetpack_map_editor" },
	["baked/pilots.lua"] = { "jetpack_pilot_editor", "jetpack_gun_editor" },
}
for file, apps in pairs(shared) do
	local canonical = read("demo_apps/jetpack/" .. file)
	for _, app in ipairs(apps) do
		check(read("demo_apps/" .. app .. "/" .. file) == canonical, app .. "/" .. file .. " differs from jetpack/" .. file)
	end
end
print("jetpack format ok")
