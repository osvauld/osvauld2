local C = require("theme")
local M = require("model")
local Stage = require("stage")
local Inspector = require("inspector")

local selected = nil -- the selected part's id
local built = { key = nil, frame = nil } -- the canvas visual; rebuilt on a new selection or edit
local play = nil -- { name, drawing, clip } while a clip plays on the canvas; nil is edit mode

-- The canvas draws the asset scaled up by building a scaled copy, rather than scaling the
-- element: hits keep reporting part ids, and canvas units are drawing units times the zoom.
local function scaled_path(path, z)
	local out = {}
	for i, cmd in ipairs(path) do
		local c = { cmd[1] }
		for j = 2, #cmd do
			c[j] = cmd[j] * z
		end
		out[i] = c
	end
	return out
end

-- A colour stays as it is; a gradient's points and radii are in drawing space, so they scale
-- too. Angles and stops don't.
local function scaled_fill(fill, z)
	if type(fill) ~= "table" then
		return fill
	end
	local function pt(p)
		return p and { p[1] * z, p[2] * z }
	end
	local kind, g = next(fill)
	local out = { stops = g.stops, extend = g.extend }
	if kind == "linear" then
		out.from, out.to = pt(g.from), pt(g.to)
	elseif kind == "radial" then
		out.center, out.focus = pt(g.center), pt(g.focus)
		out.radius, out.focus_radius = g.radius * z, g.focus_radius and g.focus_radius * z
	else -- sweep
		out.center, out.from_angle, out.to_angle = pt(g.center), g.from_angle, g.to_angle
	end
	return { [kind] = out }
end

-- A lone length may be a var, `"@line"`: it stays a name here, and the vars scale instead.
local function len(v, z)
	return type(v) == "number" and v * z or v
end

local function scaled_shape(s, z)
	if s.shadow then
		local sh, r = s.shadow, s.shadow.rect
		local rect = { r[1] * z, r[2] * z, r[3] * z, r[4] * z }
		local radius = len(sh.radius or 0, z)
		return { shadow = { rect = rect, radius = radius, blur = len(sh.blur, z), color = sh.color } }
	end
	local stroke = s.stroke and { len(s.stroke[1], z), s.stroke[2] }
	return { path = scaled_path(s.path, z), fill = scaled_fill(s.fill, z), stroke = stroke }
end

-- The asset at canvas scale, with the `outline` part's outlines drawn again in the accent colour.
local function canvas_drawing(parts, z, outline)
	local out = {}
	for i, p in ipairs(parts) do
		local shapes = {}
		for _, s in ipairs(p.shapes or {}) do
			table.insert(shapes, scaled_shape(s, z))
		end
		if p.id == outline then
			for _, s in ipairs(p.shapes or {}) do
				if s.path then -- a shadow has no outline to trace
					table.insert(shapes, { path = scaled_path(s.path, z), stroke = { 3, C.accent } })
				end
			end
		end
		out[i] = {
			id = p.id,
			parent = p.parent,
			pivot = { p.pivot[1] * z, p.pivot[2] * z },
			shapes = #shapes > 0 and shapes or nil,
		}
	end
	local size = M.asset.size
	local vars = {} -- a number var is always a length
	for name, v in pairs(M.asset.vars or {}) do
		vars[name] = len(v, z)
	end
	return gfx.drawing({ size = { size[1] * z, size[2] * z }, vars = vars, parts = out })
end

local function visual(parts)
	local key = M.name .. ":" .. (selected or "") .. ":" .. Inspector.rev
	if built.key ~= key then
		built.key = key
		built.frame = canvas_drawing(parts, C.zoom, selected):pose({})
	end
	return built.frame
end

-- A clip at canvas scale: moves are in drawing units, so x and y scale; turns and scales don't.
local function scaled_clip(clip, z)
	local tracks = {}
	for part, props in pairs(clip.tracks) do
		tracks[part] = {}
		for prop, keys in pairs(props) do
			local k = (prop == "x" or prop == "y") and z or 1
			local out = {}
			for i, key in ipairs(keys) do
				out[i] = { key[1], key[2] * k, key[3] }
			end
			tracks[part][prop] = out
		end
	end
	return { length = clip.length, loop = clip.loop, tracks = tracks }
end

-- Every press makes a new clip handle, which is what restarts a clip that is already playing.
local function start(name)
	play = {
		name = name,
		drawing = canvas_drawing(M.parts(), C.zoom, nil),
		clip = gfx.clip(scaled_clip(M.asset.clips[name], C.zoom)),
	}
