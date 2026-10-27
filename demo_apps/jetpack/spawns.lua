local F = require("format")
local Weapons = require("weapons")
local Bots = require("bots")
local S = {}

-- Soldiers carry two guns: `a.gun` (in hand) and `a.other` (on the back), each with its own mag
-- (`a.ammo`; `a.other_ammo`, nil = never used, full). A swap takes `swap_time` and no firing meanwhile.
-- Guns lie on the floor as `w.loose` entries { gun, x, y, vy, rest, ammo (nil = full), ttl (drops
-- only), pad }; a map's weapon spawn is a pad in `w.pads` that refills `respawn` seconds after its
-- gun is taken. Design: design/jetpack-militia.md step 5.
S.default_loadout = { "rifle", "pistol" }  -- Mini Militia starts with a main gun and a pistol-class sidearm
S.swap_time = 0.3
S.reach = { x = 44, y = 40 }               -- how near the soldier's centre a gun must lie to be taken
S.lie = 14                                 -- a resting gun's centre is this far above the surface
S.drop_ttl = 20                            -- seconds a dropped gun lies before it despawns
S.max_loose = 12                           -- dropped guns at once; the oldest goes first (pad guns do not count)
S.fall = 900                               -- a dropped gun falls at this acceleration

function S.actor(id, x, y, hp)
	return { id = id, gun = "rifle", x = x, y = y, vx = 0, vy = 0, hp = hp, fuel = 100,
		aim = 0, fire_clock = 0, respawn = 0, shield = 0, thrusting = false, grounded = false, swap_left = 0 }
end

-- The guns a soldier starts (and respawns) with: the `jetpack-loadout` test seam (`w.arm`, a gun id for
-- the player and for bots), else the map's `loadout`, else the default. Unknown gun ids are skipped.
local function loadout_for(w, a)
	local arm = w.arm and (a == w.player and w.arm.player or (a ~= w.player and w.arm.bot)) or nil
	local ids = arm and { arm } or w.map.loadout or S.default_loadout
	local out = {}
	for _, id in ipairs(ids) do
		if w.gun_by_id[id] and #out < 2 then table.insert(out, id) end
	end
	if #out == 0 then out[1] = w.guns[1].id end
	return out
end

-- Full mags, the loadout in hand.
function S.equip(w, a)
	local ids = loadout_for(w, a)
	a.gun, a.other, a.other_ammo = ids[1], ids[2], nil
	a.ammo_gun, a.reload_left, a.swap_left, a.pending = nil, nil, 0, false
	Weapons.sync(w, a)
end

-- Test seam: the `jetpack-loadout` asset names one gun for the player and one for bots.
function S.arm(w, loadout)
	if not loadout then return end
	w.arm = { player = loadout.player, bot = loadout.bot }
	S.equip(w, w.player)
	for _, b in ipairs(w.bots) do S.equip(w, b) end
end

