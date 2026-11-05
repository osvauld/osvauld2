-- Weapon spawns, pickups, loadout and swap in the model (design/jetpack-militia.md step 5, test 5), on the
-- virtual clock:  lua scripts/test_jetpack_pickups.lua   (from the worktree root)
math.clamp = math.clamp or function(v, lo, hi) return math.max(lo, math.min(hi, v)) end
math.atan2 = math.atan2 or function(y, x) return math.atan(y, x) end
package.path = "demo_apps/jetpack/?.lua;scripts/?.lua;" .. package.path
local World = require("jetpack_solo")
local Spawns = require("spawns")
local Weapons = require("weapons")
local Bots = require("bots")
local C = require("theme")
local DT = C.step
local function check(ok, why) assert(ok, why) end

-- A flat floor (top at 600) with the player at 100 and one bot far away; `pads` are weapon spawns.
local function arena(pads, loadout)
	local map = { w = 6000, h = 1000, loadout = loadout,
		platforms = { { id = "g", kind = "ground", x = 0, y = 600, w = 6000, h = 400 } },
		spawns = { { team = "player", x = 100, y = 581 }, { team = "bot", x = 3000, y = 581 } },
		weapon_spawns = pads }
	local w = World.new(map)
	w:start(); w:advance(0)
	return w, w.player, w.bots[1]
end
local function run(w, seconds)
	for _ = 1, math.floor(seconds / DT + 0.5) do w:step(DT) end
end
local function loose_of(w, gun)
	local n = 0
	for _, l in ipairs(w.loose) do if not gun or l.gun == gun then n = n + 1 end end
	return n
end
local function drops(w)
	local n = 0
	for _, l in ipairs(w.loose) do if not l.pad then n = n + 1 end end
	return n
end
local PAD = { id = "w1", x = 140, y = 586, pool = { "shotgun" }, respawn = 10 }

-- 1. Loadout: the map's, else rifle + pistol; two carried guns, each with a full mag.
local w, p, b = arena({})
check(p.gun == "rifle" and p.other == "pistol" and b.gun == "rifle" and b.other == "pistol", "default loadout is rifle + pistol")
local ammo, mag = w:ammo(p)
check(ammo == 30 and mag == 30, "the first gun starts full")
w = arena({}, { "smg", "shotgun" })
check(w.player.gun == "smg" and w.player.other == "shotgun" and w.bots[1].gun == "smg", "the map's loadout wins")
w = arena({}, { "sniper" })
check(w.player.gun == "sniper" and w.player.other == nil, "a one-gun loadout carries one gun")
check(not Spawns.swap(w, w.player), "swap with one gun does nothing")

