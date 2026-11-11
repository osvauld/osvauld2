-- Shared by the brawl tests: the rules loaded from the app's folder, inputs, stepping, a plain-data
-- check and a net-like copy. Run from the worktree root.
package.path = "demo_apps/brawl/?.lua;" .. package.path
local U = {}
U.F = require("fighter")
U.tune = require("tune")
-- The yard is the one the app plays: maps/yard.lua compiled on the asset library (brawl-maps.md).
U.map = require("map")
U.library = require("library").load()
U.compiled = U.map.compile(require("maps/yard"), U.library, { body = { w = U.tune.body_w, h = U.tune.body_h } })
U.yard = U.compiled.stage

-- `cast` (optional): each fighter's `ctl`, { "human" } by default; a dummy takes no input.
function U.rules(cast) return U.F.rules({ stage = U.yard, tune = U.tune, cast = cast }) end

function U.fresh(cast)
	local rules = U.rules(cast)
	return U.F.new(rules), rules
end

-- A plain input; presses are counts (fighter.lua acts when one grows). `heavy` is held.
function U.input(t)
	t = t or {}
	return { x = t.x or 0, y = t.y or 0, jump = t.jump or false, jump_n = t.jump_n or 0,
		down_n = t.down_n or 0, dash_n = t.dash_n or 0, restart_n = t.restart_n or 0,
		light_n = t.light_n or 0, heavy_n = t.heavy_n or 0, dodge_n = t.dodge_n or 0, heavy = t.heavy or false,
		pct_n = t.pct_n or 0, pct = t.pct or 0 }
end

-- `input` is fighter 1's, or a list of every fighter's ({ [1] = ..., [2] = ... }) when it has `[1]`.
function U.step(s, rules, input, n)
	local inputs = input and input[1] and input or { input or U.input() }
	for _ = 1, n or 1 do U.F.step(s, inputs, rules) end
end

-- Is `v` plain data by net.match's rules? Returns false and why.
function U.plain(v, seen, path)
	seen, path = seen or {}, path or "state"
	local kind = type(v)
	if kind == "number" or kind == "string" or kind == "boolean" then return true end
	if kind ~= "table" then return false, path .. " is a " .. kind end
	if getmetatable(v) ~= nil then return false, path .. " has a metatable" end
	if seen[v] then return false, path .. " is held in two places (or a cycle)" end
	seen[v] = true
	for k, x in pairs(v) do
		if type(k) ~= "number" and type(k) ~= "string" then return false, path .. " has a " .. type(k) .. " key" end
		local ok, why = U.plain(x, seen, path .. "." .. tostring(k))
		if not ok then return false, why end
	end
	return true
end

-- A copy through text, as the net would carry it: numbers by %.17g (exact for doubles), strings,
-- booleans and tables of those. Anything else is an error.
local function encode(v, out)
	local kind = type(v)
	if kind == "number" then out[#out + 1] = "n" .. string.format("%.17g", v)
	elseif kind == "string" then out[#out + 1] = "s" .. #v .. ":" .. v
	elseif kind == "boolean" then out[#out + 1] = v and "t" or "f"
	elseif kind == "table" then
		assert(getmetatable(v) == nil, "a table with a metatable")
		local keys = {}
		for k in pairs(v) do keys[#keys + 1] = k end
		table.sort(keys, function(a, b)
			if type(a) == type(b) then return a < b end
			return type(a) == "number"
		end)
		out[#out + 1] = "{" .. #keys
		for _, k in ipairs(keys) do encode(k, out); encode(v[k], out) end
	else error("cannot send a " .. kind) end
end

local function decode(parts, i)
	local p = parts[i]
	local tag = p:sub(1, 1)
	if tag == "n" then return tonumber(p:sub(2)), i + 1
	elseif tag == "s" then return p:sub(p:find(":") + 1), i + 1
	elseif tag == "t" then return true, i + 1
	elseif tag == "f" then return false, i + 1
	end
	local t, n = {}, tonumber(p:sub(2))
	i = i + 1
	for _ = 1, n do
		local k, v
		k, i = decode(parts, i)
		v, i = decode(parts, i)
		t[k] = v
	end
	return t, i
end

function U.wire(v)
	local out = {}
	encode(v, out)
	local text = table.concat(out, "\0")
	local parts = {}
	for part in (text .. "\0"):gmatch("([^\0]*)\0") do parts[#parts + 1] = part end
	return (decode(parts, 1))
end

-- Equal in every field, by ==; returns false and the first path that differs.
function U.same(a, b, path)
	path = path or "state"
	if type(a) ~= "table" or type(b) ~= "table" then
		if a == b then return true end
		return false, path .. ": " .. tostring(a) .. " vs " .. tostring(b)
	end
	for k, v in pairs(a) do
		local ok, why = U.same(v, b[k], path .. "." .. tostring(k))
		if not ok then return false, why end
	end
	for k, v in pairs(b) do
		if a[k] == nil then return false, path .. "." .. tostring(k) .. " only on the right (" .. tostring(v) .. ")" end
	end
	return true
end

function U.copy(v) return U.wire(v) end

function U.platform(rules, id)
	for i, p in ipairs(rules.stage.platforms) do if p.id == id then return p, i end end
	error("no platform " .. id)
end

return U
