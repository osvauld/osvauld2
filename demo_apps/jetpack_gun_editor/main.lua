local C = require("theme")
local F = require("format")
local M = require("model")
local P = require("pilot_art")
local R = require("rig")

-- The pilot holding the preview gun: the workspace's first pilot, else the baked scout.
local pilots = F.load_pilots(asset:open("jetpack-pilots"), require("baked/pilots"))

-- Viewer state: the tool, the part new shapes go to, the selection and a drag in flight. The guns are
-- the asset's.
local S = { tool = "select", part = "body", selected = nil, drag = nil, motion = "idle", playing = true, t = 0 }
local drafts = {} -- field text not yet committed; cleared when another gun shows
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

local function colour(gun, fill)
	local name = type(fill) == "string" and fill:match("^@(.+)$")
	if not name then return type(fill) == "string" and fill or C.bad end
	return (gun.vars and gun.vars[name]) or C.bad
end

-- A shape's path this frame: the drag's live one, else the asset's.
local function live_path(gun, s)
	local d = S.drag
	if not (d and d.id == s.id) then return s.path end
	if d.mode == "shape" then return M.moved(gun, d.from, d.dx, d.dy) end
	if d.mode == "grip" then return M.scaled(gun, d.from, d.dx, d.dy) end
	if d.mode == "pt" then return M.with_point(gun, d.from, d.j, d.k, d.x + d.dx, d.y + d.dy) end
	return s.path
end
local function live_xy(d, name, x, y)
	if d and d.id == name then return x + d.dx, y + d.dy end
	return x, y
end

-- Enough structure to draw from: size, the required points and each part's pivot. Shapes are checked
-- one by one as they draw, so one broken shape hides itself and the status names it.
local function pair(v) return type(v) == "table" and type(v[1]) == "number" and type(v[2]) == "number" end
local function drawable(gun)
	if not (pair(gun.size) and type(gun.points) == "table" and type(gun.parts) == "table") then return false end
	for _, name in ipairs(F.gun_required) do
		if not pair(gun.points[name]) then return false end
	end
	if gun.points.support ~= nil and not pair(gun.points.support) then return false end
	for _, part in ipairs(gun.parts) do
		if type(part) ~= "table" or not pair(part.pivot) then return false end
	end
	return type(gun.scale) == "number"
end

local function canvas(gun)
	local w, h = gun.size[1], gun.size[2]
	local items = { gfx.fill({ path = box(0, 0, w * z, h * z), brush = brush(C.paper) }) }
	for x = 4, w - 1, 4 do
		table.insert(items, gfx.stroke({ path = gfx.path({ { "move", x * z, 0 }, { "line", x * z, h * z } }), brush = brush(C.grid), width = 1 }))
	end
	for y = 4, h - 1, 4 do
		table.insert(items, gfx.stroke({ path = gfx.path({ { "move", 0, y * z }, { "line", w * z, y * z } }), brush = brush(C.grid), width = 1 }))
	end
	local chosen = nil
	-- The part being edited draws on top, so a part behind another can still be seen and picked.
	local order = {}
	for _, part in ipairs(gun.parts or {}) do
		if part.id ~= S.part then table.insert(order, part) end
	end
	for _, part in ipairs(gun.parts or {}) do
		if part.id == S.part then table.insert(order, part) end
	end
	for _, part in ipairs(order) do
		for _, s in ipairs(type(part.shapes) == "table" and part.shapes or {}) do
			local path = type(s) == "table" and F.path(s.path, w, h, "") and live_path(gun, s)
			if path then table.insert(items, gfx.fill({ id = "shape:" .. s.id, path = scaled(path), brush = brush(colour(gun, s.fill)) })) end
			if path and s.id == S.selected then chosen = path end
		end
	end
	local d = S.drag
	-- The soldier's gun hand, to scale: a fist about four game units across closes round the grip.
	local gx, gy = live_xy(d, "point:grip", gun.points.grip[1], gun.points.grip[2])
	table.insert(items, gfx.fill({ path = dot(gx * z, gy * z, 3.6 / gun.scale * z), brush = brush(C.hand) }))
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
	for _, part in ipairs(gun.parts or {}) do
		if part.id == S.part then
			local px, py = live_xy(d, "pivot:" .. part.id, part.pivot[1], part.pivot[2])
			local r = C.dot * z * 0.9
			table.insert(items, gfx.fill({ id = "pivot:" .. part.id, path = box(px * z - r, py * z - r, 2 * r, 2 * r), brush = brush(C.pivot) }))
		end
	end
	for _, name in ipairs(F.gun_points) do
		local p = gun.points[name]
		if p then
			local px, py = live_xy(d, "point:" .. name, p[1], p[2])
			table.insert(items, gfx.fill({ id = "point:" .. name, path = dot(px * z, py * z, C.dot * z), brush = brush(C.points[name]) }))
		end
	end
	return gfx.frame({ width = w * z, height = h * z, table.unpack(items) })