-- 2. Spawns appear at match start; a taken gun respawns after its time (the whole cycle on the clock).
w, p = arena({ PAD })
check(loose_of(w, "shotgun") == 1 and #w.pads == 1, "the pad's gun is there at the start")
local l = w.loose[1]
check(l.x == 140 and l.pad == 1 and l.ammo == nil, "it lies on the pad, fresh")
run(w, 1)
check(Spawns.nearby(w, p) == l, "standing at the gun, it is the one in reach")
check(Spawns.pickup(w, p), "pick up")
check(p.gun == "shotgun" and p.other == "pistol", "the picked gun replaces the current gun, the second slot stays")
local a2, m2 = w:ammo(p)
check(a2 == 6 and m2 == 6, "a gun from a spawn comes with a full mag")
check(loose_of(w, "shotgun") == 0, "the pad is empty")
check(loose_of(w, "rifle") == 1 and drops(w) == 1, "the old gun dropped where the new one lay")
check(w.loose[1].x == 140, "...at that spot")
run(w, 9.5)
check(loose_of(w, "shotgun") == 0, "not respawned yet at 9.5 s")
run(w, 1)
check(loose_of(w, "shotgun") == 1, "respawned after its 10 s")
check(w.loose[#w.loose].pad == 1, "back on its pad")

-- A gun hung low or high by the map's author (snapped coordinates) stays where it hangs when swapped; a
-- falling drop that starts a few units too low still lands on the surface.
w, p = arena({ { id = "w1", x = 140, y = 596, pool = { "shotgun" }, respawn = 10 } })
Spawns.pickup(w, p)
run(w, 1)
check(w.loose[1].gun == "rifle" and w.loose[1].y == 596, "the swapped-out gun stays at the pad's height: " .. w.loose[1].y)
Spawns.drop(w, "smg", nil, 300, 592)
run(w, 1)
check(w.loose[2].y == 600 - Spawns.lie, "a drop a few units low still lands on the floor: " .. w.loose[2].y)

-- 3. Pick up needs reach; the nearest gun in reach is the one taken.
w, p = arena({ PAD })
p.x = 140 + Spawns.reach.x + 20
check(Spawns.nearby(w, p) == nil and not Spawns.pickup(w, p) and p.gun == "rifle", "out of reach: nothing")
p.x, p.y = 140, 586 - Spawns.reach.y - 20
check(Spawns.nearby(w, p) == nil, "far above: nothing")

-- 4. Swap keeps per-gun ammo and takes time; no firing while it lasts.
w, p = arena({})
p.ammo = 27
check(Spawns.swap(w, p) and p.gun == "pistol" and p.other == "rifle", "swap toggles")
check(select(1, w:ammo(p)) == 12, "the other gun has its own full mag")
p.ammo = 5
check(not Spawns.swap(w, p), "no second swap inside the swap time")
check(not Weapons.pull(w, p, "player", true), "no firing while swapping")
run(w, 0.15)
check(p.swap_left and p.swap_left > 0, "still swapping at 0.15 s")
run(w, 0.2)
check(p.swap_left == 0, "swap done after 0.3 s")
p.trigger_prev = false
check(Weapons.pull(w, p, "player", true), "firing again once it is done")
check(p.ammo == 4, "the pistol had 5, one shot left 4")
run(w, 0.4)
check(Spawns.swap(w, p) and p.gun == "rifle" and p.ammo == 27, "swap back: the rifle kept its 27")
run(w, 0.4)
Spawns.swap(w, p)
check(p.gun == "pistol" and p.ammo == 4, "and the pistol its 4")
-- swapping cancels a reload in progress
run(w, 0.4)
Weapons.reload(w, p)
check(p.reload_left, "reloading")
Spawns.swap(w, p)
check(p.reload_left == nil and p.gun == "rifle", "swap cancels the reload")

-- 5. The input path: the world's swap/pickup flags are consumed by one step.
w, p = arena({ PAD })
run(w, 1)
w.input.swap = true
w:step(DT)
check(p.gun == "pistol" and w.input.swap == false, "input.swap swaps once")
run(w, 0.4)
w.input.pickup = true
w:step(DT)
check(p.gun == "shotgun" and w.input.pickup == false, "input.pickup picks up once")
check(p.other == "rifle", "the current gun (pistol) dropped, the rifle stayed in hand")
check(loose_of(w, "pistol") == 1, "the pistol lies at the spot")

-- 6. One hand free: picking up fills it instead of dropping.
w, p = arena({ PAD }, { "rifle" })
run(w, 1)
Spawns.pickup(w, p)
check(p.gun == "shotgun" and p.other == "rifle" and drops(w) == 0, "an empty second slot takes the old gun, nothing drops")

-- 7. Death drops: the current gun with its remaining ammo, despawning after S.drop_ttl; a respawn re-arms.
w, p, b = arena({})
b.ammo = 7
b.x, b.y = 3000, 300 -- in the air: the drop falls to the floor
w:damage(b, 1000)
check(b.hp == 0, "bot dead")
run(w, 0.05)
check(drops(w) == 1 and w.loose[1].gun == "rifle" and w.loose[1].ammo == 7, "the bot's rifle dropped with its 7 rounds")
check(w.loose[1].ttl <= Spawns.drop_ttl and w.loose[1].ttl > Spawns.drop_ttl - 0.2, "it has a lifetime")
run(w, 1.5)
local lg = w.loose[1]
check(lg.y == 600 - Spawns.lie and lg.rest, "it fell and lies on the floor: " .. lg.y)
-- the bot is back at 2.2 s with a fresh loadout
run(w, 1)
check(b.hp > 0 and b.gun == "rifle" and b.other == "pistol", "respawned with the loadout")
check(select(1, w:ammo(b)) == 30, "...and a full mag")
check(drops(w) == 1, "only one drop (it did not drop again)")
p.x = lg.x
p.y = lg.y - 5
p.shield = 1e9 -- the bot shoots at him; a death would drop his gun too
Spawns.pickup(w, p)
check(p.gun == "rifle" and p.ammo == 7, "a dropped gun is picked up with the ammo it had")
check(drops(w) == 1 and w.loose[1].gun == "rifle", "and the player's own rifle dropped in its place")
run(w, Spawns.drop_ttl - 0.5)
check(drops(w) == 1, "still lying just before its time")
run(w, 1)
check(drops(w) == 0, "despawned after " .. Spawns.drop_ttl .. " s")

-- 8. The loose-gun cap: the oldest drop goes first; pad guns never count against it.
w, p = arena({ PAD })
for i = 1, Spawns.max_loose + 5 do Spawns.drop(w, "pistol", i, 400 + i, 586) end
check(drops(w) == Spawns.max_loose, "capped at " .. Spawns.max_loose)
check(loose_of(w, "shotgun") == 1, "the pad's gun is untouched")
local oldest = math.huge
for _, g in ipairs(w.loose) do if not g.pad then oldest = math.min(oldest, g.ammo) end end
check(oldest == 6, "the five oldest went (ammo tags 1..5): " .. oldest)

-- 9. Weighted pools: weight = how many times an id is listed; deterministic under the world RNG.
local pool = { "rifle", "rifle", "rifle", "smg" }
local function rolls(n)
	local world = arena({})
	local out, rifles = {}, 0
	for i = 1, n do
		out[i] = Spawns.roll(world, pool)
		if out[i] == "rifle" then rifles = rifles + 1 end
	end
	return out, rifles
end
local r1, rifles = rolls(400)
local r2 = rolls(400)
local same = true
for i = 1, 400 do same = same and r1[i] == r2[i] end
check(same, "the same world seed rolls the same guns")
check(rifles > 400 * 0.65 and rifles < 400 * 0.85, "weights hold: " .. rifles .. " of 400 were rifles")
check(Spawns.roll(arena({}), { "sniper" }) == "sniper", "a pool of one needs no roll")
check(Spawns.roll(arena({}), { "plasma", "smg" }) == "smg", "unknown ids are skipped")
local pooled = { id = "w1", x = 140, y = 586, pool = { "smg", "smg", "shotgun", "pistol" }, respawn = 2 }
local function seq()
	local world, player = arena({ pooled })
	local out = { world.loose[1].gun }
	for _ = 1, 8 do
		player.x, player.y = 140, 581
		Spawns.pickup(world, player)
		run(world, 2.1)
		for _, g in ipairs(world.loose) do if g.pad then table.insert(out, g.gun) end end
	end
	return table.concat(out, ",")
end
check(seq() == seq(), "a pooled pad's sequence is deterministic: " .. seq())

-- 10. Bots take a better gun when they are near one, and not a worse one.
w, p, b = arena({})
b.gun, b.other = "pistol", nil
b.x, b.y = 500, 581
Spawns.drop(w, "rifle", nil, 510, 586)
w:step(DT)
check(b.gun == "rifle" and b.other == "pistol", "a pistol bot picked up the rifle (the pistol went to the free slot)")
check(drops(w) == 0, "the rifle is off the floor")
w, p, b = arena({})
b.x, b.y = 500, 581
Spawns.drop(w, "pistol", nil, 510, 586)
Spawns.drop(w, "sniper", nil, 520, 586)
Spawns.drop(w, "rocket", nil, 505, 586)
run(w, 0.5)
check(b.gun == "rifle" and drops(w) == 3, "a rifle bot ignores a pistol, a sniper and a rocket launcher")
w, p, b = arena({})
b.gun, b.other = "pistol", nil
b.x, b.y = 700, 581
p.x = 700
Spawns.drop(w, "smg", nil, 860, 586)
run(w, 0.3)
check(b.x > 700, "the bot heads for the smg")
run(w, 3)
check(b.gun == "smg", "and takes it")
w, p, b = arena({})
b.gun, b.other = "pistol", nil
b.x, b.y = 700, 581
Spawns.drop(w, "smg", nil, 1500, 586)
check(Bots.seek(w, b) == nil, "too far to seek")

-- 11. Dead soldiers do not pick up or swap.
w, p, b = arena({ PAD })
p.hp = 0
check(not Spawns.pickup(w, p) and not Spawns.swap(w, p), "the dead take nothing")

print("jetpack pickups ok: loadout, spawn/take/respawn, swap ammo, death drops, cap, weighted pools, bots")
