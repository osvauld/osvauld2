return {
	bg = "#070a12",
	panel = "#0c111d",
	text = "#e7eefb",
	dim = "#7e8da8",

	sprite = "#3f7fd4",
	sprite_core = "#bcdcff",
	hot = "#ffd166",
	pin = "#ff5d8f",
	grab = "#7ef0c0",
	guide = "#34496e",

	count = 200,
	field_w = 940,
	field_h = 560,
	sprite_r = 7,
	ring_gap = 3,
	ring_w = 1.6,

	-- A frame doesn't clip and the pointer only reaches what is inside the element, so every
	-- orbit is kept this far from the panel edge.
	margin = 16,
	orbit_min = 30,
	orbit_span = 148,

	fling_damp = 1.9,
	-- The shortest span a throw is measured over; below it the samples are mostly jitter.
	fling_window = 0.03,
	bounce = 0.55,
	rest = 6,
	max_fling = 1400,

	pad = 16,
	gap = 6,
	head_size = 14,
	line_size = 12,
}
