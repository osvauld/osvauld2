-- The `jetpack-guns` workspace asset: every gun the game can hold. Seeded once from the baked guns;
-- after that the asset is the truth. One write per finished gesture.
local C = require("theme")
local F = require("format")
local M = {}

M.asset = asset:open("jetpack-guns")
M.gun_id = "rifle"

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
M.plain = plain

if M.asset.info == nil then
	local baked = require("baked/guns")
	M.asset:set({ "guns" }, tag(baked.guns))
	M.asset:set({ "info" }, tag({ version = F.versions.guns }))
end

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end
local function round(v) return math.floor(v * 100 + 0.5) / 100 end

M.shown = nil -- the gun on screen, which every write addresses
M.notice = nil -- why the last action was refused, shown in the status

-- The raw gun (drawn even when invalid) and its validation. A chosen gun not in the mirror (just made:
-- a frame behind; or deleted) shows the first one meanwhile.
function M.current()
	local list = M.asset.guns or {}
	local shown = list[1]
	for _, g in ipairs(list) do
		if g.id == M.gun_id then shown = g end
	end
	if not shown then return nil, nil, "no guns" end
	M.shown = shown.id
	local ok, err = F.gun(shown)
	return shown, ok, err
end

local function at(...) return { "guns", M.shown, ... } end

-- A shape, the part holding it and its place there.
function M.find(gun, id)
	for _, part in ipairs(gun.parts or {}) do
		for i, s in ipairs(part.shapes or {}) do
			if s.id == id then return s, part, i end
		end
	end
end

function M.part_of(gun, id)
	for _, part in ipairs(gun.parts or {}) do
		if part.id == id then return part end
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

local function mapped(path, f)
	local out = {}
	for i, c in ipairs(path) do
		local m = { c[1] }
		for k = 2, #c, 2 do m[k], m[k + 1] = f(c[k], c[k + 1]) end
		out[i] = m
	end
	return out
end

-- Moved by whole units, kept inside the gun (a shape already wider than it moves freely).
local function inside(v, lo, hi)
	if hi < lo then return v end
	return clamp(v, lo, hi)
end
function M.moved(gun, path, dx, dy)
	local x, y, w, h = M.box(path)
	dx = inside(math.floor(dx + 0.5), -x, gun.size[1] - w - x)
	dy = inside(math.floor(dy + 0.5), -y, gun.size[2] - h - y)
	return mapped(path, function(px, py) return round(px + dx), round(py + dy) end)
end

-- Scaled from its top-left corner to a box `dw`, `dh` larger, at least a unit, inside the gun.
function M.scaled(gun, path, dw, dh)
	local x, y, w, h = M.box(path)
	local nw = clamp(math.floor(w + dw + 0.5), 1, gun.size[1] - x)
	local nh = clamp(math.floor(h + dh + 0.5), 1, gun.size[2] - y)
	local sx, sy = w > 0 and nw / w or 1, h > 0 and nh / h or 1
	return mapped(path, function(px, py) return round(x + (px - x) * sx), round(y + (py - y) * sy) end)
end

-- Point `k` (the x index) of command `j` moved to x, y.
function M.with_point(gun, path, j, k, x, y)
	local out = mapped(path, function(px, py) return px, py end)
	out[j][k] = clamp(math.floor(x + 0.5), 0, gun.size[1])
	out[j][k + 1] = clamp(math.floor(y + 0.5), 0, gun.size[2])
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

