-- The `jetpack-pilots` workspace asset: every pilot the game can fly. Seeded once from the baked
-- pilots; after that the asset is the truth. One write per finished gesture.
local C = require("theme")
local F = require("format")
local M = {}

M.asset = asset:open("jetpack-pilots")
M.pilot_id = "scout"

-- Doc writes are tagged at every depth: positional-only tables are lists, the rest maps.
local function tag(v)
	if type(v) ~= "table" then return v end
	local out, n = {}, 0
	for k, x in pairs(v) do
		out[k] = tag(x)
		n += 1
	end
	return n == #v and doc.list(out) or doc.map(out) -- an empty table is an empty list
end
M.tag = tag

local function plain(v)
	if type(v) ~= "table" then return v end
	local out = {}
	for k, x in pairs(v) do out[k] = plain(x) end
	return out
end

-- Seeded from the baked pilots; an asset still at version 1 (three parts, or five with arms) is
-- upgraded to the rig once, in place, and written back.
if M.asset.info == nil then
	local baked = require("baked/pilots")
	M.asset:set({ "pilots" }, tag(baked.pilots))
	M.asset:set({ "info" }, tag({ version = F.versions.pilots }))
elseif M.asset.info.version == 1 then
	local upgraded = {}
	for i, p in ipairs(M.asset.pilots or {}) do upgraded[i] = F.upgrade(plain(p)) end
	M.asset:set({ "pilots" }, tag(upgraded))
	M.asset:set({ "info" }, tag({ version = F.versions.pilots }))
end

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end
local function round(v) return math.floor(v * 100 + 0.5) / 100 end

M.shown = nil -- the pilot on screen, which every write addresses

-- The raw pilot (drawn even when invalid) and its validation. A chosen pilot not in the mirror
-- (just made: a frame behind; or deleted) shows the first one meanwhile.
function M.current()
	local list = M.asset.pilots or {}
	local shown = list[1]
	for _, p in ipairs(list) do
		if p.id == M.pilot_id then shown = p end
	end
	if not shown then return nil, nil, "no pilots" end
	M.shown = shown.id
	local ok, err = F.pilot(shown)
	return shown, ok, err
end

local function at(...) return { "pilots", M.shown, ... } end

-- A shape and the part holding it.
function M.find(pilot, id)
	for _, part in ipairs(pilot.parts or {}) do
		for i, s in ipairs(part.shapes or {}) do
			if s.id == id then return s, part, i end
		end
	end
end

