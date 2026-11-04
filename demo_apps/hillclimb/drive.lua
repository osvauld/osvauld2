-- Pedals to motors: `{ gas = 0..1, brake = 0..1 }` and the run's `{ fuel = L }` in, a motor per
-- axle id out (or none: the wheel rolls free). No world, no input devices.
--
-- Gas drives the driven wheels toward top speed at up to `drive.torque` × pedal, while there is
-- fuel. The brake drives every wheel toward `brake.reverse_speed` backwards at up to
-- `brake.torque` × pedal: rolling forward that is braking, at rest it backs up. The brake wins
-- when both are held and works with an empty tank.
local U = require("units")
local D = {}

function D.motors(car, pedals, run)
	local out = {}
	local gas, brake = pedals.gas or 0, pedals.brake or 0
	for i, w in ipairs(car.wheels) do
		local id = "axle" .. i
		if brake > 0 then
			out[id] = {
				speed = -U.wheel_speed(car.brake.reverse_speed, w.radius),
				torque = U.torque(car.brake.torque) * brake,
			}
		elseif gas > 0 and w.driven and run.fuel > 0 then
			out[id] = {
				speed = U.wheel_speed(car.drive.top_speed, w.radius),
				torque = U.torque(car.drive.torque) * gas,
			}
		end
	end
	return out
end

return D