end

local function shape_id(shape)
	if not shape then return nil end
	return string.match(shape, "^(%a+):(.+)$")
end

local function click(gun, e)
	local x, y = e.x / z, e.y / z
	if M.makers[S.tool] then
		S.selected = M.add_shape(gun, S.part, S.tool, x, y)
		return
	end
	local mode, id = shape_id(e.shape)
	if mode == "shape" then
		local _, part = M.find(gun, id)
		S.selected, S.part = id, part and part.id or S.part
	elseif mode ~= "grip" and mode ~= "pt" then
		S.selected = nil
	end
end

local function drag(gun, e)
	if e.phase == "start" then
		local mode, id = shape_id(e.shape)
		if not mode then return end
		local d = { mode = mode, dx = 0, dy = 0 }
		if mode == "shape" or mode == "grip" then
			local s, part = M.find(gun, id)
			if not s then return end
			S.selected, S.part = id, part.id
			d.id, d.part, d.from = id, part.id, s.path
		elseif mode == "pt" then
			local sid, j, k = string.match(id, "^(.+):(%d+):(%d+)$")
			local s, part = M.find(gun, sid)
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
			local p = gun.points[d.name]
			M.set_point(gun, d.name, p[1] + d.dx, p[2] + d.dy)
		elseif d.mode == "pivot" then
			local part = M.part_of(gun, d.name)
			if part then M.set_pivot(gun, d.name, part.pivot[1] + d.dx, part.pivot[2] + d.dy) end
		else
			local s = M.find(gun, d.id)
			if s then M.set_path(d.part, d.id, live_path(gun, s)) end
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