end

local function chip(id, label, on, on_click)
	return ui.button({
		id = id,
		px = 10,
		py = 6,
		radius = 6,
		fill = on and C.row_selected or C.panel,
		hover_fill = on and C.row_selected or C.row_hover,
		on_click = on_click,
		ui.text({ label, color = on and C.accent or C.text, font_size = 13, no_wrap = true }),
	})
end

local function asset_tab(name)
	return chip("asset:" .. name, name, name == M.name, function()
		selected, play = nil, nil
		M.open(name)
	end)
end

-- "edit" shows the rest pose and selects parts; a clip name plays that clip in a world.
local function clip_bar()
	local names = {}
	for name in pairs(M.asset.clips or {}) do
		table.insert(names, name)
	end
	table.sort(names)
	local chips = { chip("mode:edit", "edit", play == nil, function()
		play = nil
	end) }
	for _, name in ipairs(names) do
		local on = play ~= nil and play.name == name
		table.insert(chips, chip("clip:" .. name, name, on, function()
			start(name)
		end))
	end
	return ui.row({ gap = 6, wrap = true, chips })
end

-- Edit mode is a frame, so a click names the part; play mode is a world, whose clicks would
-- only name the whole asset — hence two modes rather than one.
local function canvas(parts)
	if play then
		local size = M.asset.size
		return ui.world({
			id = "stage",
			width = size[1] * C.zoom,
			height = size[2] * C.zoom,
			{ id = M.name, pos = { 0, 0 }, drawing = play.drawing, clip = play.clip },
		})
	end
	return ui.frame({
		id = "canvas",
		visual = visual(parts),
		-- A click on no part clears the selection.
		on_click = function(e)
			selected = e.shape
		end,
	})
end

local function part_row(p)
	local on = p.id == selected
	return ui.button({
		id = "part:" .. p.id,
		px = 12,
		py = 8,
		gap = 8,
		align_center = true,
		radius = 6,
		fill = on and C.row_selected or C.panel,
		hover_fill = on and C.row_selected or C.row_hover,
		on_click = function()
			selected = p.id
		end,
		ui.text({ p.id, color = on and C.accent or C.text, font_size = 13, no_wrap = true }),
		p.parent and ui.text({ "in " .. p.parent, color = C.muted, font_size = 11, no_wrap = true })
			or false,
	})
end

local function entity_row(r)
	return ui.row({
		px = 12,
		py = 8,
		gap = 8,
		align_center = true,
		ui.text({ r.id, color = C.text, font_size = 13, no_wrap = true }),
		ui.text({ r.note, color = C.muted, font_size = 11, no_wrap = true }),
	})
end

return function()
	local stage = M.scenes[M.name]
	local parts = not stage and M.parts() or nil
	local tabs, rows = {}, {}
	for _, name in ipairs(M.names) do
		table.insert(tabs, asset_tab(name))
	end
	for name in pairs(M.scenes) do
		table.insert(tabs, asset_tab(name))
	end
	local picked = nil
	for _, p in ipairs(parts or {}) do
		table.insert(rows, part_row(p))
		picked = p.id == selected and p or picked
	end
	for _, r in ipairs(stage and Stage.rows() or {}) do
		table.insert(rows, entity_row(r))
	end
	local title = M.name .. " · " .. (stage and "stage" or M.view)
	local listing = stage and "entities, back to front" or "parts, back to front"
	local body = stage and Stage.view()
		or parts and canvas(parts)
		or ui.text({ "opening the asset…", color = C.muted, no_wrap = true })
	return ui.row({
		full = true,
		fill = C.bg,
		ui.col({
			grow = true,
			fill = C.canvas,
			pad = 12,
			gap = 12,
			parts and clip_bar() or false,
			-- Ctrl+wheel zooms around the pointer and a drag on empty space pans; clicks still
			-- report canvas units, zoom undone.
			ui.col({
				id = "viewport",
				grow = true,
				w_full = true,
				center = true,
				zoomable = true,
				body,
			}),
		}),
		ui.col({
			w = C.panel_w,
			no_shrink = true,
			fill = C.panel,
			pad = 12,
			gap = 4,
			ui.row({ gap = 6, mb = 8, wrap = true, tabs }),
			ui.text({ title, color = C.text, font_size = 15, no_wrap = true }),
			ui.text({ listing, color = C.muted, font_size = 11, no_wrap = true }),
			rows,
			Inspector.view(picked),
		}),
	})
end
