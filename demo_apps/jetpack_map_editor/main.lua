local C = require("theme")
local M = require("model")

-- Viewer state: the tool, the zoom, the selection and a drag in flight. The map is the asset's.
local S = { tool = "select", zoom = 3, selected = nil, drag = nil }
local drafts = {}
local brushes = {}

local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end
local function box(x, y, w, h)
	return gfx.path({ { "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } })
end
local function circle(x, y, r)
	local k = 0.5523 * r
	return gfx.path({ { "move", x + r, y },
		{ "cubic", x + r, y + k, x + k, y + r, x, y + r }, { "cubic", x - k, y + r, x - r, y + k, x - r, y },
		{ "cubic", x - r, y - k, x - k, y - r, x, y - r }, { "cubic", x + k, y - r, x + r, y - k, x + r, y },
		{ "close" } })
end

local function zoom() return C.zooms[S.zoom] end

-- A thing's box this frame: the drag's live value, else the asset's.
local function live(map, thing, kind)
	local d = S.drag
	if not (d and d.id == thing.id) then return thing.x, thing.y, thing.w, thing.h end
	if kind == "spawn" or kind == "weapon" then
		return math.max(0, math.min(map.w, M.snap(d.from.x + d.dx))), math.max(0, math.min(map.h, M.snap(d.from.y + d.dy)))
	elseif d.mode == "grip" then
		return M.fit(map, d.from.x, d.from.y, d.from.w + d.dx, d.from.h + d.dy, true)
	end
	return M.fit(map, d.from.x + d.dx, d.from.y + d.dy, d.from.w, d.from.h)
end

local function canvas(map)
	local z = zoom()
	local items = { gfx.fill({ path = box(0, 0, map.w * z, map.h * z), brush = brush(C.sky) }) }
	for x = C.grid_step, map.w - 1, C.grid_step do
		table.insert(items, gfx.stroke({ path = gfx.path({ { "move", x * z, 0 }, { "line", x * z, map.h * z } }),
			brush = brush(C.grid), width = 1 }))
	end
	for y = C.grid_step, map.h - 1, C.grid_step do
		table.insert(items, gfx.stroke({ path = gfx.path({ { "move", 0, y * z }, { "line", map.w * z, y * z } }),
			brush = brush(C.grid), width = 1 }))
	end
	local chosen = nil
	for _, p in ipairs(map.platforms or {}) do
		local x, y, w, h = live(map, p, "platform")
		table.insert(items, gfx.fill({ id = "platform:" .. p.id, path = box(x * z, y * z, w * z, h * z),
			brush = brush(C[p.kind] or C.bad) }))
		if p.id == S.selected then chosen = { x, y, w, h } end
	end
	for _, s in ipairs(map.spawns or {}) do
		local x, y = live(map, s, "spawn")
		local r = C.spawn_r * z
		table.insert(items, gfx.fill({ id = "spawn:" .. s.id, path = circle(x * z, y * z, r),
			brush = brush(s.team == "player" and C.player or C.bot) }))
		table.insert(items, gfx.stroke({ path = circle(x * z, y * z, r), brush = brush(s.id == S.selected and C.select or C.spawn_ring),
			width = s.id == S.selected and 3 or 1.5 }))
	end
	for _, w in ipairs(map.weapon_spawns or {}) do
		local x, y = live(map, w, "weapon")
		local r = C.spawn_r * z * 0.9
		local lit = w.id == S.selected
		table.insert(items, gfx.fill({ id = "weapon:" .. w.id, path = gfx.path({ { "move", x * z, y * z - r }, { "line", x * z + r, y * z },
			{ "line", x * z, y * z + r }, { "line", x * z - r, y * z }, { "close" } }), brush = brush(C.weapon) }))
		table.insert(items, gfx.stroke({ path = circle(x * z, y * z, r * 1.25), brush = brush(lit and C.select or C.weapon_ring),
			width = lit and 3 or 1.5 }))
	end
	if chosen then
		local x, y, w, h = chosen[1] * z, chosen[2] * z, chosen[3] * z, chosen[4] * z
		table.insert(items, gfx.stroke({ path = box(x, y, w, h), brush = brush(C.select), width = 2 }))
		local g = C.grip
		table.insert(items, gfx.fill({ id = "grip:" .. S.selected, path = box(x + w - g / 2, y + h - g / 2, g, g),
			brush = brush(C.select) }))
	end
	return gfx.frame({ width = map.w * z, height = map.h * z, table.unpack(items) })
end

local function shape_id(shape)
	if not shape then return nil end
	local mode, id = string.match(shape, "^(%a+):(.+)$")
	return id, mode
end

local function click(map, e)
	local z = zoom()
	local x, y = e.x / z, e.y / z
	if S.tool == "ledge" or S.tool == "cover" or S.tool == "ground" then
		S.selected = M.add_platform(map, S.tool, x, y)
	elseif S.tool == "player" then
		local you = M.player_spawn(map)
		if you then M.place_spawn(map, you.id, x, y); S.selected = you.id end
	elseif S.tool == "bot" then
		S.selected = M.add_bot(map, x, y)
	elseif S.tool == "weapon" then
		S.selected = M.add_weapon(map, x, y)
	else
		S.selected = (shape_id(e.shape))
	end
end

local function drag(map, e)
	local z = zoom()
	if e.phase == "start" then
		local id, mode = shape_id(e.shape)
		if not id then return end
		local thing, kind = M.find(map, id)
		if not thing then return end
		S.selected = id
		S.drag = { id = id, mode = mode, kind = kind, dx = 0, dy = 0,
			from = { x = thing.x, y = thing.y, w = thing.w, h = thing.h } }
		return
	end
	local d = S.drag
	if not d then return end
	d.dx, d.dy = e.dx / z, e.dy / z
	if e.phase == "end" then
		local thing = M.find(map, d.id)
		if thing then
			local x, y, w, h = live(map, thing, d.kind)
			if d.kind == "spawn" then M.place_spawn(map, d.id, x, y)
			elseif d.kind == "weapon" then M.place_weapon(map, d.id, x, y)
			else M.place_platform(map, d.id, x, y, w, h, d.mode == "grip") end
		end
		S.drag = nil
	end
end

local function text(value, id, color, size)
	return ui.text({ value, id = id, color = color or C.text, font_size = size or 12, no_wrap = true })
end
local function button(label, id, on, action)
	return ui.button({ id = id, h = 32, px = 10, radius = 6, center = true,
		fill = on and C.edge or C.panel, hover_fill = C.edge, stroke = { 1, on and C.accent or C.edge },
		on_click = action, text(label, nil, on and C.accent or C.text, 12) })
end
local function tool(name, label)
	return button(label, "tool:" .. name, S.tool == name, function() S.tool = name end)
end

local function field(map, key, label)
	local id = "map:" .. key
	local value = drafts[id] or tostring(map[key])
	local bad = drafts[id] ~= nil and tonumber(drafts[id]) == nil
	return ui.row({ gap = 6, align_center = true,
		ui.row({ w = 52, text(label, nil, C.muted, 11) }),
		ui.input({ id = id, value = value, grow = true, h = 28, px = 8, radius = 4, fill = C.bg,
			stroke = { 1, bad and C.bad or C.edge }, color = bad and C.bad or C.text, font_size = 12,
			on_input = function(v) drafts[id] = v end,
			on_enter = function()
				if M.set_size(map, key, tonumber(drafts[id] or "")) then drafts[id] = nil end
			end,
			on_esc = function() drafts[id] = nil end,
		}),
	})
end

-- A weapon spawn's gun pool (a row per gun: how many times it is listed) and respawn time.
local function weapon_panel(map, thing)
	local rows = {}
	for _, gun in ipairs(M.guns) do
		table.insert(rows, ui.row({ gap = 6, align_center = true,
			ui.row({ grow = true, text(gun.name, nil, M.weight(thing, gun.id) > 0 and C.text or C.muted, 12) }),
			button("−", "pool:" .. gun.id .. ":minus", false, function() M.add_to_pool(thing, gun.id, -1) end),
			ui.row({ w = 18, center = true, text(tostring(M.weight(thing, gun.id)), "pool:" .. gun.id .. ":count", C.accent, 12) }),
			button("+", "pool:" .. gun.id .. ":plus", false, function() M.add_to_pool(thing, gun.id, 1) end),
		}))
	end
	local key = "weapon:respawn:" .. thing.id
	local value = drafts[key] or tostring(thing.respawn)
	local bad = drafts[key] ~= nil and tonumber(drafts[key]) == nil
	return ui.col({ gap = 6,
		text("GUNS (weight)", nil, C.muted, 11), rows,
		ui.row({ gap = 6, align_center = true,
			ui.row({ w = 52, text("Respawn s", nil, C.muted, 11) }),
			ui.input({ id = "weapon:respawn", value = value, grow = true, h = 28, px = 8, radius = 4, fill = C.bg,
				stroke = { 1, bad and C.bad or C.edge }, color = bad and C.bad or C.text, font_size = 12,
				on_input = function(v) drafts[key] = v end,
				on_enter = function()
					if M.set_respawn(thing, tonumber(drafts[key] or "")) then drafts[key] = nil end
				end,
				on_esc = function() drafts[key] = nil end,
			}),
		}),
	})
end

local function selected_panel(map)
	local thing, kind = nil, nil
	if S.selected then thing, kind = M.find(map, S.selected) end
	if not thing then
		return text("Nothing selected", nil, C.muted, 11)
	end
	local what = kind == "spawn" and (thing.team .. " spawn") or kind == "weapon" and "weapon spawn" or thing.kind
	local point = kind == "spawn" or kind == "weapon"
	local where = point and string.format("%.0f, %.0f", thing.x, thing.y)
		or string.format("%.0f, %.0f  ·  %.0f × %.0f", thing.x, thing.y, thing.w, thing.h)
	return ui.col({ gap = 6,
		text(string.upper(what), "selected:kind", C.accent, 12),
		text(where, "selected:box", C.muted, 11),
		kind == "weapon" and weapon_panel(map, thing) or false,
		button("Delete", "selected:delete", false, function()
			if M.delete(map, thing.id) then S.selected = nil end
		end),
	})
end

return function()
	local map, valid, err = M.current()
	if not map then
		return ui.col({ full = true, center = true, fill = C.bg, text("This asset has no map called " .. M.map_id, "editor:status", C.bad, 13) })
	end
	local bots = 0
	for _, s in ipairs(map.spawns or {}) do if s.team == "bot" then bots += 1 end end
	local status = M.notice or (valid and string.format("%d platforms · %d bots · %d weapon spawns · ready for the game",
		#map.platforms, bots, #(map.weapon_spawns or {})))
		or ("Not playable: " .. tostring(err))
	return ui.col({ full = true, fill = C.bg,
		ui.row({ h = 52, w_full = true, px = 12, gap = 8, align_center = true, fill = C.panel,
			text("MAP", nil, C.accent, 15), text(map.name or map.id, "map:name", C.text, 13),
			ui.col({ w = 12 }),
			tool("select", "Select"), tool("ledge", "Ledge"), tool("cover", "Cover"), tool("ground", "Ground"),
			tool("player", "Player spawn"), tool("bot", "Bot spawn"), tool("weapon", "Weapon spawn"),
			ui.col({ grow = true }),
			button("−", "zoom:out", false, function() S.zoom = math.max(1, S.zoom - 1) end),
			text(math.floor(zoom() * 100) .. "%", "zoom:level", C.muted, 11),
			button("+", "zoom:in", false, function() S.zoom = math.min(#C.zooms, S.zoom + 1) end),
		}),
		ui.row({ grow = true, w_full = true, min_h = 200,
			ui.col({ id = "map:scroll", grow = true, min_w = 200, h_full = true, scroll_x = true, scroll_y = true, pad = 12,
				ui.frame({ id = "map:canvas", visual = canvas(map), drag_on_shapes = true,
					on_click = function(e) click(map, e) end,
					on_drag = function(e) drag(map, e) end,
				}),
			}),
			ui.col({ w = 250, no_shrink = true, h_full = true, pad = 12, gap = 12, fill = C.panel,
				text("MAP SIZE", nil, C.muted, 11),
				field(map, "w", "Width"), field(map, "h", "Height"),
				text("SELECTED", nil, C.muted, 11),
				selected_panel(map),
				ui.col({ grow = true }),
				ui.text({ status, id = "editor:status", color = (valid and not M.notice) and C.muted or C.bad, font_size = 11 }),
				ui.text({ "Click to place with a tool. Drag anything to move it, its corner to resize.",
					color = C.muted, font_size = 11 }),
			}),
		}),
	})
end
