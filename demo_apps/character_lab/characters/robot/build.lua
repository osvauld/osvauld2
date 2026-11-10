-- A generic robot on Paka's skeleton (docs/design/2d-characters.md step 5). `build(variant)`
-- returns a drawing module: Paka's parts, ids, parents, pivots and order, read from its rig, with
-- vector shapes on the slot parts, so Paka's clip files play unchanged.
--
-- Pieces are drawn in the setup pose (the character standing, as on screen) and mapped back into
-- rest space through each slot's setup transform: the rest pose has every bone unrotated, so a
-- head drawn there would lie on its side.
local paka = require("characters/paka/rig")
local setup = require("characters/paka/setup")

local K = 0.5523 -- cubic handle for a quarter circle

local function mul(m, n)
	return {
		m[1] * n[1] + m[3] * n[2], m[2] * n[1] + m[4] * n[2],
		m[1] * n[3] + m[3] * n[4], m[2] * n[3] + m[4] * n[4],
		m[1] * n[5] + m[3] * n[6] + m[5], m[2] * n[5] + m[4] * n[6] + m[6],
	}
end

local function inverse(m)
	local det = m[1] * m[4] - m[2] * m[3]
	local a, b, c, d = m[4] / det, -m[2] / det, -m[3] / det, m[1] / det
	return { a, b, c, d, -(a * m[5] + c * m[6]), -(b * m[5] + d * m[6]) }
end

-- Each part's setup transform, composed as the engine composes a pose.
local setup_world = {}
do
	local byid = {}
	for _, p in ipairs(paka.parts) do byid[p.id] = p end
	local function first(track, key, default)
		local keys = track and track[key]
		return keys and keys[1][2] or default
	end
	local function world(id)
		if setup_world[id] then return setup_world[id] end
		local p, tr = byid[id], setup.tracks[id]
		local px, py = p.pivot[1], p.pivot[2]
		local r = math.rad(first(tr, "rot", 0))
		local s = first(tr, "scale", 1)
		local sx, sy = s * first(tr, "scale_x", 1), s * first(tr, "scale_y", 1)
		local c, n = math.cos(r), math.sin(r)
		local m = mul(mul({ 1, 0, 0, 1, first(tr, "x", 0) + px, first(tr, "y", 0) + py },
			{ c * sx, n * sx, -n * sy, c * sy, 0, 0 }), { 1, 0, 0, 1, -px, -py })
		if p.parent then m = mul(world(p.parent), m) end
		setup_world[id] = m
		return m
	end
	for _, p in ipairs(paka.parts) do world(p.id) end
end

-- Colours ------------------------------------------------------------------------------------

local function rgb(c)
	return tonumber(c:sub(2, 3), 16), tonumber(c:sub(4, 5), 16), tonumber(c:sub(6, 7), 16)
end

local function mix(a, b, t)
	local ar, ag, ab = rgb(a)
	local br, bg, bb = rgb(b)
	local function ch(x, y) return math.floor(x + (y - x) * t + 0.5) end
	return string.format("#%02x%02x%02x", ch(ar, br), ch(ag, bg), ch(ab, bb))
end

local function alpha(c, a)
	return c .. string.format("%02x", math.floor(a * 255 + 0.5))
end

-- Base, shadow, highlight.
local function material(c, ink, far)
	if far then c = mix(c, ink, 0.16) end
	return { base = c, dark = mix(c, ink, 0.32), light = mix(c, "#ffffff", 0.5) }
end

-- Paths, in setup space -----------------------------------------------------------------------

local function ellipse(cx, cy, rx, ry)
	ry = ry or rx
	return {
		{ "move", cx + rx, cy },
		{ "cubic", cx + rx, cy + K * ry, cx + K * rx, cy + ry, cx, cy + ry },
		{ "cubic", cx - K * rx, cy + ry, cx - rx, cy + K * ry, cx - rx, cy },
		{ "cubic", cx - rx, cy - K * ry, cx - K * rx, cy - ry, cx, cy - ry },
		{ "cubic", cx + K * rx, cy - ry, cx + rx, cy - K * ry, cx + rx, cy },
		{ "close" },
	}
