-- The hero in profile, facing right; the world mirrors it for left. Same part ids and box as
-- the front view, so the idle clip plays on it unchanged. `leg_l`/`arm_far` are the far side.
return {
	size = { 160, 240 },
	parts = {
		{ id = "arm_far", parent = "body", pivot = { 80, 98 }, shapes = {
			{ path = { {"move",73,100}, {"quad",73,93,80,93}, {"quad",87,93,87,100}, {"line",87,146},
				{"quad",87,153,80,153}, {"quad",73,153,73,146}, {"close"} },
				fill = "#c23a86", stroke = { 2, "#1b1b3a" } },
		} },
		{ id = "leg_l", parent = "body", pivot = { 80, 164 }, shapes = {
			{ path = { {"move",73,164}, {"line",87,164}, {"line",87,212}, {"line",73,212}, {"close"} },
				fill = "#26304a", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",72,210}, {"line",90,210}, {"quad",96,210,96,216}, {"line",96,220},
				{"line",72,220}, {"close"} },
				fill = "#1b1b3a" },
		} },
		{ id = "leg_r", parent = "body", pivot = { 80, 164 }, shapes = {
			{ path = { {"move",73,164}, {"line",87,164}, {"line",87,212}, {"line",73,212}, {"close"} },
				fill = "#2e3a59", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",72,210}, {"line",90,210}, {"quad",96,210,96,216}, {"line",96,220},
				{"line",72,220}, {"close"} },
				fill = "#1b1b3a" },
		} },
		{ id = "body", pivot = { 80, 166 }, shapes = {
			{ path = { {"move",72,88}, {"line",88,88}, {"quad",98,88,98,100}, {"line",98,154},
				{"quad",98,166,88,166}, {"line",72,166}, {"quad",62,166,62,154}, {"line",62,100},
				{"quad",62,88,72,88}, {"close"} },
				fill = "#f84aa7", stroke = { 2, "#1b1b3a" } },
		} },
		{ id = "head", parent = "body", pivot = { 80, 88 }, shapes = {
			{ path = { {"move",107,50}, {"line",124,61}, {"line",106,68}, {"close"} },
				fill = "#ffd9b3", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",110,58}, {"cubic",110,74.57,96.57,88,80,88}, {"cubic",63.43,88,50,74.57,50,58},
				{"cubic",50,41.43,63.43,28,80,28}, {"cubic",96.57,28,110,41.43,110,58}, {"close"} },
				fill = "#ffd9b3", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",50,64}, {"cubic",44,36,66,24,86,27}, {"cubic",98,29,106,36,108,44},
				{"quad",90,38,78,46}, {"quad",68,54,66,70}, {"quad",56,72,50,64}, {"close"} },
				fill = "#3b2f5c", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",98,56}, {"cubic",98,58.21,96.21,60,94,60}, {"cubic",91.79,60,90,58.21,90,56},
				{"cubic",90,53.79,91.79,52,94,52}, {"cubic",96.21,52,98,53.79,98,56}, {"close"} },
				fill = "#1b1b3a" },
			{ path = { {"move",96,73}, {"quad",101,76,105,72} }, stroke = { 2, "#1b1b3a" } },
		} },
		{ id = "arm_near", parent = "body", pivot = { 80, 98 }, shapes = {
			{ path = { {"move",73,100}, {"quad",73,93,80,93}, {"quad",87,93,87,100}, {"line",87,146},
				{"quad",87,153,80,153}, {"quad",73,153,73,146}, {"close"} },
				fill = "#f84aa7", stroke = { 2, "#1b1b3a" } },
			{ path = { {"move",86,156}, {"cubic",86,159.31,83.31,162,80,162}, {"cubic",76.69,162,74,159.31,74,156},
				{"cubic",74,152.69,76.69,150,80,150}, {"cubic",83.31,150,86,152.69,86,156}, {"close"} },
				fill = "#ffd9b3", stroke = { 2, "#1b1b3a" } },
		} },
	},
}
