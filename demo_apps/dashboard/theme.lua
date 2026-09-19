-- palette + geometry; depends on nothing
return {
	bg = "#0d1117",
	panel = "#161b22",
	tile = "#1c2128",
	line = "#30363d",
	grid = "#242b33",
	text = "#e6edf3",
	dim = "#8b949e",
	accent = "#58a6ff",
	cursor = "#8b949e",
	band = "#1f2f47", -- opaque stand-in: gfx.solid is documented for "#rrggbb" only
	btn = "#21262d",
	btn_hover = "#30363d",

	-- plot geometry. fixed, because gfx.frame dimensions are an intrinsic layout claim
	-- and axis labels are flow siblings that must line up with it by hand.
	plot_w = 660,
	plot_h = 132,
	pad_x = 10, -- inset of the first/last sample inside the plot
	pad_y = 10,
	axis_w = 52, -- width of the y-label gutter
}