end

-- `r` is one radius or { top_left, top_right, bottom_right, bottom_left }.
local function rrect(x, y, w, h, r)
	local tl, tr, br, bl
	if type(r) == "table" then tl, tr, br, bl = r[1], r[2], r[3], r[4] else tl, tr, br, bl = r, r, r, r end
	local cap = math.min(w, h) / 2
	tl, tr, br, bl = math.min(tl, cap), math.min(tr, cap), math.min(br, cap), math.min(bl, cap)
	return {
		{ "move", x + tl, y },
		{ "line", x + w - tr, y },
		{ "cubic", x + w - tr + K * tr, y, x + w, y + tr - K * tr, x + w, y + tr },
		{ "line", x + w, y + h - br },
		{ "cubic", x + w, y + h - br + K * br, x + w - br + K * br, y + h, x + w - br, y + h },
		{ "line", x + bl, y + h },
		{ "cubic", x + bl - K * bl, y + h, x, y + h - bl + K * bl, x, y + h - bl },
		{ "line", x, y + tl },
		{ "cubic", x, y + tl - K * tl, x + tl - K * tl, y, x + tl, y },
		{ "close" },
	}
end

-- A rod from a (radius ra) to b (radius rb), round at both ends.
local function capsule(ax, ay, bx, by, ra, rb)
	rb = rb or ra
	local dx, dy = bx - ax, by - ay
	local len = math.sqrt(dx * dx + dy * dy)
	local ux, uy = dx / len, dy / len
	local nx, ny = -uy, ux
	local function quarter(cx, cy, r, s1x, s1y, s2x, s2y)
		return { "cubic", cx + (s1x + K * s2x) * r, cy + (s1y + K * s2y) * r,
			cx + (s2x + K * s1x) * r, cy + (s2y + K * s1y) * r, cx + s2x * r, cy + s2y * r }
	end
	return {
		{ "move", ax + nx * ra, ay + ny * ra },
		{ "line", bx + nx * rb, by + ny * rb },
		quarter(bx, by, rb, nx, ny, ux, uy), quarter(bx, by, rb, ux, uy, -nx, -ny),
		{ "line", ax - nx * ra, ay - ny * ra },
		quarter(ax, ay, ra, -nx, -ny, -ux, -uy), quarter(ax, ay, ra, -ux, -uy, nx, ny),
		{ "close" },
	}
end

local function poly(...)
	local pts = { ... }
	local out = { { "move", pts[1], pts[2] } }
	for i = 3, #pts, 2 do table.insert(out, { "line", pts[i], pts[i + 1] }) end
	table.insert(out, { "close" })
	return out
end

-- Shaded solids: shadow-coloured whole, the lit part pulled toward the light (top left), a
-- highlight, the outline last so the lit part never thins it.
local function solid(ink, line, m, whole, lit, shine)
	local out = { { path = whole, fill = m.dark }, { path = lit, fill = m.base } }
	if shine then table.insert(out, { path = shine, fill = m.light }) end
	table.insert(out, { path = whole, stroke = { line, ink } })
	return out
end

-- The robot -------------------------------------------------------------------------------------

