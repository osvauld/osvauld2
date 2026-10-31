-- Pure stick maths (no ui): how far a thumb offset (dx, dy in px) moves, flies and aims. `ring` is the
-- stick's travel radius in px. Also where the buttons (swap, grenade, melee, reload, zoom) sit, and the zoom
-- button's pure state (view-state only: the level a gun is scoped at; the camera does the rest).
local S = {}

S.deadzone = 0.12   -- of the ring, before the stick means anything
S.hover = 0.5       -- stick height that holds a hover: S.thrust(hover) ~ gravity / thrust (0.64)
S.floor = 0.5       -- the throttle just past the deadzone; below ~0.64 the pack never lifts, so travel is not wasted there
S.curve = 1.5       -- throttle = floor + (1 - floor) * t^curve, t = stick height past the deadzone (0..1)
S.walk_full = 0.8   -- of the ring, a full walk
S.aim_at = 0.2      -- of the ring, the aim stick starts steering
S.fire_at = 0.6     -- of the ring, the aim stick fires (the drawn ring)

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- Stick size (px) for a playfield whose short side is `short`, and its travel radius.
function S.size(short) return clamp(math.floor(short * 0.3 + 0.5), 110, 150) end
function S.ring(size) return size * 0.42 end

-- Horizontal move -1..1 from the left stick.
function S.walk(dx, ring)
	local v = dx / (ring * S.walk_full)
	if math.abs(v) < S.deadzone then return 0 end
	return clamp((v - (v > 0 and S.deadzone or -S.deadzone)) / (1 - S.deadzone), -1, 1)
end

-- Jetpack throttle from the left stick's push up (dy < 0): a number in (0, 1], or false when idle.
function S.thrust(dy, ring)
	local h = clamp(-dy / ring, 0, 1)
	if h <= S.deadzone then return false end
	return S.floor + (1 - S.floor) * ((h - S.deadzone) / (1 - S.deadzone)) ^ S.curve
end

-- The right stick: the aim angle (nil while it rests) and whether it is pushed past the fire ring.
function S.aim(dx, dy, ring)
	local length = math.sqrt(dx * dx + dy * dy) / ring
	if length < S.aim_at then return nil, false end
	return (math.atan2 or math.atan)(dy, dx), length >= S.fire_at
end

-- Button slots in a column on the playfield's right edge, above the aim stick (44 px, down to 36 on a short playfield); `slot_names` is their
-- order top to bottom. `swap` (key Q), `reload` (R) and `zoom` (Z) are drawn and wired (controls.lua); `pickup`
-- sits left of `swap` and only shows over a gun on the floor. The camera keeps the soldier off all of
-- them and off the sticks.
S.slot_names = { "swap", "grenade", "melee", "reload", "zoom" }
function S.slots(w, h)
	local top, gap, margin = 10, 6, 10
	-- The column ends above the aim stick; a short (landscape phone) playfield shrinks the buttons to fit.
	local stick_top = h - margin - S.size(math.min(w, h))
	local size = clamp(math.floor((stick_top - gap - top - gap * (#S.slot_names - 1)) / #S.slot_names), 36, 44)
	local out = {}
	for i, name in ipairs(S.slot_names) do
		out[name] = { x = w - margin - size, y = top + (i - 1) * (size + gap), w = size, h = size }
	end
	out.pickup = { x = out.swap.x - gap - size, y = out.swap.y, w = size, h = size }
	return out
end

-- Zoom ----------------------------------------------------------------------------------------

-- A gun's zoom levels (stats.zooms: ascending, first 1); { 1 } for none.
local no_zoom = { 1 }
function S.zoom_levels(gun) return gun and gun.stats and gun.stats.zooms or no_zoom end

-- The next level index. A tap (dir nil) wraps from the last back to 1; the wheel (dir +1 wider, -1 closer) stops at the ends.
function S.zoom_step(levels, i, dir)
	if dir then return clamp(i + dir, 1, #levels) end
	return i % #levels + 1
end

-- "1x", "1.25x", "2x".
function S.zoom_label(level) return string.format("%gx", level) end

function S.zoom_new() return { i = 1, gun = nil, swap = 0 } end

-- Follow the player's equipment: any gun change, a fresh equip (the swap clock jumps up, which a swap and a
-- pick up both do, even for the same gun id) or a death puts the level back to 1.
function S.zoom_track(z, gun_id, swap_left, alive)
	if not alive or gun_id ~= z.gun or (swap_left or 0) > z.swap then z.i = 1 end
	z.gun, z.swap = gun_id, swap_left or 0
end

-- Mouse aim (desktop). The angle runs from the soldier's SCREEN position to the cursor, so the camera moving
-- under a resting cursor re-aims only through the soldier's small drift on screen. Inside a dead radius (one soldier
-- tall) the aim holds, so a cursor passing over him does not spin the gun. The angle is smoothed (S.mouse_tau seconds
-- exponential, 90 % of a step in ~0.09 s): direct, but a one-pixel jitter stops being a visible shake. Sticks bypass this.
S.mouse_tau = 0.04
S.soldier_units = 52  -- a soldier's height in world units

function S.mouse_new() return {} end
function S.mouse_dead(scale) return S.soldier_units * scale end

-- One step: the smoothed aim angle (rad). `start` is the aim to begin from, and to keep while the cursor is in the dead radius first.
function S.mouse_step(m, dt, cx, cy, sx, sy, dead, start)
	local dx, dy = cx - sx, cy - sy
	if dx * dx + dy * dy >= dead * dead then m.target = math.atan2(dy, dx) end
	if m.angle == nil then m.angle = m.target or start or 0 end
	if m.target then
		local diff = (m.target - m.angle + math.pi) % (2 * math.pi) - math.pi
		m.angle = m.angle + diff * (1 - math.exp(-math.max(0, dt) / S.mouse_tau))
	end
	return m.angle
end

return S
