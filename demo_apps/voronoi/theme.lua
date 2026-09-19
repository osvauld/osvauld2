local C = {
	bg = "#0d1117",
	panel = "#161b22",
	text = "#c9d1d9",
	dim = "#6e7681",
	accent = "#1f6feb",
	accent_hot = "#388bfd",

	edge = "#0d1117",
	halo = "#e6edf3",
	site_rim = "#0d1117",
	site_core = "#c9d1d9",
	site_core_hot = "#ffffff",

	w = 760,
	h = 460,
	margin = 26,
	speed = 24,
	swirl = 0.9,

	site_r = 4,
	site_hit = 8,
	edge_w = 1.5,
	halo_w = 3,
}

C.cells = {
	"hsl(212,48%,40%)",
	"hsl(160,40%,34%)",
	"hsl(44,52%,40%)",
	"hsl(280,34%,44%)",
	"hsl(4,46%,44%)",
	"hsl(190,44%,36%)",
	"hsl(330,38%,44%)",
	"hsl(96,34%,36%)",
}

C.cells_hot = {
	"hsl(212,66%,58%)",
	"hsl(160,54%,50%)",
	"hsl(44,70%,56%)",
	"hsl(280,50%,62%)",
	"hsl(4,64%,60%)",
	"hsl(190,60%,52%)",
	"hsl(330,54%,60%)",
	"hsl(96,48%,52%)",
}

return C