-- x, y, w, h of every coordinate in a path (controls included: ours sit on the outline's box).
function M.box(path)
	local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
	for _, c in ipairs(path) do
		for k = 2, #c, 2 do
			x0, x1 = math.min(x0, c[k]), math.max(x1, c[k])
			y0, y1 = math.min(y0, c[k + 1]), math.max(y1, c[k + 1])
		end
	end
	if x0 == math.huge then return 0, 0, 0, 0 end
	return x0, y0, x1 - x0, y1 - y0
end

-- A copy of `path` with each point mapped by `f(x, y)`.
local function mapped(path, f)
	local out = {}
	for i, c in ipairs(path) do
		local m = { c[1] }
		for k = 2, #c, 2 do m[k], m[k + 1] = f(c[k], c[k + 1]) end
		out[i] = m
	end
	return out
end

-- Moved by whole units, kept inside the pilot (a shape already wider than it moves freely).
local function inside(v, lo, hi)
	if hi < lo then return v end
	return clamp(v, lo, hi)
end
function M.moved(pilot, path, dx, dy)
	local x, y, w, h = M.box(path)
	dx = inside(math.floor(dx + 0.5), -x, pilot.size[1] - w - x)
	dy = inside(math.floor(dy + 0.5), -y, pilot.size[2] - h - y)
	return mapped(path, function(px, py) return round(px + dx), round(py + dy) end)
end

-- Scaled from its top-left corner to a box `dw`, `dh` larger, at least a unit, inside the pilot.
function M.scaled(pilot, path, dw, dh)
	local x, y, w, h = M.box(path)
	local nw = clamp(math.floor(w + dw + 0.5), 1, pilot.size[1] - x)
	local nh = clamp(math.floor(h + dh + 0.5), 1, pilot.size[2] - y)
	local sx, sy = w > 0 and nw / w or 1, h > 0 and nh / h or 1
	return mapped(path, function(px, py) return round(x + (px - x) * sx), round(y + (py - y) * sy) end)
end

-- Point `k` (the x index) of command `j` moved to x, y.
function M.with_point(pilot, path, j, k, x, y)
	local out = mapped(path, function(px, py) return px, py end)
	out[j][k] = clamp(math.floor(x + 0.5), 0, pilot.size[1])
	out[j][k + 1] = clamp(math.floor(y + 0.5), 0, pilot.size[2])
	return out
end

local K = 0.5523
M.makers = {
	rect = function(x, y, w, h)
		return { { "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } }
	end,
	ellipse = function(x, y, w, h)
		local rx, ry = w / 2, h / 2
		local cx, cy = x + rx, y + ry
		return { { "move", cx + rx, cy },
			{ "cubic", cx + rx, cy + ry * K, cx + rx * K, cy + ry, cx, cy + ry },
			{ "cubic", cx - rx * K, cy + ry, cx - rx, cy + ry * K, cx - rx, cy },
			{ "cubic", cx - rx, cy - ry * K, cx - rx * K, cy - ry, cx, cy - ry },
			{ "cubic", cx + rx * K, cy - ry, cx + rx, cy - ry * K, cx + rx, cy }, { "close" } }
	end,
	triangle = function(x, y, w, h)
		return { { "move", x + w / 2, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } }
	end,
}

M.notice = nil -- why the last add was refused, shown in the status

-- A new shape on top of `part`, its corner at the click, kept inside the pilot.
function M.add_shape(pilot, part, kind, x, y)
	local count = 0
	for _, p in ipairs(pilot.parts) do count += #(p.shapes or {}) end
	if count >= F.pilot_limits.shapes then
		M.notice = "At the limit of " .. F.pilot_limits.shapes .. " shapes"
		return nil
	end
	M.notice = nil
	local side = C.new_side
	x = clamp(math.floor(x + 0.5), 0, pilot.size[1] - side)
	y = clamp(math.floor(y + 0.5), 0, pilot.size[2] - side)
	local id = "s:" .. string.sub(uuid(), 1, 8)
	M.asset:insert(at("parts", part, "shapes"), tag({ id = id, fill = "@main", path = M.makers[kind](x, y, side, side) }))
	return id
end

function M.set_path(part, id, path) M.asset:set(at("parts", part, "shapes", id, "path"), tag(path)) end
function M.set_fill(part, id, fill) M.asset:set(at("parts", part, "shapes", id, "fill"), fill) end
function M.delete_shape(part, id) M.asset:delete(at("parts", part, "shapes", id)) end

-- One step up (drawn later) or down within its part.
function M.restack(pilot, id, step)
	local _, part, i = M.find(pilot, id)
	if not part then return end
	local to = i + step
	if to < 1 or to > #part.shapes then return end
	M.asset:move(at("parts", part.id, "shapes", id), to)
end

function M.set_point(pilot, name, x, y)
	M.asset:set(at("points", name), doc.list({ clamp(math.floor(x + 0.5), 0, pilot.size[1]),
		clamp(math.floor(y + 0.5), 0, pilot.size[2]) }))
end

function M.set_pivot(pilot, part, x, y)
	M.asset:set(at("parts", part, "pivot"), doc.list({ clamp(math.floor(x + 0.5), 0, pilot.size[1]),
		clamp(math.floor(y + 0.5), 0, pilot.size[2]) }))
end

-- A var takes a #hex colour; anything else is refused (the field shows it red).
function M.set_var(name, value)
	if not F.hex(value) then return false end
	M.asset:set(at("vars", name), value)
	return true
end

function M.rename(name)
	if type(name) ~= "string" or name == "" then return false end
	M.asset:set(at("name"), name)
	return true
end

-- A new pilot, a copy of `pilot` with fresh shape ids; it becomes the one shown.
function M.copy(pilot)
	if #M.asset.pilots >= 32 then
		M.notice = "At the limit of 32 pilots"
		return
	end
	local id = "pilot:" .. string.sub(uuid(), 1, 8)
	local t = plain(pilot)
	t.id, t.name = id, (pilot.name or pilot.id) .. " copy"
	for _, part in ipairs(t.parts) do
		for _, s in ipairs(part.shapes or {}) do s.id = "s:" .. string.sub(uuid(), 1, 8) end
	end
	M.asset:insert({ "pilots" }, tag(t))
	M.pilot_id = id
end

-- The last pilot stays: the game needs one to fly.
function M.delete_pilot(pilot)
	if #M.asset.pilots <= 1 then
		M.notice = "The last pilot stays: the game needs one"
		return
	end
	M.notice = nil
	M.asset:delete({ "pilots", pilot.id })
end

return M
