-- A wooden crate. Loose by default: anything that walks into it pushes it.
local S = 48
return {
	size = { S, S },
	parts = {
		{ id = "box", pivot = { S / 2, S / 2 }, shapes = {
			{ path = { {"move",1,1}, {"line",S-1,1}, {"line",S-1,S-1}, {"line",1,S-1}, {"close"} },
				fill = "#a8743f", stroke = { 2.5, "#2b1a10" } },
			{ path = { {"move",4,4}, {"line",S-4,S-4} }, stroke = { 3, "#6b4423" } },
			{ path = { {"move",S-4,4}, {"line",4,S-4} }, stroke = { 3, "#6b4423" } },
		} },
	},
	world = { collider = { rect = { S, S } }, loose = true },
}