-- Weighted pick from `pool` (a gun listed twice is twice as likely) among the guns the game has;
-- the world's RNG decides, so the same match rolls the same guns.
function S.roll(w, pool)
	local known = {}
	for _, id in ipairs(pool) do if w.gun_by_id[id] then table.insert(known, id) end end
	if #known == 0 then return nil end
	if #known == 1 then return known[1] end
	return known[math.floor(Weapons.random(w) * #known) + 1]
end

local function remove(list, item)
	for i, v in ipairs(list) do if v == item then table.remove(list, i); return end end
end

-- A gun on the floor at (x, y); drops fall to the surface and despawn, the oldest first past the cap.
-- `rest` keeps it where it is (a gun swapped for the one lying there stays put, even where a map's author
-- hung it in the air).
function S.drop(w, gun, ammo, x, y, rest)
	local l = { gun = gun, ammo = ammo, x = x, y = y, vy = 0, rest = rest or false, ttl = S.drop_ttl }
	table.insert(w.loose, l)
	local count = 0
	for _, g in ipairs(w.loose) do if g.ttl then count = count + 1 end end
	for i = 1, #w.loose do
		if count <= S.max_loose then break end
		if w.loose[i].ttl then table.remove(w.loose, i); count = count - 1; break end
	end
	return l
end

local function fill_pad(w, pad)
	local gun = S.roll(w, pad.pool)
	if not gun then pad.timer = pad.spawn.respawn; return end
	pad.loose = { gun = gun, x = pad.spawn.x, y = pad.spawn.y, vy = 0, rest = true, pad = pad }
	table.insert(w.loose, pad.loose)
end

-- Put the player and up to `max_bots` bots on the map's spawn points, and fill the weapon spawns.
function S.populate(w, max_bots)
	local you = F.spawn(w.map, "player")
	w.player = S.actor("player", you.x, you.y, 100)
	w.player.spawn = you
	w.bots = {}
	local n = 0
	for _, s in ipairs(w.map.spawns) do if s.team == "bot" then n = n + 1 end end
	for i = 1, math.min(n, max_bots) do
		local s = F.spawn(w.map, "bot", i)
		local b = S.actor("bot:" .. i, s.x, s.y, 60)
		b.spawn = s
		b.damage_scale = Bots.damage_scale
		table.insert(w.bots, b)
	end
	S.equip(w, w.player)
	for _, b in ipairs(w.bots) do S.equip(w, b) end
	w.loose, w.pads = {}, {}
	for _, ws in ipairs(w.map.weapon_spawns or {}) do
		local pad = { spawn = ws, pool = ws.pool, timer = 0 }
		table.insert(w.pads, pad)
		fill_pad(w, pad)
	end
end

-- The loose gun nearest to `a`'s centre within reach, or nil.
function S.nearby(w, a)
	local best, score = nil, math.huge
	for _, l in ipairs(w.loose) do
		local dx, dy = math.abs(a.x - l.x), math.abs(a.y - l.y)
		if dx <= S.reach.x and dy <= S.reach.y and dx + dy * 0.5 < score then best, score = l, dx + dy * 0.5 end
	end
	return best
end

-- Swap guns in hand. Not while dead, mid-swap, or with nothing on the back.
function S.swap(w, a)
	if a.hp <= 0 or not a.other or (a.swap_left or 0) > 0 then return false end
	Weapons.sync(w, a)
	local held = a.ammo
	a.gun, a.other = a.other, a.gun
	local back = a.other_ammo
	a.other_ammo = held
	a.reload_left, a.pending, a.swap_left = nil, false, S.swap_time
	a.ammo_gun = nil
	if back then a.ammo, a.ammo_gun = back, a.gun end
	Weapons.sync(w, a) -- a gun never used fills its mag
	return true
end

-- Take `l` (default: the nearest in reach). It replaces the gun in hand, which drops where `l` lay -- or,
-- with the back slot empty, moves there. A pad's gun comes full; a dropped one with the ammo it had.
function S.pickup(w, a, l)
	l = l or S.nearby(w, a)
	if not l or a.hp <= 0 or (a.swap_left or 0) > 0 then return false end
	Weapons.sync(w, a)
	remove(w.loose, l)
	if l.pad then l.pad.loose, l.pad.timer = nil, l.pad.spawn.respawn end
	if a.other == nil then
		a.other, a.other_ammo = a.gun, a.ammo
	else
		S.drop(w, a.gun, a.ammo, l.x, l.y, l.rest)
	end
	a.gun, a.ammo_gun, a.reload_left, a.pending, a.swap_left = l.gun, nil, nil, false, S.swap_time
	if l.ammo then a.ammo, a.ammo_gun = l.ammo, l.gun end
	Weapons.sync(w, a)
	return true
end

-- Soldier index 1 is the player. Returns true while the soldier is alive and should act; a dead one drops
-- the gun in hand (once), counts down and returns at its spawn with the loadout.
function S.tick(w, a, index, dt)
	if a.hp > 0 then a.dropped = false; return true end
	if not a.dropped then
		a.dropped = true
		Weapons.sync(w, a)
		S.drop(w, a.gun, a.ammo, a.x, a.y)
	end
	a.respawn = a.respawn - dt
	if a.respawn <= 0 then
		a.x, a.y = a.spawn.x, a.spawn.y
		a.hp, a.vx, a.vy, a.fuel = index == 1 and 100 or 60, 0, 0, 100
		a.exhausted = false
		a.shield, a.fire_clock = 1.2, 0
		S.equip(w, a)
	end
	return false
end

-- Per step: pads refill, drops fall and expire.
function S.update_pickups(w, dt)
	for _, pad in ipairs(w.pads) do
		if not pad.loose then
			pad.timer = pad.timer - dt
			if pad.timer <= 0 then fill_pad(w, pad) end
		end
	end
	for i = #w.loose, 1, -1 do
		local l = w.loose[i]
		if l.ttl then
			l.ttl = l.ttl - dt
			if l.ttl <= 0 then table.remove(w.loose, i) end
		end
		if not l.rest then
			l.vy = l.vy + S.fall * dt
			local ny = l.y + l.vy * dt
			for _, r in ipairs(w.solids) do
				if l.x >= r.x and l.x <= r.x + r.w and l.y + S.lie <= r.y + 6 and ny + S.lie >= r.y then
					ny, l.vy, l.rest = r.y - S.lie, 0, true
				end
			end
			l.y = ny
		end
	end
end

return S
