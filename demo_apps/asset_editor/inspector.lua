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

local function colour(s)
	return pcall(gfx.solid, s) and s or nil
end

-- `base` extended by more segments: a new table, so the base can be shared.
local function at(base, ...)
	local out = table.clone(base)
	for _, seg in ipairs({ ... }) do
		table.insert(out, seg)
	end
	return out
end

local function field(id, label, value, parse, path)
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
		ui.row({ w = 52, ui.text({ label, color = C.muted, font_size = 11, no_wrap = true }) }),
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
		ui.row({ w = 52, ui.text({ label, color = C.muted, font_size = 11, no_wrap = true }) }),
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
		table.insert(out, field(id .. "blur", "blur", s.shadow.blur, positive, at(sh, "blur")))
		table.insert(out, field(id .. "color", "colour", s.shadow.color, colour, at(sh, "color")))
		return
	end
	if type(s.fill) == "string" then
		table.insert(out, field(id .. "fill", "fill", s.fill, colour, at(here, "fill")))
	elseif s.fill then
		table.insert(out, note("fill", next(s.fill) .. " gradient"))
	end
	if s.stroke then
		table.insert(out, field(id .. "width", "stroke", s.stroke[1], positive, at(here, "stroke", 1)))
		table.insert(out, field(id .. "ink", "ink", s.stroke[2], colour, at(here, "stroke", 2)))
	end
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
