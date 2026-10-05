-- The hero from behind, for walking up: the front view's body with the head turned away —
-- hair over the face and a bun on top. Same part ids and pivots, so every hero clip plays on it.
return {
	size = { 160, 240 },
	parts = {
		{ id = "arm_far", parent = "body", pivot = { 56, 98 }, shapes = {
			{ path = { {"move",49,100}, {"quad",49,93,56,93}, {"quad",63,93,63,100}, {"line",63,146},
				{"quad",63,153,56,153}, {"quad",49,153,49,146}, {"close"} },
				fill = "#c23a86", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",62,156}, {"cubic",62,159.31,59.31,162,56,162}, {"cubic",52.69,162,50,159.31,50,156},
				{"cubic",50,152.69,52.69,150,56,150}, {"cubic",59.31,150,62,152.69,62,156}, {"close"} },
				fill = "#e8b894", stroke = { 2, "#1b1b3a" } },
		} },
		{ id = "leg_l", parent = "body", pivot = { 68, 164 }, shapes = {
			{ path = { {"move",61,164}, {"line",75,164}, {"line",75,212}, {"line",61,212}, {"close"} },
				fill = "#2e3a59", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",58,210}, {"line",76,210}, {"quad",82,210,82,216}, {"line",82,220}, {"line",56,220},
				{"line",56,214}, {"quad",56,210,58,210}, {"close"} },
				fill = "#1b1b3a" },
		} },
		{ id = "leg_r", parent = "body", pivot = { 92, 164 }, shapes = {
			{ path = { {"move",85,164}, {"line",99,164}, {"line",99,212}, {"line",85,212}, {"close"} },
				fill = "#2e3a59", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",82,210}, {"line",100,210}, {"quad",106,210,106,216}, {"line",106,220}, {"line",80,220},
				{"line",80,214}, {"quad",80,210,82,210}, {"close"} },
				fill = "#1b1b3a" },
		} },
		{ id = "body", pivot = { 80, 166 }, shapes = {
			{ path = { {"move",66,88}, {"line",94,88}, {"quad",106,88,106,100}, {"line",106,154},
				{"quad",106,166,94,166}, {"line",66,166}, {"quad",54,166,54,154}, {"line",54,100},
				{"quad",54,88,66,88}, {"close"} },
				fill = "#f84aa7", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",92,90}, {"quad",104,90,104,100}, {"line",104,154}, {"quad",104,164,94,164},
				{"line",90,164}, {"quad",98,128,92,90}, {"close"} },
				fill = "#d63d8f" },
		} },
		{ id = "head", parent = "body", pivot = { 80, 88 }, shapes = {
			{ path = { {"move",110,58}, {"cubic",110,74.57,96.57,88,80,88}, {"cubic",63.43,88,50,74.57,50,58},
				{"cubic",50,41.43,63.43,28,80,28}, {"cubic",96.57,28,110,41.43,110,58}, {"close"} },
				fill = "#ffd9b3", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",51,66}, {"cubic",46,38,64,27,80,27}, {"cubic",96,27,114,38,109,66},
				{"quad",96,80,80,80}, {"quad",64,80,51,66}, {"close"} },
				fill = "#3b2f5c", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",90,20}, {"cubic",90,25.52,85.52,30,80,30}, {"cubic",74.48,30,70,25.52,70,20},
				{"cubic",70,14.48,74.48,10,80,10}, {"cubic",85.52,10,90,14.48,90,20}, {"close"} },
				fill = "#3b2f5c", stroke = { 2, "#1b1b3a" } },
		} },
		{ id = "arm_near", parent = "body", pivot = { 104, 98 }, shapes = {
			{ path = { {"move",97,100}, {"quad",97,93,104,93}, {"quad",111,93,111,100}, {"line",111,146},
				{"quad",111,153,104,153}, {"quad",97,153,97,146}, {"close"} },
				fill = "#f84aa7", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",110,156}, {"cubic",110,159.31,107.31,162,104,162}, {"cubic",100.69,162,98,159.31,98,156},
				{"cubic",98,152.69,100.69,150,104,150}, {"cubic",107.31,150,110,152.69,110,156}, {"close"} },
				fill = "#ffd9b3", stroke = { 2, "#1b1b3a" } },
		} },
	},
}