-- The gun as the game draws it (pilot_art.lua and gun_art.lua, the game's own code): the soldier holding
-- it, at 5×. The checked gun is a new table each frame; holding the last one while its content is the
-- same keeps gun_art's compiled drawing.
local held = { key = nil, gun = nil }
local function fingerprint(v, buf)
	if type(v) ~= "table" then buf[#buf + 1] = tostring(v); return end
	local keys = {}
	for k in pairs(v) do keys[#keys + 1] = k end
	table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
	for _, k in ipairs(keys) do
		buf[#buf + 1] = tostring(k)
		fingerprint(v[k], buf)
	end
end
local function stable(gun)
	local buf = {}
	fingerprint(gun, buf)
	local key = table.concat(buf, "|")
	if held.key ~= key then held.key, held.gun = key, gun end
	return held.gun
end

-- What the preview soldier is doing at time `t`: idle faces right, left faces left, aiming sweeps the
-- gun both ways and fires.
local function acting(t)
	local mode = S.motion
	if mode == "left" then return { grounded = true, vx = 0, aim = math.pi } end
	if mode == "aim" then
		local aim = math.sin(t * 1.3) * 1.4
		if math.floor(t / 3) % 2 == 1 then aim = math.pi - aim end
		return { grounded = true, vx = 0, aim = aim, flash = (t % 0.4) < 0.07 and 1 or 0 }
	end
	return { grounded = true, vx = 0, aim = 0 }
end

local PREVIEW = 5
local function preview(pilot, gun)
	local w, h = pilot.size[1] * pilot.scale, pilot.size[2] * pilot.scale
	local pad, top = 30, 10 -- room for the gun either side, and above when it aims up
	local fw, fh = (w + 2 * pad) * PREVIEW, (h + top + 4) * PREVIEW
	return gfx.frame({ width = fw, height = fh, gfx.fill({ path = box(0, 0, fw, fh), brush = brush(P.colors.sky) }),
		gfx.instance({ visual = P.actor(acting(S.t), S.t, pilot, "player", gun),
			transform = { PREVIEW, 0, 0, PREVIEW, pad * PREVIEW, top * PREVIEW } }) })
end

local part_names = { mag = "Magazine", body = "Body", slide = "Slide", bolt = "Bolt", barrel = "Barrel" }

local function var_row(gun, name)
	local id = "var:" .. name
	local value = drafts[id] or gun.vars[name]
	local bad = drafts[id] ~= nil and not F.hex(drafts[id])
	local s = S.selected and M.find(gun, S.selected)
	return ui.row({ gap = 6, align_center = true,
		ui.button({ id = "fill:" .. name, w = 24, h = 24, radius = 4, fill = colour(gun, "@" .. name),
			stroke = { 2, (s and s.fill == "@" .. name) and C.select or C.edge },
			on_click = function()
				local chosen, part = nil, nil
				if S.selected then chosen, part = M.find(gun, S.selected) end
				if chosen then M.set_fill(part.id, chosen.id, "@" .. name) end
			end }),
		ui.row({ w = 46, text(name, nil, C.muted, 11) }),
		ui.input({ id = id, value = value, grow = true, h = 24, px = 8, radius = 4, fill = C.bg,
			stroke = { 1, bad and C.bad or C.edge }, color = bad and C.bad or C.text, font_size = 12,
			on_input = function(v) drafts[id] = v end,
			on_enter = function()
				if M.set_var(name, drafts[id]) then drafts[id] = nil end
			end,
			on_esc = function() drafts[id] = nil end,
		}),
	})
end

-- A number field: shows its draft, commits on Enter, red while the draft is not a legal value.
local function number_field(gun, label, name, value, commit, ok)
	local id = "stat:" .. name
	local shown = drafts[id] or tostring(value)
	local bad = drafts[id] ~= nil and not ok(tonumber(drafts[id]))
	return ui.row({ gap = 4, align_center = true, grow = true, min_w = 90,
		ui.row({ w = 52, text(label, nil, C.muted, 10) }),
		ui.input({ id = id, value = shown, grow = true, h = 24, px = 6, radius = 4, fill = C.bg,
			stroke = { 1, bad and C.bad or C.edge }, color = bad and C.bad or C.text, font_size = 12,
			on_input = function(v) drafts[id] = v end,
			on_enter = function()
				if commit(drafts[id] or tostring(value)) then drafts[id] = nil end
			end,
			on_esc = function() drafts[id] = nil end,
		}),
	})
end

local stat_labels = { damage = "damage", fire_interval = "interval", shot_speed = "speed", spread = "spread",
	pellets = "pellets", mag_size = "mag", reload_time = "reload", range = "range", recoil = "recoil",
	melee = "melee", splash = "splash" }

local function stats_panel(gun)
	local st = type(gun.stats) == "table" and gun.stats or {}
	local function field(name)
		return number_field(gun, stat_labels[name], name, st[name] ~= nil and st[name] or "",
			function(v) return M.set_stat(name, v) end, function(v) return F.stat_ok(name, v) end)
	end
	local names = {}
	for _, name in ipairs(F.stat_order) do names[#names + 1] = name end
	if st.kind == "rocket" then names[#names + 1] = "splash" end
	local pairs_ = {}
	for i = 1, #names, 2 do
		pairs_[#pairs_ + 1] = ui.row({ gap = 6, field(names[i]), names[i + 1] and field(names[i + 1]) or ui.col({ grow = true }) })
	end
	-- Zoom levels: 1x is fixed; each further level widens the view by its factor (the zoom button steps through them).
	local zooms = M.zooms_of(gun)
	local zrows = {}
	for i = 2, #zooms do
		zrows[#zrows + 1] = number_field(gun, "zoom " .. i, "zoom:" .. i, zooms[i],
			function(v) return M.set_zoom(gun, i, v) end,
			function(v)
				local trial = M.zooms_of(gun)
				trial[i] = v
				return v ~= nil and F.zooms_ok(trial)
			end)
	end
	local zoom_rows = { gap = 6 }
	for i = 1, #zrows, 3 do
		zoom_rows[#zoom_rows + 1] = ui.row({ gap = 6, zrows[i], zrows[i + 1] or ui.col({ grow = true }), zrows[i + 2] or ui.col({ grow = true }) })
	end
	local scale = number_field(gun, "scale", "scale", gun.scale or "", M.set_scale,
		function(v) return v ~= nil and v >= F.gun_limits.min_scale and v <= F.gun_limits.max_scale end)
	return ui.col({ gap = 6,
		ui.row({ gap = 6,
			button("Bullet", "stat:kind:bullet", st.kind == "bullet", function() M.set_kind("bullet", gun) end),
			button("Rocket", "stat:kind:rocket", st.kind == "rocket", function() M.set_kind("rocket", gun) end),
			button(st.auto and "Auto: on" or "Auto: off", "stat:auto", st.auto == true, function() M.set_auto(not st.auto) end),
		}),
		ui.col({ gap = 6, pairs_ }),
		ui.row({ gap = 6, align_center = true, text("ZOOM LEVELS  1x" .. (#zooms == 1 and " only" or ""), "zoom:levels", C.muted, 10),
			button("+ level", "zoom:add", false, function() M.add_zoom(gun) end),
			button("- level", "zoom:remove", false, function() M.remove_zoom(gun) end) }),
		#zrows > 0 and ui.col(zoom_rows) or false,
		ui.row({ gap = 6, scale, ui.col({ grow = true }) }),
	})
end

local function selected_panel(gun)
	local s, part = nil, nil
	if S.selected then s, part = M.find(gun, S.selected) end
	if not s then return text("Click a shape · pick a swatch to fill it", "shape:selected", C.muted, 11) end
	return ui.col({ gap = 6,
		text(s.id .. " · " .. part.id, "shape:selected", C.accent, 12),
		ui.row({ gap = 6,
			button("Up", "shape:up", false, function() M.restack(gun, s.id, 1) end),
			button("Down", "shape:down", false, function() M.restack(gun, s.id, -1) end),
			button("Delete", "shape:delete", false, function()
				M.delete_shape(part.id, s.id)
				S.selected = nil
			end),
		}),
	})
end

-- Can the scout's free hand reach this gun's support point at every aim? The editor says so, since a
-- gun the hands cannot hold is not playable well.
local function reach_warning(gun, valid)
	local pilot = pilots[1]
	if not (valid and pilot and valid.points.support) then return nil end
	local hold = R.gun_hold(valid, pilot)
	for deg = -85, 85, 5 do
		if not R.pose(pilot, math.rad(deg), hold).reach.free then
			return "The free hand cannot reach the support point at some aims: bring it nearer the grip or shrink the scale"
		end
	end
end

return function()
	local gun, valid, err = M.current()
	if not gun then
		return ui.col({ full = true, center = true, fill = C.bg, text("This asset has no guns", "editor:status", C.bad, 13) })
	end
	local count = 0
	for _, part in ipairs(gun.parts or {}) do count += #(part.shapes or {}) end
	local warn = reach_warning(gun, valid)
	local status = M.notice or (valid and (warn or string.format("%d shapes · ready for the game", count)))
		or ("Not playable: " .. tostring(err))
	local status_bad = M.notice ~= nil or not valid or warn ~= nil

	local list = { gap = 6, w = 190, no_shrink = true, h_full = true, pad = 12, fill = C.panel, text("GUNS", nil, C.muted, 11) }
	for _, g in ipairs(M.asset.guns or {}) do
		table.insert(list, button(g.name or g.id, "gun:" .. g.id, g.id == M.shown, function()
			M.gun_id, S.selected, drafts = g.id, nil, {}
		end))
	end
	table.insert(list, ui.col({ h = 6 }))
	table.insert(list, button("New gun", "gun:new", false, function()
		M.blank()
		S.selected, drafts = nil, {}
	end))
	table.insert(list, button("Duplicate", "gun:copy", false, function()
		M.copy(gun)
		S.selected, drafts = nil, {}
	end))
	table.insert(list, button("Delete gun", "gun:delete", false, function()
		M.delete_gun(gun)
		S.selected, drafts = nil, {}
	end))

	table.insert(list, ui.col({ h = 4 }))
	table.insert(list, text("PARTS · new shapes go to the chosen one", nil, C.muted, 11))
	for _, id in ipairs(F.gun_parts) do
		local have = drawable(gun) and M.part_of(gun, id) ~= nil
		table.insert(list, ui.button({ id = "part:" .. id, w_full = true, h = 24, px = 8, radius = 5,
			fill = S.part == id and C.accent or C.bg, hover_fill = S.part == id and C.accent or C.edge,
			stroke = { 1, S.part == id and C.accent or C.edge }, on_click = function() S.part = id end,
			text(part_names[id] .. (have and "" or " (none yet)"), nil, S.part == id and C.bg or (have and C.text or C.muted), 11) }))
	end
	table.insert(list, ui.col({ h = 4 }))
	table.insert(list, text("POINTS", nil, C.muted, 11))
	table.insert(list, ui.text({ "grip red · support blue · muzzle green · eject yellow · mag purple", color = C.muted, font_size = 10 }))
	table.insert(list, gun.points and gun.points.support ~= nil
		and button("Make one-handed", "support:remove", false, function() M.remove_support() end)
		or button("Add support", "support:add", false, function() M.add_support(gun) end))

	local palette = { gap = 6 }
	local vars = type(gun.vars) == "table" and gun.vars or {}
	local names = {}
	for name in pairs(vars) do names[#names + 1] = name end
	table.sort(names)
	for _, name in ipairs(names) do
		if type(vars[name]) == "string" then table.insert(palette, var_row(gun, name)) end
	end

	local pilot = pilots[1]
	local shown = valid and pilot and stable(valid)
	local name_id = "gun:name"
	local aiming = S.playing and shown and S.motion == "aim"
	return ui.col({ id = "gun:root", full = true, fill = C.bg,
		-- Declared only while the preview moves: that is the repaint request.
		on_frame = aiming and function(e) S.t += e.dt end or nil,
		ui.row({ h = 52, w_full = true, px = 12, gap = 6, align_center = true, fill = C.panel,
			text("GUN", nil, C.accent, 15),
			ui.input({ id = name_id, value = drafts[name_id] or gun.name or gun.id, w = 140, h = 30, px = 8, radius = 4,
				fill = C.bg, stroke = { 1, C.edge }, color = C.text, font_size = 12,
				on_input = function(v) drafts[name_id] = v end,
				on_enter = function() if M.rename(drafts[name_id]) then drafts[name_id] = nil end end,
				on_esc = function() drafts[name_id] = nil end }),
			ui.col({ w = 10 }),
			button("Select", "tool:select", S.tool == "select", function() S.tool = "select" end),
			button("Rect", "tool:rect", S.tool == "rect", function() S.tool = "rect" end),
			button("Ellipse", "tool:ellipse", S.tool == "ellipse", function() S.tool = "ellipse" end),
			button("Triangle", "tool:triangle", S.tool == "triangle", function() S.tool = "triangle" end),
		}),
		ui.row({ grow = true, w_full = true, min_h = 200,
			ui.col(list),
			ui.col({ id = "gun:scroll", grow = true, min_w = 200, h_full = true, scroll_x = true, scroll_y = true, pad = 16, gap = 10,
				drawable(gun) and ui.frame({ id = "gun:canvas", visual = canvas(gun), drag_on_shapes = true,
					on_click = function(e) click(gun, e) end,
					on_drag = function(e) drag(gun, e) end,
				}) or text("This gun cannot be drawn: " .. tostring(err), nil, C.bad, 12),
				ui.text({ M.makers[S.tool] and ("Click the gun to add a " .. S.tool .. " to the " .. S.part .. " · Select to move it")
					or "Drag a shape to move it, its corner to resize, a dot to reshape · coloured dots are the gun's points · black square: the part's pivot · the tan disc is the soldier's fist",
					id = "editor:hint", color = C.muted, font_size = 11 }),
				text("IN HAND", nil, C.muted, 11),
				ui.row({ gap = 6,
					button("Right", "motion:idle", S.motion == "idle", function() S.motion = "idle" end),
					button("Left", "motion:left", S.motion == "left", function() S.motion = "left" end),
					button("Aim", "motion:aim", S.motion == "aim", function() S.motion = "aim" end),
					button(S.playing and "❚❚" or "▶", "motion:play", false, function() S.playing = not S.playing end),
				}),
				(shown and pilot) and ui.frame({ id = "preview:hand", visual = preview(pilot, shown) })
					or text("Not playable yet — see the status", nil, C.bad, 11),
			}),
			ui.col({ id = "gun:side", w = 300, no_shrink = true, h_full = true, pad = 12, gap = 10, fill = C.panel, scroll_y = true,
				text("STATS", nil, C.muted, 11),
				stats_panel(gun),
				text("COLOURS", nil, C.muted, 11),
				ui.col(palette),
				text("SELECTED", nil, C.muted, 11),
				selected_panel(gun),
				ui.text({ status, id = "editor:status", color = status_bad and C.bad or C.muted, font_size = 11 }),
			}),
		}),
	})
end
