-- palette + geometry; depends on nothing
return {
	bg = "#0d1117",
	panel = "#161b22",
	line = "#30363d",
	text = "#e6edf3",
	dim = "#8b949e",
	hub = "#161b22", -- the centre disc: unnamed paint, so the pointer falls through it

	-- the plot is square and fixed: gfx.frame dimensions are an intrinsic layout claim
	size = 320,
	radius = 128,
	pop = 12, -- how far a slice slides out along its own bisector when it is hot
}
