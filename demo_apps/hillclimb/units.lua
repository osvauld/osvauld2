-- SI to the world's units and back. The world is 100 units to the metre and masses are kg, so
-- springs (kg/s² = N/m) and dampers (kg/s = N·s/m) pass through; lengths scale by 100 and
-- torques (kg·m²/s²) by 100². The only place the scale lives.
local U = {}

local PER_METRE = 100

U.gravity = 9.81 * PER_METRE

function U.length(m) return m * PER_METRE end
function U.metres(u) return u / PER_METRE end
function U.point(p) return { p[1] * PER_METRE, p[2] * PER_METRE } end
function U.mass(kg) return kg end
function U.stiffness(n_per_m) return n_per_m end
function U.damping(ns_per_m) return ns_per_m end
function U.torque(nm) return nm * PER_METRE * PER_METRE end
function U.newton_metres(t) return t / (PER_METRE * PER_METRE) end

-- A wheel of `radius_m` rolling at `kmh`, as a motor speed: degrees a second.
function U.wheel_speed(kmh, radius_m)
	return math.deg((kmh / 3.6) / radius_m)
end

-- World units a second as km/h.
function U.kmh(u_per_s) return u_per_s / PER_METRE * 3.6 end

return U
