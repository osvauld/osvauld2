local M = {}
local function copy(v) return { alice = v.alice, bob = v.bob } end

local function event(m, message)
	m.event = message
end

function M.total(m, who)
	if m.naive then return m.scalar[who] end
	local v = m.replicas[who]
	return v.alice + v.bob
end

local function increment(m, who)
	if m.naive then
		m.scalar[who] = m.scalar[who] + 1
	else
		m.replicas[who][who] = m.replicas[who][who] + 1
	end
	m.clicks = m.clicks + 1
	event(m, who == "alice" and "Alice increments her own contribution." or "Bob increments his own contribution.")
end

local function send(m, who)
	if m.naive then return end
	local target = who == "alice" and "bob" or "alice"
	m.sequence = m.sequence + 1
	m.queue[#m.queue + 1] = { id = m.sequence, from = who, to = target, state = copy(m.replicas[who]) }
	event(m, "Queued snapshot " .. m.sequence .. "; later sender edits do not change this message.")
end

local function merge(m, packet)
	local target = m.replicas[packet.to]
	for _, who in ipairs({ "alice", "bob" }) do target[who] = math.max(target[who], packet.state[who]) end
	m.last = { from = packet.from, to = packet.to, state = copy(packet.state), id = packet.id }
	m.deliveries = m.deliveries + 1
	event(m, "Delivered snapshot " .. packet.id .. " to " .. packet.to .. ": keep the maximum in each slot.")
end

local function deliver(m, last)
	if not m.connected then event(m, "Disconnected: the snapshot stays queued.") return end
	if #m.queue == 0 then event(m, "No queued snapshots. Queue an update first.") return end
	merge(m, table.remove(m.queue, last and #m.queue or 1))
end

M.actions = {
	["inc-alice"] = function(m) increment(m, "alice") end,
	["inc-bob"] = function(m) increment(m, "bob") end,
	["send-alice"] = function(m) send(m, "alice") end,
	["send-bob"] = function(m) send(m, "bob") end,
	["deliver-first"] = function(m) deliver(m, false) end,
	["deliver-last"] = function(m) deliver(m, true) end,
	network = function(m) m.connected = not m.connected event(m, m.connected and "Connected. Deliver queued snapshots when ready." or "Disconnected. Both replicas can keep incrementing.") end,
	redeliver = function(m)
		if not m.connected then event(m, "Reconnect before delivering a snapshot.") return end
		if not m.last then event(m, "Deliver one snapshot before repeating it.") return end
		merge(m, m.last)
	end,
	["copy-total"] = function(m)
		if not m.naive then return end
		m.scalar.bob = m.scalar.alice
		event(m, "Copied Alice's scalar to Bob. Both say one, although two clicks happened.")
	end,
}

function M.prepare(scene)
	local m = {
		replicas = { alice = { alice = 0, bob = 0 }, bob = { alice = 0, bob = 0 } },
		scalar = { alice = 0, bob = 0 }, queue = {}, connected = false,
		clicks = 0, sequence = 0, deliveries = 0, event = "Try a local increment on either replica.",
	}
	if scene == "naive" then m.naive = true end
	if scene ~= "hook" then increment(m, "alice") increment(m, "bob") end
	if scene == "merge" or scene == "duplicate" or scene == "order" then m.connected = true end
	if scene == "duplicate" then
		send(m, "alice") send(m, "bob") deliver(m, false) deliver(m, false)
	elseif scene == "order" then
		send(m, "alice") increment(m, "alice") send(m, "alice") send(m, "bob")
	end
	event(m, scene == "order" and "Three snapshots, including an older Alice state. Choose a delivery order." or "Scene prepared. Try the controls or play the demonstration.")
	return m
end

return M
