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
	-- The lid swings back about its hinge and holds there.
	open = { length = 0.5, tracks = { lid = { rot = { {0, 0}, {0.5, -100, "in_out"} } } } },
	close = { length = 0.4, tracks = { lid = { rot = { {0, -100}, {0.4, 0, "in_out"} } } } },
}
