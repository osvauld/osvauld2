-- mp_dots: the smallest `net.match` (jetpack-mp.md M4, M5). Each player moves one dot; the
-- platform exchanges inputs, rewinds on a late one, and hashes the state every second. "script"
-- makes each player's input a function of the frame, so a test drives every peer the same way.

local SPEED = 2
local keys = ui.state("keys", { left = false, right = false, up = false, down = false })
local mode = ui.state("mode", { scripted = false, cheat = false })
local match

local function held(k)
	return keys[k] and 1 or 0
end

local function start()
	match = net.match({
		id = "dots",
		tick = 1 / 60,
		frame = 2,
		players = net.peers(),
		setup = { size = 400 },
		state = function(setup, players)
			local s = { size = setup.size, t = 0, dots = {} }
			for i = 1, #players do
				s.dots[i] = { x = 40 + 60 * i, y = 200 }
			end
			return s
		end,
		input = function()
			if mode.scripted then
				local f, me = match.frame, match.me
				return { dx = ((f // 15 + me) % 3) - 1, dy = ((f // 20 + 2 * me) % 3) - 1 }
			end
			return { dx = held("right") - held("left"), dy = held("down") - held("up") }
		end,
		step = function(s, inputs)
			s.t += 1
			for i, d in ipairs(s.dots) do
				local input = inputs[i]
				if input then
					-- "cheat" (a test seam, jetpack-mp.md M9): this device plays by other rules.
					local speed = mode.cheat and SPEED * 2 or SPEED
					d.x = math.clamp(d.x + input.dx * speed, 0, s.size)
					d.y = math.clamp(d.y + input.dy * speed, 0, s.size)
				end
			end
		end,
	})
end

local ARROWS = { ArrowLeft = "left", ArrowRight = "right", ArrowUp = "up", ArrowDown = "down" }

return function()
	local rows = {
		ui.row({
			gap = 8,
			ui.button({ "start", id = "start", on_click = start }),
			ui.button({ mode.scripted and "scripted" or "keys", id = "script", on_click = function()
				mode.scripted = not mode.scripted
			end }),
			ui.button({ mode.cheat and "cheating" or "fair", id = "cheat", on_click = function()
				mode.cheat = not mode.cheat
			end }),
		}),
	}
	if match then
		rows[#rows + 1] = ui.text({ "status " .. match.status .. " frame " .. match.frame, id = "status", no_wrap = true })
		if match.ended then
			rows[#rows + 1] = ui.text({ "ended " .. match.ended, id = "ended", no_wrap = true })
		end
		if #match.waiting > 0 then
			rows[#rows + 1] = ui.text({ "waiting for " .. table.concat(match.waiting, ","), id = "waiting", no_wrap = true })
		end
		local checks = {}
		for _, c in ipairs(match.checks) do
			checks[#checks + 1] = c.frame .. "=" .. c.hash
		end
		rows[#rows + 1] = ui.text({ "checks " .. table.concat(checks, " "), id = "checks" })
		rows[#rows + 1] = ui.text({ "desynced " .. table.concat(match.desynced, ","), id = "desynced", no_wrap = true })
		local s = match.state
		if s then
			for i, d in ipairs(s.dots) do
				local mark = i == match.me and " (me)" or ""
				rows[#rows + 1] = ui.text({ string.format("dot %d: %d, %d%s", i, d.x, d.y, mark), id = "dot:" .. i, no_wrap = true })
			end
		end
	end
	return ui.col({
		id = "dots",
		pad = 16,
		gap = 8,
		on_key = function(e)
			local k = ARROWS[e.code or ""]
			if k then
				keys[k] = e.down
			end
		end,
		table.unpack(rows),
	})
end
