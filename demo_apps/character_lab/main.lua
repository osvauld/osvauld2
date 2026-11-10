-- Step 4's lab (docs/design/2d-characters.md): converted characters side by side, one clip name
-- for all. Paused, each is a plain frame posed at exactly `t` (what S-C4 compares); playing, a
-- world plays the clips in Rust. A converted rig's rest pose is not a pose, so a clip always is.
local C = require("theme")
local cast = require("cast")

-- Survives hot reload, so an edited clip file comes back on the same clip and time. Read in
-- view: a reload carries ui.state over after the new module has loaded, so a table taken at
-- module scope would be a fresh one.
local function state()
	return ui.state("lab", { clip = "run", t = 0, playing = false })
end
local S = state()
local run = { started = nil, now = 0 }   -- the play clock, from on_frame
local hot, hot_in = nil, nil
local posed, posed_at = {}, nil

local function current(c)
	return cast.clip(c, S.clip)
end

local function repose()
	for _, c in ipairs(cast.characters) do
		posed[c.id] = c.drawing:pose(current(c):at(S.t))
	end
	posed_at = S.clip .. "@" .. S.t
end

local function length()
	return current(cast.characters[1]).length
end

-- What the world shows `now` seconds after it started the clip.
local function playing_time()
	local len = length()
	if current(cast.characters[1]).loop then
		return run.now % len
	end
	return math.min(run.now, len)
end

local function set_time(t)
	S.t = math.max(0, t)
	repose()
end

local function pick(name)
	S.clip = name
	S.draft = nil
	run.started = nil
	run.now = 0
	set_time(S.playing and 0 or math.min(S.t, length()))
end

local function toggle()
	if S.playing then
		S.playing = false
		set_time(playing_time())
	else
		S.playing = true
		run.started = nil
		run.now = 0
	end
	S.draft = nil
end

local function button(id, label, on, f)
	return ui.button({
		id = id, h = 26, px = 10, radius = 6, center = true,
		fill = on and C.on or C.card, hover_fill = on and C.on or C.hot,
		ui.text({ label, color = C.text, font_size = 12, no_wrap = true }),
		on_click = f,
	})
end

local function picker()
	local row = {}
	for _, name in ipairs(cast.names) do
		table.insert(row, button("clip:" .. name, name, name == S.clip, function() pick(name) end))
	end
	return ui.row({ wrap = true, gap = 6, row })
end

-- A narrow slot's frame keeps its whole box (the column would stretch it to the slot, and only a
-- point inside the box is picked), so it overlaps its neighbours' boxes, and a frame with no part
-- under the pointer clears the readout only if it set it.
local function slot(c)
	local s = C.slots[c.slot]
	return ui.col({
		w = s.w, h = C.slot_h, px = C.pad_x, py = C.pad_y,
		ui.frame({
			id = "char:" .. c.id, visual = posed[c.id], offset = { s.shift, 0 },
			w = s.shift ~= 0 and posed[c.id].width or nil,
			on_hover = function(e)
				if e.phase ~= "leave" and e.shape then
					hot, hot_in = e.shape, c.id
				elseif hot_in == c.id then
					hot, hot_in = nil, nil
				end
			end,
		}),
	})
end

local function stage()
	if S.playing then
		local w = { id = "world", height = C.slot_h, fill = C.stage }
		local x = 0
		for _, c in ipairs(cast.characters) do
			local s = C.slots[c.slot]
			table.insert(w, {
				id = c.id, pos = { x + C.pad_x + s.shift, C.pad_y },
				drawing = c.drawing, clip = current(c),
			})
			x = x + s.w
		end
		w.width = x
		return ui.world(w)
	end
	local slots = {}
	for _, c in ipairs(cast.characters) do
		table.insert(slots, slot(c))
	end
	return ui.row({ id = "stage", clip = true, fill = C.stage, slots })
end

return function()
	S = state()
	if posed_at ~= S.clip .. "@" .. S.t then repose() end   -- first view, and after a reload
	local t = S.playing and playing_time() or S.t
	return ui.col({
		id = "lab", full = true, pad = 20, gap = 12, fill = C.bg,
		on_frame = S.playing and function(e)
			run.started = run.started or e.elapsed
			run.now = e.elapsed - run.started
		end or nil,
		picker(),
		ui.row({
			gap = 8, align_center = true,
			button("play", S.playing and "Pause" or "Play", S.playing, toggle),
			button("prev", "−1/60", false, function() S.draft = nil; set_time(S.t - 1 / 60) end),
			button("next", "+1/60", false, function() S.draft = nil; set_time(S.t + 1 / 60) end),
			ui.input({
				id = "time", w = 90, h = 26, px = 6, radius = 6, fill = C.card, color = C.text,
				font_size = 12,
				value = S.draft or string.format("%.4f", S.t),
				on_input = function(v)
					S.draft = v
					local n = tonumber(v)
					if n and not S.playing then set_time(n) end
				end,
			}),
			ui.text({ string.format("time: %.4f / %.4f s", t, length()), color = C.muted,
				font_size = 12, no_wrap = true }),
			ui.text({ "clip: " .. S.clip, color = C.muted, font_size = 12, no_wrap = true }),
			ui.text({ "part: " .. (hot or "—"), color = C.muted, font_size = 12, no_wrap = true }),
		}),
		stage(),
	})
end