local function pieces(v)
	local ink = v.ink
	local plate, plate_far = material(v.body, ink), material(v.body, ink, true)
	local trim, trim_far = material(v.trim, ink), material(v.trim, ink, true)
	local glow = { base = v.accent, dark = mix(v.accent, ink, 0.35), light = mix(v.accent, "#ffffff", 0.7) }

	local function ball(m, cx, cy, r, line)
		return solid(ink, line or 2, m, ellipse(cx, cy, r),
			ellipse(cx - 0.2 * r, cy - 0.2 * r, 0.7 * r),
			ellipse(cx - 0.38 * r, cy - 0.42 * r, 0.24 * r, 0.15 * r))
	end
	local function box(m, x, y, w, h, r, k, line)
		local hr = type(r) == "table" and r[1] or r
		return solid(ink, line or 2, m, rrect(x, y, w, h, r), rrect(x, y, w - k, h - k, r),
			rrect(x + 0.7 * hr + 2, y + 2.5, (w - k) * 0.38, 3, 1.5))
	end
	local function limb(m, ax, ay, bx, by, ra, rb, line)
		rb = rb or ra
		local dx, dy = bx - ax, by - ay
		local len = math.sqrt(dx * dx + dy * dy)
		local nx, ny = -dy / len, dx / len
		if nx + ny > 0 then nx, ny = -nx, -ny end -- the side facing the light
		local s = 0.22 * math.min(ra, rb)
		local function at(t, off) return ax + dx * t + nx * off, ay + dy * t + ny * off end
		local hx1, hy1 = at(0.2, 0.45 * ra)
		local hx2, hy2 = at(0.7, 0.45 * rb)
		return solid(ink, line or 2, m, capsule(ax, ay, bx, by, ra, rb),
			capsule(ax - 0.7 * s, ay - 0.7 * s, bx - 0.7 * s, by - 0.7 * s, 0.7 * ra, 0.7 * rb),
			capsule(hx1, hy1, hx2, hy2, 0.16 * ra, 0.16 * rb))
	end
	local function lens(cx, cy, rx, ry)
		return { path = ellipse(cx, cy, rx, ry), fill = { radial = {
			center = { cx - 0.25 * rx, cy - 0.3 * ry }, radius = 1.25 * math.max(rx, ry),
			stops = { { 0, glow.light }, { 0.45, glow.base }, { 1, glow.dark } },
		} }, stroke = { 1.2, ink } }
	end
	local function cat(...)
		local out = {}
		for _, list in ipairs({ ... }) do
			for _, s in ipairs(list) do table.insert(out, s) end
		end
		return out
	end

	local P = {}

	-- Head: a rounded helmet, a dark visor, sockets for whatever grows out of the top.
	local visor = rrect(152, 57, 92, 50, 20)
	P["slot:head_main"] = cat(
		box(plate, 140, 30, 108, 88, 28, 7),
		{
			{ path = rrect(143, 72, 6, 22, 3), fill = plate.dark },
			{ path = visor, fill = v.visor, stroke = { 2, ink } },
			{ path = poly(198, 59, 214, 59, 192, 105, 176, 105), fill = alpha("#ffffff", 0.07) },
			{ path = poly(219, 59, 225, 59, 203, 105, 197, 105), fill = alpha("#ffffff", 0.05) },
		},
		v.head == "antenna" and cat(ball(trim, 154.5, 33.5, 4.5, 1.6), ball(trim, 234.5, 34, 4.5, 1.6)) or {}
	)
	-- Eyes: a lens in a housing; its glow and core are the eye white and pupil, so the blink
	-- clip's shutters and the look-around still read.
	P["slot:dark_eye_front"] = {
		{ path = ellipse(178, 81, 17), fill = trim.base, stroke = { 1.6, ink } },
		{ path = ellipse(178, 81, 14), fill = mix(v.visor, ink, 0.4) },
	}
	P["slot:eye_white_front"] = {
		{ path = ellipse(178, 81, 14.5), fill = alpha(v.accent, 0.3) },
		lens(178, 81, 12, 12),
	}
	P["slot:eye_front"] = {
		{ path = ellipse(180.5, 81.5, 5), fill = glow.light },
		{ path = ellipse(174, 76, 2.4), fill = "#ffffff" },
	}
	P["slot:dark_eye_back"] = {
		{ path = ellipse(231, 80, 11, 15), fill = trim.base, stroke = { 1.6, ink } },
		{ path = ellipse(231.5, 80, 8.5, 12.5), fill = mix(v.visor, ink, 0.4) },
	}
	P["slot:eye_white_back"] = {
		{ path = ellipse(231.5, 80, 9, 13), fill = alpha(v.accent, 0.3) },
		lens(231.5, 80, 7.5, 11),
	}
	P["slot:eye_back"] = {
		{ path = ellipse(233.5, 80.5, 3.4, 4.6), fill = glow.light },
		{ path = ellipse(229, 75.5, 1.6, 2), fill = "#ffffff" },
	}
	P["slot:blink_front"] = {
		{ path = ellipse(178, 81, 14), fill = trim.dark },
		{ path = { { "move", 166, 82 }, { "quad", 178, 88, 190, 82 } }, stroke = { 2, glow.base } },
	}
	P["slot:blink_back"] = {
		{ path = ellipse(231.5, 80, 8.5, 12.5), fill = trim.dark },
		{ path = { { "move", 225, 82 }, { "quad", 231.5, 86, 238, 82 } }, stroke = { 1.6, glow.base } },
	}
	P["slot:head_smile"] = {
		{ path = { { "move", 205, 97.5 }, { "quad", 212.5, 103, 220, 97.5 } },
			stroke = { 2.4, glow.base, cap = "round" } },
	}

	-- What grows out of the top.
	if v.head == "antenna" then
		P["slot:head_ear_front"] = cat(limb(trim, 160.7, 50, 146.5, 14, 1.9, 1.9, 1.4),
			ball(glow, 145.5, 11.5, 5, 1.6))
		P["slot:head_ear_back"] = cat(limb(trim_far, 229.7, 45.5, 242.5, 14, 1.9, 1.9, 1.4),
			ball(glow, 243.5, 11.5, 4.5, 1.6))
	elseif v.head == "fins" then
		local function fin(m, bx, by, tx, ty, w)
			local dx, dy = tx - bx, ty - by
			local len = math.sqrt(dx * dx + dy * dy)
			local nx, ny = -dy / len * w, dx / len * w
			local whole = { { "move", bx + nx, by + ny }, { "quad", tx + nx * 0.3, ty + ny * 0.3, tx, ty },
				{ "quad", tx - nx * 0.6, ty - ny * 0.6, bx - nx, by - ny }, { "close" } }
			return {
				{ path = whole, fill = m.base },
				{ path = { { "move", bx - nx * 0.35, by - ny * 0.35 }, { "quad", tx - nx * 0.35, ty - ny * 0.35, tx, ty },
					{ "quad", tx - nx * 0.6, ty - ny * 0.6, bx - nx, by - ny }, { "close" } }, fill = m.dark },
				{ path = whole, stroke = { 1.8, ink } },
			}
		end
		-- Swept back (the robot faces right), so they read as fins, not horns.
		P["slot:head_ear_front"] = fin(trim, 157, 44, 136, 24, 5.5)
		P["slot:head_ear_back"] = fin(trim_far, 234, 40, 226, 15, 4.5)
		local crest = { { "move", 174, 38 }, { "line", 222, 38 }, { "cubic", 214, 20, 196, 9, 168, 5 },
			{ "cubic", 175, 15, 177, 27, 174, 38 }, { "close" } }
		P["slot:head_ciolca"] = {
			{ path = crest, fill = trim.dark },
			{ path = { { "move", 174, 38 }, { "line", 212, 38 }, { "cubic", 206, 22, 192, 12, 168, 5 },
				{ "cubic", 175, 15, 177, 27, 174, 38 }, { "close" } }, fill = trim.base },
			{ path = { { "move", 213, 33 }, { "cubic", 206, 22, 193, 14, 177, 10 } },
				stroke = { 2.2, glow.base, cap = "round" } },
			{ path = crest, stroke = { 2, ink } },
		}
	elseif v.head == "dome" then
		P["slot:head_main"] = cat(P["slot:head_main"],
			{ { path = ellipse(196, 20, 20, 15), fill = alpha(v.accent, 0.22) } },
			box(trim, 182, 25, 30, 9, 4, 3, 1.8),
			{
				{ path = { { "move", 185, 26 }, { "cubic", 185, 9, 209, 9, 209, 26 }, { "close" } },
					fill = { radial = { center = { 192, 15 }, radius = 16,
						stops = { { 0, glow.light }, { 0.5, glow.base }, { 1, glow.dark } } } },
					stroke = { 1.8, ink } },
				{ path = ellipse(191, 16, 3, 2), fill = "#ffffff" },
			})
	end

	P["slot:head_neck"] = cat(box(trim, 184, 104, 30, 28, 6, 4),
		{ { path = { { "move", 186, 114 }, { "line", 212, 114 }, { "move", 186, 121 }, { "line", 212, 121 } },
			stroke = { 1.4, alpha(ink, 0.55) } } })
	P["slot:amulet"] = box(trim, 171, 119, 62, 15, 7.5, 4)

	-- Chassis: a rounded barrel, a seam for the turn of its front, a belt, a chest light.
	P["slot:body"] = cat(
		box(plate, 160, 125, 78, 80, 24, 9),
		{
			{ path = { { "move", 190, 131 }, { "quad", 183, 156, 190, 181 } }, stroke = { 1.5, alpha(ink, 0.35) } },
			{ path = rrect(167, 148, 11, 3.5, 1.75), fill = plate.dark },
			{ path = rrect(167, 154, 11, 3.5, 1.75), fill = plate.dark },
			{ path = rrect(167, 160, 11, 3.5, 1.75), fill = plate.dark },
		},
		box(trim, 164, 182, 71, 11, 5.5, 3, 1.6),
		{ { path = rrect(203, 184, 14, 6, 2), fill = glow.dark } },
		ball(trim, 212, 155, 10.5),
		{ lens(212, 155, 6.5, 6.5), { path = ellipse(210, 153, 1.8), fill = "#ffffff" } }
	)

	-- Legs: dark pistons into chunky boots.
	local function boot(m, x)
		return cat(box(m, x, 220, 39, 19, { 9, 13, 4, 4 }, 5),
			{ { path = rrect(x - 1, 235, 41, 6, 3), fill = trim.dark, stroke = { 1.6, ink } } })
	end
	P["slot:leg_front"] = limb(trim, 180, 194, 178, 226, 9.5, 9)
	P["slot:hoof_front"] = boot(plate, 159)
	P["slot:leg_back"] = limb(trim_far, 219, 194, 218.5, 226, 9.5, 9)
	P["slot:hoof-back"] = boot(plate_far, 199.5)

	-- Arms: a pauldron on a dark upper arm, an elbow ball, an armoured forearm, a gripper.
	local function hand(m, cx, cy)
		return cat(
			limb(trim_far, cx - 4, cy + 4, cx - 5, cy + 12, 3, 2.6, 1.5),
			limb(trim_far, cx + 3, cy + 4, cx + 5, cy + 12, 3, 2.6, 1.5),
			box(m, cx - 9.5, cy - 8, 19, 15, 6, 4, 1.8)
		)
	end
	P["slot:shoulder_front"] = cat(
		limb(trim, 169, 122, 157.7, 154.3, 6.5, 6.5),
		ball(trim, 157.7, 154.3, 6.5, 1.6),
		ball(plate, 166, 128, 15)
	)
	P["slot:arm_front"] = limb(plate, 157.7, 154.3, 150.5, 185, 8.5, 7.5)
	P["slot:palm_front"] = hand(trim, 149.5, 192)
	P["slot:shoulder_back"] = cat(
		limb(trim_far, 228.3, 128.2, 240.2, 152, 6, 6),
		ball(trim_far, 240.2, 152, 6, 1.6),
		ball(plate_far, 231, 131, 13)
	)
	P["slot:arm_back"] = limb(plate_far, 240.2, 152, 251.5, 182, 8, 7)
	P["slot:palm_back"] = hand(trim_far, 252.5, 189)

	-- The weapon: an energy blade in hand, its hilt in a sheath on the back.
	local function along(sx, sy, deg)
		local r = math.rad(deg)
		local ux, uy = math.cos(r), math.sin(r)
		return function(t, n) n = n or 0 return sx + ux * t - uy * n, sy + uy * t + ux * n end
	end
	local function hilt(q)
		local p1x, p1y = q(-11)
		local g1x, g1y = q(-9)
		local g2x, g2y = q(15)
		local e1x, e1y = q(18, 11)
		local e2x, e2y = q(18, -11)
		return cat(
			ball(trim, p1x, p1y, 4.5, 1.6),
			limb(trim, g1x, g1y, g2x, g2y, 4, 4, 1.6),
			limb(plate, e1x, e1y, e2x, e2y, 4, 4, 1.6)
		)
	end
	local back = along(124, 76, 59.47)
	local s1x, s1y = back(20)
	local s2x, s2y = back(135)
	local band1x, band1y = back(28, 0)
	P["slot:sword"] = cat(
		limb(trim_far, s1x, s1y, s2x, s2y, 6.5, 5),
		{ { path = capsule(band1x, band1y, s2x, s2y, 1.4), fill = alpha(v.accent, 0.6) } },
		hilt(back)
	)
	local hand_q = along(253.2, 192.4, -21.3)
	local function blade(w0, w1, t0, t1, tip)
		local ax, ay = hand_q(t0, w0)
		local bx, by = hand_q(t1, w1)
		local cx, cy = hand_q(tip)
		local dx, dy = hand_q(t1, -w1)
		local ex, ey = hand_q(t0, -w0)
		return poly(ax, ay, bx, by, cx, cy, dx, dy, ex, ey)
	end
	local g0x, g0y = hand_q(22)
	local g1x, g1y = hand_q(148)
	P["slot:sword_forward"] = cat(
		{
			{ path = capsule(g0x, g0y, g1x, g1y, 14, 8), fill = alpha(v.accent, 0.3) },
			{ path = blade(9, 7.5, 21, 130, 153), fill = glow.base },
			{ path = blade(4, 3, 22, 132, 147), fill = glow.light },
		},
		hilt(hand_q)
	)

	return P
