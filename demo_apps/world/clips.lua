-- Animation as data. Keys are {time, value, easing?}; `in_out` shapes the segment arriving at
-- the key. Values offset the rest pose: x/y in drawing units, rot in degrees about the pivot.
return {
	-- Breathing: the body bobs, the head lags a little, the arms sway.
	idle = {
		length = 1.2, loop = true,
		tracks = {
			body = { y = { {0, 0}, {0.6, -3, "in_out"}, {1.2, 0, "in_out"} } },
			head = { rot = { {0, 0}, {0.6, 3, "in_out"}, {1.2, 0, "in_out"} } },
			arm_near = { rot = { {0, 0}, {0.6, -6, "in_out"}, {1.2, 0, "in_out"} } },
			arm_far = { rot = { {0, 0}, {0.6, 6, "in_out"}, {1.2, 0, "in_out"} } },
		},
	},
	-- Walking in place: legs and arms swing in opposition, the body dips on each step. The
	-- world moves the hero; this only has to look like walking.
	walk = {
		length = 0.5, loop = true,
		tracks = {
			leg_l = { rot = { {0, 22}, {0.25, -22, "in_out"}, {0.5, 22, "in_out"} } },
			leg_r = { rot = { {0, -22}, {0.25, 22, "in_out"}, {0.5, -22, "in_out"} } },
			arm_near = { rot = { {0, -18}, {0.25, 18, "in_out"}, {0.5, -18, "in_out"} } },
			arm_far = { rot = { {0, 18}, {0.25, -18, "in_out"}, {0.5, 18, "in_out"} } },
			body = { y = {
				{0, 0}, {0.125, -3, "in_out"}, {0.25, 0, "in_out"},
				{0.375, -3, "in_out"}, {0.5, 0, "in_out"},
			} },
		},
	},
	-- Walking in profile: legs stride forward and back about the hip, arms counter-swing.
	walk_side = {
		length = 0.5, loop = true,
		tracks = {
			leg_l = { rot = { {0, 28}, {0.25, -28, "in_out"}, {0.5, 28, "in_out"} } },
			leg_r = { rot = { {0, -28}, {0.25, 28, "in_out"}, {0.5, -28, "in_out"} } },
			arm_near = { rot = { {0, 24}, {0.25, -24, "in_out"}, {0.5, 24, "in_out"} } },
			arm_far = { rot = { {0, -24}, {0.25, 24, "in_out"}, {0.5, -24, "in_out"} } },
			body = { y = {
				{0, 0}, {0.125, -3, "in_out"}, {0.25, 0, "in_out"},
				{0.375, -3, "in_out"}, {0.5, 0, "in_out"},
			} },
		},
	},
	-- A jump, played once: the body — and everything hanging off it — rises and falls while the
	-- entity's feet stay where they are, so draw order ignores the jump. Legs tuck, arms lift.
	jump = {
		length = 0.45,
		tracks = {
			body = { y = {
				{0, 0}, {0.1, -28}, {0.225, -40, "in_out"}, {0.35, -28, "in_out"}, {0.45, 0},
			} },
			leg_l = { y = { {0, 0}, {0.225, -10, "in_out"}, {0.45, 0, "in_out"} } },
			leg_r = { y = { {0, 0}, {0.225, -10, "in_out"}, {0.45, 0, "in_out"} } },
			arm_near = { rot = { {0, 0}, {0.225, -30, "in_out"}, {0.45, 0, "in_out"} } },
			arm_far = { rot = { {0, 0}, {0.225, 30, "in_out"}, {0.45, 0, "in_out"} } },
		},
	},
	-- Holding something in front: both arms turn in toward the middle, the body breathes.
	carry = {
		length = 1.2, loop = true,
		tracks = {
			body = { y = { {0, 0}, {0.6, -2, "in_out"}, {1.2, 0, "in_out"} } },
			arm_near = { rot = { {0, 35} } },
			arm_far = { rot = { {0, -35} } },
		},
	},
	-- Walking while holding: the walk's legs and bob, the carry's arms. One clip plays at a time
	-- (no layers yet), so the two are combined here by hand.
	carry_walk = {
		length = 0.5, loop = true,
		tracks = {
			leg_l = { rot = { {0, 22}, {0.25, -22, "in_out"}, {0.5, 22, "in_out"} } },
			leg_r = { rot = { {0, -22}, {0.25, 22, "in_out"}, {0.5, -22, "in_out"} } },
			arm_near = { rot = { {0, 35} } },
			arm_far = { rot = { {0, -35} } },
			body = { y = {
				{0, 0}, {0.125, -3, "in_out"}, {0.25, 0, "in_out"},
				{0.375, -3, "in_out"}, {0.5, 0, "in_out"},
			} },
		},
	},
	-- Height is a pose, not a place: a carried chest is attached where it would stand on the floor
	-- in front of the hero, and `lift` raises its drawing into the hero's hands. Its footprint
	-- stays on the floor, so when it is let go it is already where it lands — `fall` only drops
	-- the drawing back, speeding up, with one small bounce. 66 is hands to floor.
	lift = { length = 0.2, tracks = { base = { y = { {0, 0}, {0.2, -66, "in_out"} } } } },
	fall = {
		length = 0.4,
		tracks = { base = { y = {
			{0, -66}, {0.12, -52}, {0.2, -30}, {0.26, 0}, {0.32, -10, "in_out"}, {0.4, 0, "in_out"},
		} } },
	},
	-- The lid swings back about its hinge and holds there.
	open = { length = 0.5, tracks = { lid = { rot = { {0, 0}, {0.5, -100, "in_out"} } } } },
	close = { length = 0.4, tracks = { lid = { rot = { {0, -100}, {0.4, 0, "in_out"} } } } },
}
