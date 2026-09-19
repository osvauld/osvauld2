local C = require("theme")
local S = require("shapes")

local M = {}

local paths = {
	upper = S.limb(C.upper_len, C.upper_w0, C.upper_w1),
	fore = S.limb(C.fore_len, C.fore_w0, C.fore_w1),
	palm = S.palm(C.hand_len, C.palm_w),
	shoulder = S.circle(0, 0, C.r_shoulder),
	elbow = S.circle(0, 0, C.r_elbow),
	wrist = S.circle(0, 0, C.r_wrist),
	anchor = S.circle(C.base_x, C.base_y, C.r_shoulder + 9),
	fingers = {
		S.finger(-C.palm_w * 0.8, -C.palm_w * 1.5),
		S.finger(0, -C.palm_w * 0.2),
		S.finger(C.palm_w * 0.8, C.palm_w * 0.9),
	},
	thumb = gfx.path({
		{ "move", C.hand_len * 0.3, C.palm_w },
		{ "quad", C.hand_len * 0.55, C.palm_w * 2.4, C.hand_len * 0.74, C.palm_w * 1.6 },
	}),
}

local brush = {
	limb = gfx.solid(C.limb),
	limb_hot = gfx.solid(C.limb_hot),
	joint = gfx.solid(C.joint),
	joint_hot = gfx.solid(C.joint_hot),
	hand = gfx.solid(C.hand),
	hand_hot = gfx.solid(C.hand_hot),
	anchor = gfx.solid(C.anchor),
	line = gfx.solid(C.line),
}

-- What a hit on each named shape means, in that shape's own frame: `origin + len` is the span
-- of its local x, so (sx - origin) / len is how far along it the pointer is.
M.spans = {
	["seg:upper"] = { label = "upper arm", origin = 0, len = C.upper_len, axis = "along its length" },
	["seg:fore"] = { label = "forearm", origin = 0, len = C.fore_len, axis = "along its length" },
	["seg:hand"] = { label = "hand", origin = 0, len = C.hand_len, axis = "along its length" },
	["joint:shoulder"] = {
		label = "shoulder joint",
		origin = -C.r_shoulder,
		len = C.r_shoulder * 2,
		axis = "across the joint",
	},
	["joint:elbow"] = { label = "elbow joint", origin = -C.r_elbow, len = C.r_elbow * 2, axis = "across the joint" },
	["joint:wrist"] = { label = "wrist joint", origin = -C.r_wrist, len = C.r_wrist * 2, axis = "across the joint" },
}

-- Grabbing a shape swings the joint it hangs from, about a pivot given in that shape's own
-- coordinates: every shape is drawn at its group's origin, except the hand, which is pushed
-- `hand_gap` past the wrist it turns around.
M.grips = {
	["seg:upper"] = { joint = "shoulder", pivot = { 0, 0 } },
	["joint:shoulder"] = { joint = "shoulder", pivot = { 0, 0 } },
	["seg:fore"] = { joint = "elbow", pivot = { 0, 0 } },
	["joint:elbow"] = { joint = "elbow", pivot = { 0, 0 } },
	["joint:wrist"] = { joint = "wrist", pivot = { 0, 0 } },
	["seg:hand"] = { joint = "wrist", pivot = { -C.hand_gap, 0 } },
}

M.depth = {
	["seg:upper"] = 1,
	["joint:shoulder"] = 1,
	["seg:fore"] = 2,
	["joint:elbow"] = 2,
	["joint:wrist"] = 3,
	["seg:hand"] = 4,
}

local function place(tx, ty, deg)
	local r = math.rad(deg)
	local c, s = math.cos(r), math.sin(r)
	return { c, s, -s, c, tx, ty }
end

local function pick(hot, id, cold, warm)
	return hot == id and warm or cold
end

-- The groups carry transforms and nothing else: naming one would swallow every id beneath it,
-- and the whole point here is that the wrist answers for itself three rotations down.
function M.build(a, hot)
	local hand = gfx.group({
		id = "seg:hand",
		transform = place(C.hand_gap, 0, 0),
		gfx.fill({ path = paths.palm, brush = pick(hot, "seg:hand", brush.hand, brush.hand_hot) }),
		gfx.stroke({
			path = paths.fingers[1],
			brush = pick(hot, "seg:hand", brush.hand, brush.hand_hot),
			width = 4,
			cap = "round",
		}),
		gfx.stroke({
			path = paths.fingers[2],
			brush = pick(hot, "seg:hand", brush.hand, brush.hand_hot),
			width = 4,
			cap = "round",
		}),
		gfx.stroke({
			path = paths.fingers[3],
			brush = pick(hot, "seg:hand", brush.hand, brush.hand_hot),
			width = 4,
			cap = "round",
		}),
		gfx.stroke({
			path = paths.thumb,
			brush = pick(hot, "seg:hand", brush.hand, brush.hand_hot),
			width = 5,
			cap = "round",
		}),
	})

	local wrist = gfx.group({
		transform = place(C.fore_len, 0, a.wrist),
		hand,
		gfx.fill({
			path = paths.wrist,
			brush = pick(hot, "joint:wrist", brush.joint, brush.joint_hot),
			id = "joint:wrist",
		}),
	})

	local fore = gfx.group({
		transform = place(C.upper_len, 0, a.elbow),
		gfx.fill({
			path = paths.fore,
			brush = pick(hot, "seg:fore", brush.limb, brush.limb_hot),
			id = "seg:fore",
		}),
		wrist,
		gfx.fill({
			path = paths.elbow,
			brush = pick(hot, "joint:elbow", brush.joint, brush.joint_hot),
			id = "joint:elbow",
		}),
	})

	local upper = gfx.group({
		transform = place(C.base_x, C.base_y, a.shoulder),
		gfx.fill({
			path = paths.upper,
			brush = pick(hot, "seg:upper", brush.limb, brush.limb_hot),
			id = "seg:upper",
		}),
		fore,
		gfx.fill({
			path = paths.shoulder,
			brush = pick(hot, "joint:shoulder", brush.joint, brush.joint_hot),
			id = "joint:shoulder",
		}),
	})

	return gfx.frame({
		width = C.frame_w,
		height = C.frame_h,
		gfx.fill({ path = paths.anchor, brush = brush.anchor }),
		upper,
	})
end

return M