end

local defaults = { ink = "#1d2231", visor = "#1b2232", head = "antenna" }

return function(variant)
	local v = {}
	for k, x in pairs(defaults) do v[k] = x end
	for k, x in pairs(variant) do v[k] = x end
	local P = pieces(v)
	local function place(m, shapes)
		local function pt(x, y) return m[1] * x + m[3] * y + m[5], m[2] * x + m[4] * y + m[6] end
		local out = {}
		for _, s in ipairs(shapes) do
			local path = {}
			for _, cmd in ipairs(s.path) do
				local c = { cmd[1] }
				for i = 2, #cmd, 2 do c[i], c[i + 1] = pt(cmd[i], cmd[i + 1]) end
				table.insert(path, c)
			end
			local fill = s.fill
			if type(fill) == "table" then
				local g = fill.radial
				local cx, cy = pt(g.center[1], g.center[2])
				fill = { radial = { center = { cx, cy }, radius = g.radius, stops = g.stops } }
			end
			table.insert(out, { path = path, fill = fill, stroke = s.stroke })
		end
		return out
	end
	local parts = {}
	for _, p in ipairs(paka.parts) do
		local part = { id = p.id, parent = p.parent, pivot = p.pivot }
		if P[p.id] then part.shapes = place(inverse(setup_world[p.id]), P[p.id]) end
		table.insert(parts, part)
	end
	return { size = paka.size, parts = parts }
end
