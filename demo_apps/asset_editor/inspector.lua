-- The selected part's fields as inputs. A field holds a draft while you type; Enter checks it and
-- writes once, Esc drops it. A value that doesn't parse turns the field red and writes nothing,
-- so the drawing never sees it.
local C = require("theme")
local M = require("model")

local I = { rev = 0 } -- bumped by every write: the canvas rebuilds from the patched mirror

local drafts = {} -- field id -> the text being typed
local showing = nil -- whose fields `drafts` belongs to

local function number(s)
	return tonumber(s)
end

local function positive(s)
	local n = tonumber(s)
	return n and n > 0 and n or nil
end

-- What the asset's var `name` holds: "colour", "number", or nil when it has no such var.
local function var_kind(name)
	local v = M.asset.vars and M.asset.vars[name]
	return type(v) == "string" and "colour" or type(v) == "number" and "number" or nil
end

local function plain_colour(s)
	return pcall(gfx.solid, s) and s or nil
end

-- A colour field takes a colour or `@name` of a colour var; typing a colour unlinks the shape.
local function colour(s)
	local name = s:match("^@(.+)$")
	if name then
		return var_kind(name) == "colour" and s or nil
	end
	return plain_colour(s)
end

-- A width takes a positive number or `@name` of a number var.
local function length(s)
	local name = s:match("^@(.+)$")
	if name then
		return var_kind(name) == "number" and s or nil
	end
	return positive(s)
end

-- A small square of the colour a value shows: a var's, when it names one.
local function swatch(v)
	local name = type(v) == "string" and v:match("^@(.+)$")
	local c = name and M.asset.vars and M.asset.vars[name] or v
	local ok = type(c) == "string" and plain_colour(c)
	return ui.row({ w = 14, h = 14, no_shrink = true, radius = 3, fill = ok and c or C.canvas,
		stroke = { 1, C.muted } })
end

-- `base` extended by more segments: a new table, so the base can be shared.
local function at(base, ...)
	local out = table.clone(base)
	for _, seg in ipairs({ ... }) do
		table.insert(out, seg)
	end
	return out
end

-- `lead` (optional) sits between the label and the input: a colour's swatch.
local function field(id, label, value, parse, path, lead)
	local text = drafts[id] or tostring(value)
	local bad = drafts[id] ~= nil and parse(drafts[id]) == nil
	local function commit()
		local v = drafts[id] and parse(drafts[id])
		if v ~= nil then
			M.asset:set(path, v)
			drafts[id] = nil
			I.rev += 1
		end
	end
	return ui.row({
		gap = 6,
		align_center = true,
		ui.row({ w = 72, ui.text({ label, color = C.muted, font_size = 11, no_wrap = true }) }),
		lead or false,
		ui.input({
			id = id,
			value = text,
			grow = true,
			h = 26,
			px = 8,
			radius = 4,
			fill = C.canvas,
			stroke = { 1, bad and C.bad or C.row_selected },
			color = bad and C.bad or C.text,
			font_size = 12,
			on_input = function(v)
				drafts[id] = v
			end,
			on_enter = commit,
			on_esc = function()
				drafts[id] = nil
			end,
		}),
	})
end

local function note(label, text)
	return ui.row({
		gap = 6,
		ui.row({ w = 72, ui.text({ label, color = C.muted, font_size = 11, no_wrap = true }) }),
		ui.text({ text, color = C.muted, font_size = 12, no_wrap = true }),
	})
end

-- One shape's fields: a gradient fill is shown but not edited yet; a missing stroke isn't offered.
local function shape_fields(base, i, s, out)
	local id = "field:" .. base[4] .. ":" .. i .. ":"
	local here = at(base, "shapes", i)
	table.insert(out, ui.text({ "shape " .. i, color = C.text, font_size = 12, mt = 6, no_wrap = true }))
	if s.shadow then
		local sh = at(here, "shadow")
		table.insert(out, field(id .. "blur", "blur", s.shadow.blur, length, at(sh, "blur")))
		table.insert(out, field(id .. "color", "colour", s.shadow.color, colour, at(sh, "color"), swatch(s.shadow.color)))
		return
	end
	if type(s.fill) == "string" then
		table.insert(out, field(id .. "fill", "fill", s.fill, colour, at(here, "fill"), swatch(s.fill)))
	elseif s.fill then
		table.insert(out, note("fill", next(s.fill) .. " gradient"))
	end
	if s.stroke then
		table.insert(out, field(id .. "width", "stroke", s.stroke[1], length, at(here, "stroke", 1)))
		table.insert(out, field(id .. "ink", "ink", s.stroke[2], colour, at(here, "stroke", 2), swatch(s.stroke[2])))
	end
end

-- The asset's vars, one field each: edit one and every shape naming it changes. A var holds a
-- plain value — no var names another.
function I.vars()
	local vars = M.asset.vars
	local names = {}
	for name in pairs(vars or {}) do
		table.insert(names, name)
	end
	if #names == 0 then
		return {}
	end
	table.sort(names)
	local out = { ui.text({ "vars", color = C.accent, font_size = 13, mt = 12, no_wrap = true }) }
	for _, name in ipairs(names) do
		local v = vars[name]
		local is_colour = type(v) == "string"
		local parse = is_colour and plain_colour or positive
		local lead = is_colour and swatch(v) or nil
		table.insert(out, field("var:" .. name, name, v, parse, { "vars", name }, lead))
	end
	return out
end

-- The fields of part `p`, or nothing for no part.
function I.view(p)
	local who = p and M.name .. ":" .. p.id
	if who ~= showing then -- drafts belong to the part they were typed for
		showing, drafts = who, {}
	end
	if not p then
		return {}
	end
	local base = { "views", M.view, "parts", p.id }
	local id = "field:" .. p.id .. ":pivot:"
	local out = { ui.text({ p.id, color = C.accent, font_size = 13, mt = 12, no_wrap = true }) }
	table.insert(out, field(id .. "x", "pivot x", p.pivot[1], number, at(base, "pivot", 1)))
	table.insert(out, field(id .. "y", "pivot y", p.pivot[2], number, at(base, "pivot", 2)))
	for i, s in ipairs(p.shapes or {}) do
		shape_fields(base, i, s, out)
	end
	return out
end

return I
