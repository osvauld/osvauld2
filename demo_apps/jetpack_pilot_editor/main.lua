local C = require("theme")
local F = require("format")
local M = require("model")
local P = require("pilot_art")

-- The preview soldier holds the workspace's rifle (the baked one if the gun editor never ran).
local preview_gun = F.find((F.load_guns(asset:open("jetpack-guns"), require("baked/guns"))), "rifle")

-- Viewer state: the tool, the part new shapes go to, the colours shown, the selection and a
-- drag in flight. The pilots are the asset's.
local S = { tool = "select", part = "torso", team = "pilot", selected = nil, drag = nil,
	motion = "walk", playing = true, t = 0 }
local drafts = {} -- field text not yet committed; cleared when another pilot shows
local brushes = {}
local z = C.zoom

local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end
local function scaled(path)
	local out = {}
	for i, c in ipairs(path) do
		local m = { c[1] }
		for k = 2, #c do m[k] = c[k] * z end
		out[i] = m
	end
	return gfx.path(out)
end
local function box(x, y, w, h)
	return gfx.path({ { "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } })
end
local function dot(x, y, r)
	return gfx.path(M.makers.ellipse(x - r, y - r, 2 * r, 2 * r))
end

-- The colour a fill paints in this view: team slots follow the team shown.
local function colour(pilot, fill)
	local name = type(fill) == "string" and fill:match("^@(.+)$")
	if not name then return type(fill) == "string" and fill or C.bad end
	local team = P.teams[S.team]
	return (team and team[name]) or (pilot.vars and pilot.vars[name]) or C.bad
end

-- A shape's path this frame: the drag's live one, else the asset's.
local function live_path(pilot, s)
	local d = S.drag
	if not (d and d.id == s.id) then return s.path end
	if d.mode == "shape" then return M.moved(pilot, d.from, d.dx, d.dy) end
	if d.mode == "grip" then return M.scaled(pilot, d.from, d.dx, d.dy) end
	if d.mode == "pt" then return M.with_point(pilot, d.from, d.j, d.k, d.x + d.dx, d.y + d.dy) end
	return s.path
end
local function live_xy(d, name, x, y)
	if d and d.id == name then return x + d.dx, y + d.dy end
	return x, y
end

-- Enough structure to draw from: size, the points and each part's pivot. Shapes are checked one
-- by one as they draw, so one broken shape hides itself and the status names it.
local function pair(v) return type(v) == "table" and type(v[1]) == "number" and type(v[2]) == "number" end
local function drawable(pilot)
	if not (pair(pilot.size) and type(pilot.points) == "table" and type(pilot.parts) == "table") then return false end
	for _, name in ipairs(F.points) do
		if not pair(pilot.points[name]) then return false end
	end
	for _, part in ipairs(pilot.parts) do
		if type(part) ~= "table" or not pair(part.pivot) then return false end
	end
	return true
end

local function canvas(pilot)
	local w, h = pilot.size[1], pilot.size[2]
	local items = { gfx.fill({ path = box(0, 0, w * z, h * z), brush = brush(C.paper) }) }
	for x = 4, w - 1, 4 do
		table.insert(items, gfx.stroke({ path = gfx.path({ { "move", x * z, 0 }, { "line", x * z, h * z } }), brush = brush(C.grid), width = 1 }))
	end
	for y = 4, h - 1, 4 do
		table.insert(items, gfx.stroke({ path = gfx.path({ { "move", 0, y * z }, { "line", w * z, y * z } }), brush = brush(C.grid), width = 1 }))
	end
	local chosen = nil
	-- The part being edited draws on top, so an arm behind the body can still be seen and picked.
	local order = {}
	for _, part in ipairs(pilot.parts or {}) do
		if part.id ~= S.part then table.insert(order, part) end
	end
	for _, part in ipairs(pilot.parts or {}) do
		if part.id == S.part then table.insert(order, part) end
	end
	for _, part in ipairs(order) do
		for _, s in ipairs(type(part.shapes) == "table" and part.shapes or {}) do
			local path = type(s) == "table" and F.path(s.path, w, h, "") and live_path(pilot, s)
			if path then table.insert(items, gfx.fill({ id = "shape:" .. s.id, path = scaled(path), brush = brush(colour(pilot, s.fill)) })) end
			if path and s.id == S.selected then chosen = path end
		end
	end
	local d = S.drag
	-- The gun as the game holds it at aim 0, its handle at `hold`, pointing right.
	local gx, gy = live_xy(d, "point:hold", pilot.points.hold[1], pilot.points.hold[2])
	table.insert(items, gfx.stroke({ path = gfx.path({ { "move", (gx - 16) * z, gy * z }, { "line", (gx + 36) * z, gy * z } }),
		brush = brush("#182e3466"), width = 14 * z }))
	if chosen then
		local x, y, bw, bh = M.box(chosen)
		table.insert(items, gfx.stroke({ path = box(x * z, y * z, bw * z, bh * z), brush = brush(C.select), width = 2 }))
		for j, c in ipairs(chosen) do
			if c[1] == "move" or c[1] == "line" then
				table.insert(items, gfx.fill({ id = "pt:" .. S.selected .. ":" .. j .. ":2", path = dot(c[2] * z, c[3] * z, C.dot * z / 2),
					brush = brush(C.select) }))
			end
		end
		local g = C.grip * z
		table.insert(items, gfx.fill({ id = "grip:" .. S.selected, path = box((x + bw) * z - g / 2, (y + bh) * z - g / 2, g, g),
			brush = brush(C.select) }))
	end
	for _, part in ipairs(pilot.parts or {}) do
		if part.id == S.part then
			local px, py = live_xy(d, "pivot:" .. part.id, part.pivot[1], part.pivot[2])
			table.insert(items, gfx.fill({ id = "pivot:" .. part.id, path = dot(px * z, py * z, C.dot * z), brush = brush(C.pivot) }))
		end
	end
	for _, name in ipairs(F.points) do
		local p = pilot.points[name]
		local px, py = live_xy(d, "point:" .. name, p[1], p[2])
		table.insert(items, gfx.fill({ id = "point:" .. name, path = dot(px * z, py * z, C.dot * z), brush = brush(C.marker) }))
	end
	return gfx.frame({ width = w * z, height = h * z, table.unpack(items) })
end

local function shape_id(shape)
	if not shape then return nil end
	return string.match(shape, "^(%a+):(.+)$")
end

local function click(pilot, e)
	local x, y = e.x / z, e.y / z
	if M.makers[S.tool] then
		S.selected = M.add_shape(pilot, S.part, S.tool, x, y)
		return
	end
	local mode, id = shape_id(e.shape)
	if mode == "shape" then
		local _, part = M.find(pilot, id)
		S.selected, S.part = id, part and part.id or S.part
	elseif mode ~= "grip" and mode ~= "pt" then
		S.selected = nil
	end
end

local function drag(pilot, e)
	if e.phase == "start" then
		local mode, id = shape_id(e.shape)
		if not mode then return end
		local d = { mode = mode, dx = 0, dy = 0 }
		if mode == "shape" or mode == "grip" then
			local s, part = M.find(pilot, id)
			if not s then return end
			S.selected, S.part = id, part.id
			d.id, d.part, d.from = id, part.id, s.path
		elseif mode == "pt" then
			local sid, j, k = string.match(id, "^(.+):(%d+):(%d+)$")
			local s, part = M.find(pilot, sid)
			if not s then return end
			j, k = tonumber(j), tonumber(k)
			d.id, d.part, d.from, d.j, d.k, d.x, d.y = sid, part.id, s.path, j, k, s.path[j][k], s.path[j][k + 1]
		elseif mode == "point" or mode == "pivot" then
			d.id, d.name = e.shape, id
		else
			return
		end
		S.drag = d
		return
	end
	local d = S.drag
	if not d then return end
	d.dx, d.dy = e.dx / z, e.dy / z
	if e.phase == "end" then
		if d.mode == "point" then
			local p = pilot.points[d.name]
			M.set_point(pilot, d.name, p[1] + d.dx, p[2] + d.dy)
		elseif d.mode == "pivot" then
			for _, part in ipairs(pilot.parts) do
				if part.id == d.name then M.set_pivot(pilot, d.name, part.pivot[1] + d.dx, part.pivot[2] + d.dy) end
			end
		else
			local s = M.find(pilot, d.id)
			if s then M.set_path(d.part, d.id, live_path(pilot, s)) end
		end
		S.drag = nil
	end
end

local function text(value, id, color, size)
	return ui.text({ value, id = id, color = color or C.text, font_size = size or 12, no_wrap = true })
end
local function button(label, id, on, action)
	return ui.button({ id = id, h = 30, px = 9, radius = 6, center = true,
		fill = on and C.accent or C.panel, hover_fill = on and C.accent or C.edge, stroke = { 1, on and C.accent or C.edge },
		on_click = action, text(label, nil, on and C.bg or C.text, 12) })
end
local function choice(label, id, on, action)
	return button(label, id, on, action)
end

-- The pilot as the game draws it (pilot_art.lua, the game's own code), per team at 2×, in motion.
-- The checked pilot is a new table each frame; holding the last one while its content is the
-- same keeps pilot_art's compiled drawing.
local held = { key = nil, pilot = nil }
local function stable(pilot)
	local buf = { M.shown, pilot.size[1], pilot.size[2], pilot.scale }
	for _, part in ipairs(pilot.parts) do
		table.insert(buf, part.pivot[1] .. "," .. part.pivot[2])
		for _, s in ipairs(part.shapes) do
			table.insert(buf, s.id .. s.fill)
			for _, c in ipairs(s.path) do table.insert(buf, table.concat(c, " ")) end
		end
	end
	local names = {}
	for k in pairs(pilot.vars) do table.insert(names, k) end
	table.sort(names) -- pairs order is not stable
	for _, k in ipairs(names) do table.insert(buf, k .. pilot.vars[k]) end
	for _, name in ipairs(F.points) do table.insert(buf, table.concat(pilot.points[name], ",")) end
	local key = table.concat(buf, "|")
	if held.key ~= key then held.key, held.pilot = key, pilot end
	return held.pilot
end

-- What the preview pilot is doing at time `t`: walking turns round every two seconds, flying
-- bobs with the flame on, aiming sweeps the gun and fires.
local function acting(t)
	local mode = S.motion
	local turn = math.floor(t / 2) % 2 == 0 and 0 or math.pi
	if mode == "walk" then return { grounded = true, vx = 120, aim = turn } end
	if mode == "fly" then return { grounded = false, vx = 0, thrusting = true, aim = turn } end
	if mode == "aim" then
		local aim = math.sin(t * 1.3) * 1.4
		if math.floor(t / 3) % 2 == 1 then aim = math.pi - aim end
		return { grounded = true, vx = 0, aim = aim, flash = (t % 0.4) < 0.07 and 1 or 0 }
	end
	return { grounded = true, vx = 0, aim = turn }
end

local function preview(pilot, team)
	local w, h = pilot.size[1] * pilot.scale, pilot.size[2] * pilot.scale -- game units
	local a = acting(S.t)
	-- Room for the gun either side, the gun raised above and the flame below.
	local pad = 40
	local fw, fh = (w + 2 * pad) * 2, (h + 30) * 2
	return gfx.frame({ width = fw, height = fh, gfx.fill({ path = box(0, 0, fw, fh), brush = brush(P.colors.sky) }),
		gfx.instance({ visual = P.actor(a, S.t, pilot, team, preview_gun), transform = { 2, 0, 0, 2, pad * 2, 30 } }) })
end

local part_names = {
	arm_free = "Free upper arm", forearm_free = "Free forearm", hand_free = "Free hand", leg_back = "Back leg",
	leg_front = "Front leg", jetpack = "Jetpack", torso = "Torso", head = "Head", arm_gun = "Gun upper arm",
	forearm_gun = "Gun forearm", hand_gun = "Gun hand", finger_gun = "Trigger finger", thumb_gun = "Thumb",
}

local function var_row(pilot, name)
	local id = "var:" .. name
	local value = drafts[id] or pilot.vars[name]
	local bad = drafts[id] ~= nil and not F.hex(drafts[id])
	local shown = colour(pilot, "@" .. name)
	local s = S.selected and M.find(pilot, S.selected)
	return ui.row({ gap = 6, align_center = true,
		ui.button({ id = "fill:" .. name, w = 26, h = 26, radius = 4, fill = shown,
			stroke = { 2, (s and s.fill == "@" .. name) and C.select or C.edge },
			on_click = function()
				local chosen, part = nil, nil
				if S.selected then chosen, part = M.find(pilot, S.selected) end
				if chosen then M.set_fill(part.id, chosen.id, "@" .. name) end
			end }),
		ui.row({ w = 52, text(name, nil, C.muted, 11) }),
		ui.input({ id = id, value = value, grow = true, h = 26, px = 8, radius = 4, fill = C.bg,
			stroke = { 1, bad and C.bad or C.edge }, color = bad and C.bad or C.text, font_size = 12,
			on_input = function(v) drafts[id] = v end,
			on_enter = function()
				if M.set_var(name, drafts[id]) then drafts[id] = nil end
			end,
			on_esc = function() drafts[id] = nil end,
		}),
	})
end

local function selected_panel(pilot)
	local s, part = nil, nil
	if S.selected then s, part = M.find(pilot, S.selected) end
	if not s then return text("Click a shape · pick a swatch to fill it", "shape:selected", C.muted, 11) end
	return ui.col({ gap = 6,
		text(s.id .. " · " .. part.id, "shape:selected", C.accent, 12),
		ui.row({ gap = 6,
			button("Up", "shape:up", false, function() M.restack(pilot, s.id, 1) end),
			button("Down", "shape:down", false, function() M.restack(pilot, s.id, -1) end),
			button("Delete", "shape:delete", false, function()
				M.delete_shape(part.id, s.id)
				S.selected = nil
			end),
		}),
	})
end

return function()
	local pilot, valid, err = M.current()
	if not pilot then
		return ui.col({ full = true, center = true, fill = C.bg, text("This asset has no pilots", "editor:status", C.bad, 13) })
	end
	local count = 0
	for _, part in ipairs(pilot.parts or {}) do count += #(part.shapes or {}) end
	local status = M.notice or (valid and string.format("%d shapes · ready for the game", count))
		or ("Not playable: " .. tostring(err))

	local list = { gap = 6, w = 190, no_shrink = true, h_full = true, pad = 12, fill = C.panel, text("PILOTS", nil, C.muted, 11) }
	for _, p in ipairs(M.asset.pilots or {}) do
		table.insert(list, choice(p.name or p.id, "pilot:" .. p.id, p.id == M.shown, function()
			M.pilot_id, S.selected, drafts = p.id, nil, {}
		end))
	end
	table.insert(list, ui.col({ h = 6 }))
	table.insert(list, button("New pilot", "pilot:new", false, function()
		M.copy(pilot)
		S.selected, drafts = nil, {}
	end))
	table.insert(list, button("Delete pilot", "pilot:delete", false, function()
		M.delete_pilot(pilot)
		S.selected, drafts = nil, {}
	end))

	-- Parts in draw order, the back drawing then the front one; a child sits under its parent.
	table.insert(list, ui.col({ h = 4 }))
	table.insert(list, text("PARTS · new shapes go to the chosen one", nil, C.muted, 11))
	for i, id in ipairs(F.parts) do
		if i == 1 then table.insert(list, text("behind the gun", nil, C.muted, 10)) end
		if i == F.split + 1 then table.insert(list, text("in front of the gun", nil, C.muted, 10)) end
		local depth, up = 0, F.parents[id]
		while up do depth, up = depth + 1, F.parents[up] end
		table.insert(list, ui.row({ w_full = true, gap = 0, ui.col({ w = depth * 10 }),
			ui.button({ id = "part:" .. id, grow = true, h = 24, px = 8, radius = 5,
				fill = S.part == id and C.accent or C.bg, hover_fill = S.part == id and C.accent or C.edge,
				stroke = { 1, S.part == id and C.accent or C.edge }, on_click = function() S.part = id end,
				text(part_names[id], nil, S.part == id and C.bg or C.text, 11) }) }))
	end

	local palette = { gap = 6 }
	local names = { "main", "dark" }
	local rest = {}
	local vars = type(pilot.vars) == "table" and pilot.vars or {}
	for name in pairs(vars) do
		if name ~= "main" and name ~= "dark" then table.insert(rest, name) end
	end
	table.sort(rest)
	for _, name in ipairs(rest) do table.insert(names, name) end
	for _, name in ipairs(names) do
		if type(vars[name]) == "string" then table.insert(palette, var_row(pilot, name)) end
	end

	local shown = valid and stable(valid)
	local name_id = "pilot:name"
	return ui.col({ id = "pilot:root", full = true, fill = C.bg,
		-- Declared only while the preview plays: that is the repaint request.
		on_frame = (S.playing and shown) and function(e) S.t += e.dt end or nil,
		ui.row({ h = 52, w_full = true, px = 12, gap = 6, align_center = true, fill = C.panel,
			text("PILOT", nil, C.accent, 15),
			ui.input({ id = name_id, value = drafts[name_id] or pilot.name or pilot.id, w = 140, h = 30, px = 8, radius = 4,
				fill = C.bg, stroke = { 1, C.edge }, color = C.text, font_size = 12,
				on_input = function(v) drafts[name_id] = v end,
				on_enter = function() if M.rename(drafts[name_id]) then drafts[name_id] = nil end end,
				on_esc = function() drafts[name_id] = nil end }),
			ui.col({ w = 10 }),
			choice("Select", "tool:select", S.tool == "select", function() S.tool = "select" end),
			choice("Rect", "tool:rect", S.tool == "rect", function() S.tool = "rect" end),
			choice("Ellipse", "tool:ellipse", S.tool == "ellipse", function() S.tool = "ellipse" end),
			choice("Triangle", "tool:triangle", S.tool == "triangle", function() S.tool = "triangle" end),
			ui.col({ w = 10 }),
			ui.col({ grow = true }),
			text("COLOURS", nil, C.muted, 11),
			choice("Pilot", "team:pilot", S.team == "pilot", function() S.team = "pilot" end),
			choice("Player", "team:player", S.team == "player", function() S.team = "player" end),
			choice("Bot", "team:bot", S.team == "bot", function() S.team = "bot" end),
		}),
		ui.row({ grow = true, w_full = true, min_h = 200,
			ui.col(list),
			ui.col({ id = "pilot:scroll", grow = true, min_w = 200, h_full = true, scroll_x = true, scroll_y = true, pad = 16, gap = 10,
				drawable(pilot) and ui.frame({ id = "pilot:canvas", visual = canvas(pilot), drag_on_shapes = true,
					on_click = function(e) click(pilot, e) end,
					on_drag = function(e) drag(pilot, e) end,
				}) or text("This pilot cannot be drawn: " .. tostring(err), nil, C.bad, 12),
				text(M.makers[S.tool] and ("Click the pilot to add a " .. S.tool .. " to the " .. S.part .. " · Select to move it")
					or "Drag a shape to move it, its corner to resize, a dot to reshape · red dots: palm_gun, palm_free, hold, jet, eye · draw both arms reaching forward · blue: the part's pivot (its joint)",
					"editor:hint", C.muted, 11),
			}),
			ui.col({ w = 280, no_shrink = true, h_full = true, pad = 12, gap = 10, fill = C.panel,
				text("COLOURS · team slots main and dark", nil, C.muted, 11),
				ui.col(palette),
				text("SELECTED", nil, C.muted, 11),
				selected_panel(pilot),
				text("IN GAME", nil, C.muted, 11),
				ui.row({ gap = 4,
					choice("Walk", "motion:walk", S.motion == "walk", function() S.motion = "walk" end),
					choice("Fly", "motion:fly", S.motion == "fly", function() S.motion = "fly" end),
					choice("Aim", "motion:aim", S.motion == "aim", function() S.motion = "aim" end),
					choice("Idle", "motion:idle", S.motion == "idle", function() S.motion = "idle" end),
					choice(S.playing and "❚❚" or "▶", "motion:play", false, function() S.playing = not S.playing end),
				}),
				shown and ui.col({ gap = 0,
					ui.frame({ id = "preview:player", visual = preview(shown, "player") }),
					ui.frame({ id = "preview:bot", visual = preview(shown, "bot") }),
				}) or text("Not playable yet — see below", nil, C.bad, 11),
				ui.col({ grow = true }),
				ui.text({ status, id = "editor:status", color = (valid and not M.notice) and C.muted or C.bad, font_size = 11 }),
			}),
		}),
	})
end