-- All five parts in draw order, the missing ones empty at `pivot`; whatever parts the gun has are kept.
local function with_parts(gun, pivot)
	local out = {}
	for _, id in ipairs(F.gun_parts) do
		local have = M.part_of(gun, id)
		out[#out + 1] = have and plain(have) or { id = id, pivot = { pivot[1], pivot[2] }, shapes = {} }
	end
	return out
end

-- A new shape on top of `part`, its corner at the click, kept inside the gun. A part the gun lacks
-- (slide, bolt, barrel) is made on the way.
function M.add_shape(gun, part, kind, x, y)
	local count = 0
	for _, p in ipairs(gun.parts) do count += #(p.shapes or {}) end
	if count >= F.gun_limits.shapes then
		M.notice = "At the limit of " .. F.gun_limits.shapes .. " shapes"
		return nil
	end
	M.notice = nil
	local side = C.new_side
	x = clamp(math.floor(x + 0.5), 0, gun.size[1] - side)
	y = clamp(math.floor(y + 0.5), 0, gun.size[2] - side)
	local id = "s:" .. string.sub(uuid(), 1, 8)
	local shape = tag({ id = id, fill = "@metal", path = M.makers[kind](x, y, side, side) })
	if M.part_of(gun, part) then
		M.asset:insert(at("parts", part, "shapes"), shape)
	else
		local parts = with_parts(gun, gun.points.grip)
		for _, p in ipairs(parts) do
			if p.id == part then p.shapes = { plain(shape) } end
		end
		M.asset:set(at("parts"), tag(parts))
	end
	return id
end

function M.set_path(part, id, path) M.asset:set(at("parts", part, "shapes", id, "path"), tag(path)) end
function M.set_fill(part, id, fill) M.asset:set(at("parts", part, "shapes", id, "fill"), fill) end
function M.delete_shape(part, id) M.asset:delete(at("parts", part, "shapes", id)) end

-- One step up (drawn later) or down within its part.
function M.restack(gun, id, step)
	local _, part, i = M.find(gun, id)
	if not part then return end
	local to = i + step
	if to < 1 or to > #part.shapes then return end
	M.asset:move(at("parts", part.id, "shapes", id), to)
end

function M.set_point(gun, name, x, y)
	M.asset:set(at("points", name), doc.list({ clamp(math.floor(x + 0.5), 0, gun.size[1]),
		clamp(math.floor(y + 0.5), 0, gun.size[2]) }))
end

-- The free hand's hold: gone makes the gun one-handed; back, it starts ahead of the grip.
function M.remove_support() M.asset:delete(at("points", "support")) end
function M.add_support(gun)
	local g = gun.points.grip
	M.set_point(gun, "support", g[1] + 22, g[2] + 2)
end

function M.set_pivot(gun, part, x, y)
	M.asset:set(at("parts", part, "pivot"), doc.list({ clamp(math.floor(x + 0.5), 0, gun.size[1]),
		clamp(math.floor(y + 0.5), 0, gun.size[2]) }))
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

-- A stat from field text: refused (false, and why in the notice) outside its range.
function M.set_stat(name, text)
	local v = tonumber(text)
	if not F.stat_ok(name, v) then
		local l = F.stat_limits[name]
		M.notice = string.format("%s must be %s%s to %s", name, l[3] and "a whole number " or "", l[1], l[2])
		return false
	end
	M.notice = nil
	M.asset:set(at("stats", name), v)
	return true
end

-- The gun's zoom levels as the game reads them: `zooms`, else a v1 `zoom` upgraded.
function M.zooms_of(gun)
	local st = type(gun.stats) == "table" and gun.stats or {}
	local out = {}
	if type(st.zooms) == "table" then
		for i, v in ipairs(st.zooms) do out[i] = v end
		return out
	end
	return type(st.zoom) == "number" and st.zoom > 1 and { 1, st.zoom } or { 1 }
end

-- Replace the zoom list (first 1, ascending, at most F.zoom_limits.count up to F.zoom_limits.max); refused otherwise.
function M.write_zooms(list)
	if not F.zooms_ok(list) then
		M.notice = string.format("zoom levels must ascend from 1 to at most %gx (up to %d levels)", F.zoom_limits.max, F.zoom_limits.count)
		return false
	end
	M.notice = nil
	M.asset:set(at("stats", "zooms"), doc.list(list))
	M.asset:delete(at("stats", "zoom")) -- a v1 gun's old stat; `zooms` wins anyway
	return true
end

-- Level `i` (2 or more; the first is always 1) from field text.
function M.set_zoom(gun, i, text)
	local v, list = tonumber(text), M.zooms_of(gun)
	if v == nil or i < 2 or i > #list then
		M.notice = "zoom level must be a number"
		return false
	end
	list[i] = v
	return M.write_zooms(list)
end

-- One more level, half a step past the last (capped at the maximum).
function M.add_zoom(gun)
	local list = M.zooms_of(gun)
	local nxt = math.min(F.zoom_limits.max, round(list[#list] + 0.5))
	if #list >= F.zoom_limits.count or nxt <= list[#list] then
		M.notice = "no room for another zoom level"
		return false
	end
	list[#list + 1] = nxt
	return M.write_zooms(list)
end

-- Drop the last level (the first, 1x, stays).
function M.remove_zoom(gun)
	local list = M.zooms_of(gun)
	if #list <= 1 then return false end
	list[#list] = nil
	return M.write_zooms(list)
end

function M.set_auto(on) M.asset:set(at("stats", "auto"), on) end

-- Bullet or rocket; a rocket needs a splash radius, a bullet has none.
function M.set_kind(kind, gun)
	M.asset:set(at("stats", "kind"), kind)
	if kind == "rocket" then
		if not (gun.stats and gun.stats.splash) then M.asset:set(at("stats", "splash"), 60) end
	else
		M.asset:delete(at("stats", "splash"))
	end
end

-- Game units per drawing unit.
function M.set_scale(text)
	local v = tonumber(text)
	local L = F.gun_limits
	if not (v and v >= L.min_scale and v <= L.max_scale) then
		M.notice = "scale must be " .. L.min_scale .. " to " .. L.max_scale
		return false
	end
	M.notice = nil
	M.asset:set(at("scale"), v)
	return true
end

local function fresh_ids(t)
	for _, part in ipairs(t.parts) do
		for _, s in ipairs(part.shapes or {}) do s.id = "s:" .. string.sub(uuid(), 1, 8) end
	end
end

-- A copy of `gun` with fresh shape ids; it becomes the one shown.
function M.copy(gun)
	if #M.asset.guns >= 32 then
		M.notice = "At the limit of 32 guns"
		return
	end
	M.notice = nil
	local t = plain(gun)
	t.id, t.name = "gun:" .. string.sub(uuid(), 1, 8), (gun.name or gun.id) .. " copy"
	fresh_ids(t)
	M.asset:insert({ "guns" }, tag(t))
	M.gun_id = t.id
end

-- A small plain gun: a body and a magazine, ready to reshape.
function M.blank()
	if #M.asset.guns >= 32 then
		M.notice = "At the limit of 32 guns"
		return
	end
	M.notice = nil
	local t = {
		id = "gun:" .. string.sub(uuid(), 1, 8), name = "New gun", size = { 64, 32 }, scale = 0.3,
		vars = { ink = "#182e34", metal = "#4a636b", wood = "#9a6a45", grip = "#6b4a38", accent = "#e99563",
			dark = "#2c4349", edge = "#729a87", gold = "#fbd484" },
		parts = {
			{ id = "mag", pivot = { 26, 26 }, shapes = { { id = "", fill = "@dark", path = M.makers.rect(22, 22, 8, 8) } } },
			{ id = "body", pivot = { 18, 18 }, shapes = {
				{ id = "", fill = "@wood", path = M.makers.rect(4, 12, 14, 8) },
				{ id = "", fill = "@metal", path = M.makers.rect(16, 10, 44, 8) },
				{ id = "", fill = "@grip", path = M.makers.rect(14, 18, 6, 10) },
			} },
		},
		points = { grip = { 17, 20 }, support = { 40, 20 }, muzzle = { 60, 13 }, eject = { 34, 11 }, mag = { 26, 22 } },
		stats = { kind = "bullet", auto = true, damage = 16, fire_interval = 0.12, mag_size = 30, melee = 25, pellets = 1,
			range = 500, recoil = 4, reload_time = 1.9, shot_speed = 1200, spread = 0.04, zoom = 1 },
	}
	fresh_ids(t)
	M.asset:insert({ "guns" }, tag(t))
	M.gun_id = t.id
end

-- The last gun stays: the game needs one to hold.
function M.delete_gun(gun)
	if #M.asset.guns <= 1 then
		M.notice = "The last gun stays: the game needs one"
		return
	end
	M.notice = nil
	M.asset:delete({ "guns", gun.id })
end

return M
