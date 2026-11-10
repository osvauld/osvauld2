local C = require("theme").mission
local Level = require("mission_world")
local Controls = require("rover_controls")
local V = require("vehicle")
local results = doc:open("crater-mission")
if not results.stats then results:set({"stats"},doc.map({completed_runs=0})) end
local M = {
	world=Level.world,level=Level,controls=Controls.new(),
	running=false,initialized=false,cargo=false,finished=false,recoveries=0,status="Ready",
}
local drive_keys = {KeyW=true,KeyA=true,KeyS=true,KeyD=true,Space=true}

function M.pose()
	return M.world:get("rover") or {position=Level.spawn,rotation=Level.rotation,velocity={0,0,0}}
end

function M.speed(pose)
	local v = pose.velocity
	return math.sqrt(v[1]^2+v[2]^2+v[3]^2)
end

function M.target()
	return (M.cargo or M.finished) and Level.home or Level.site
end

function M.distance(pose)
	local at = M.target()
	return math.sqrt((pose.position[1]-at[1])^2+(pose.position[3]-at[3])^2)
end

function M.history()
	return results.stats and results.stats.completed_runs or 0
end

function M.send()
	V.apply(M.world,M.running and M.controls:input() or {brake=1})
end

function M.reset()
	M.world:reset("rover")
	M.controls:clear()
	M.cargo,M.finished,M.running,M.initialized = false,false,false,true
	M.recoveries,M.status = 0,"Ready"
	M.send()
end

function M.enter()
	if not M.initialized then M.reset() end
end

function M.drive()
	if not M.initialized or M.finished then M.reset() end
	M.running,M.status = true,"Exploring"
	M.send()
end

function M.pause()
	M.running = false
	M.controls:clear()
	if not M.finished then M.status="Paused" end
	M.send()
end

function M.interact()
	if M.finished then return end
	if not M.running then M.status="Press Drive to begin"; return end
	local pose,at = M.pose(),M.target()
	local q = pose.rotation
	local upright = 1-2*(q[1]^2+q[3]^2)
	if M.distance(pose)>C.collect_range or math.abs(pose.position[2]-at[2])>0.5 then
		M.status = M.cargo and "Return to the landing pad" or "Reach the crater beacon"
		return
	end
	if M.speed(pose)>C.collect_speed or upright<0.6 then M.status="Stop upright before interacting"; return end
	if not M.cargo then
		M.cargo,M.status = true,"Sample secured — return to lander"
	else
		local completed = results.stats.completed_runs or 0
		results:set({"stats","completed_runs"},completed+1)
		M.cargo,M.finished,M.running,M.status = false,true,false,"Mission complete"
		M.controls:clear()
		M.send()
	end
end

function M.zone(e)
	if e.who~="rover" or e.phase~="enter" or e.id~="fall-zone" then return end
	M.world:reset("rover")
	M.controls:clear()
	M.cargo,M.finished,M.running = false,false,false
	M.recoveries = M.recoveries+1
	M.status = "Recovered at lander — press Drive"
	M.send()
end

function M.key(e)
	if e.cancelled then M.pause(); return true end
	if drive_keys[e.code] then M.controls:key(e); M.send(); return true end
	if e.code=="KeyE" then
		if e.down and not e.repeated then M.interact() end
		return true
	end
	if e.down and not e.repeated then
		if e.code=="KeyR" then M.reset(); return true end
		if e.code=="KeyP" then
			if M.running then M.pause() else M.drive() end
			return true
		end
	end
	return false
end

function M.pedal(name,e)
	M.controls:hover(name,e)
	M.send()
end

return M
