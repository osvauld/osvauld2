-- The lid hinges at its back edge, so an "open" clip is one rotation about that pivot.
return {
	size = { 96, 80 },
	parts = {
		{ id = "base", pivot = { 48, 80 }, shapes = {
			{ path = { {"move",8,36}, {"line",88,36}, {"line",88,76}, {"quad",88,80,84,80},
				{"line",12,80}, {"quad",8,80,8,76}, {"close"} },
				fill = "#9a5b2e", stroke = { 2.5, "#2b1a10" } },
			{ path = { {"move",8,50}, {"line",88,50}, {"line",88,56}, {"line",8,56}, {"close"} },
				fill = "#e0b04a" },
		} },
		{ id = "lid", parent = "base", pivot = { 8, 36 }, shapes = {
			{ path = { {"move",8,36}, {"line",8,24}, {"cubic",8,6,88,6,88,24}, {"line",88,36}, {"close"} },
				fill = "#b36a36", stroke = { 2.5, "#2b1a10" } },
			{ path = { {"move",42,30}, {"line",54,30}, {"line",54,42}, {"line",42,42}, {"close"} },
				fill = "#e0b04a", stroke = { 2, "#2b1a10" } },
		} },
	},
}
