-- Authored, bundled practice content. It is curriculum-informed, not an official syllabus.
local classes = {
	{
		class = 1,
		name = "Nearby places",
		intro = "Notice directions, land, water, and the route you take each day.",
		exercises = {
			{
				id = "c1_morning_direction",
				kind = "choose",
				title = "Morning direction",
				prompt = "The Sun appears on one side of the sky in the morning. What do we call that direction?",
				options = {
					{ id = "north", label = "North" },
					{ id = "east", label = "East" },
					{ id = "west", label = "West" },
				},
				answer = "east",
				clue = "Think of the direction linked with sunrise.",
				success = "East is the direction where the Sun appears to rise as Earth turns.",
			},
			{
				id = "c1_land_and_water",
				kind = "match",
				title = "Land and water",
				prompt = "Tap a place, then tap the description that belongs with it.",
				left = {
					{ id = "hill", label = "Hill" },
					{ id = "river", label = "River" },
					{ id = "pond", label = "Pond" },
				},
				right = {
					{ id = "flowing", label = "Water flowing in a channel" },
					{ id = "raised", label = "Land raised above nearby ground" },
					{ id = "still", label = "A smaller body of still water" },
				},
				answers = { hill = "raised", river = "flowing", pond = "still" },
				clue = "Ask whether each word names raised land, flowing water, or still water.",
				success = "You matched each nearby feature to what makes it different.",
			},
			{
				id = "c1_route_to_school",
				kind = "route",
				title = "A route to school",
				prompt = "Move Riya from home to school. Go east past the neem tree, then north.",
				map = {
					cols = 4, rows = 3, start = { 1, 3 }, goal = { 4, 1 },
					answer = { "E", "E", "N", "N", "E" },
					landmarks = { { 3, 3, "tree" }, { 3, 2, "bridge" } },
					checkpoints = { { 3, 3 }, { 3, 2 } },
				},
				clue = "From home: east, east, north, north, east.",
				success = "You guided Riya past the neem tree and bridge to school.",
			},
		},
	},
	{
		class = 2,
		name = "Views and symbols",
		intro = "Look from above, read simple symbols, and follow route clues.",
		exercises = {
			{
				id = "c2_view_from_above",
				kind = "choose",
				title = "A useful map view",
				prompt = "You want to draw where the desks and door are in a classroom. Which view is most useful?",
				options = {
					{ id = "above", label = "A view from above" },
					{ id = "close", label = "A close-up of one pencil" },
					{ id = "under", label = "A view from under a desk" },
				},
				answer = "above",
				clue = "Choose the view that lets you see how many things are placed across the room.",
				success = "A view from above helps show the position of many objects on one plan.",
			},
			{
				id = "c2_map_symbols",
				kind = "match",
				title = "Read the key",
				prompt = "Match each simple map symbol with what its key says it means.",
				left = {
					{ id = "blue_line", label = "Blue line" },
					{ id = "dotted_line", label = "Dotted line" },
					{ id = "star", label = "Star" },
				},
				right = {
					{ id = "landmark", label = "Important landmark" },
					{ id = "river", label = "River" },
					{ id = "path", label = "Walking path" },
				},
				answers = { blue_line = "river", dotted_line = "path", star = "landmark" },
				clue = "A map key explains every symbol; colour and line style both give clues.",
				success = "A key turns small marks into useful information about a place.",
			},
			{
				id = "c2_library_route",
				kind = "route",
				title = "Find the library",
				prompt = "Guide Aarav east to the post office, then north to the library. The pond blocks one square.",
				map = {
					cols = 4, rows = 4, start = { 1, 4 }, goal = { 4, 1 },
					answer = { "E", "E", "E", "N", "N", "N" },
					blocked = { { 2, 2, "water" } }, landmarks = { { 4, 4, "post" } }, checkpoint = { 4, 4 },
				},
				clue = "Reach the post office on the east edge before travelling north.",
				success = "You used east and north to describe the route from park to library.",
			},
		},
	},
	{
		class = 3,
		name = "Directions and landforms",
		intro = "Use relative position and compare broad physical features.",
		exercises = {
			{
				id = "c3_relative_direction",
				kind = "choose",
				title = "North of the playground",
				prompt = "The library is north of the playground. From the library, in which direction is the playground?",
				options = {
					{ id = "north", label = "North" },
					{ id = "south", label = "South" },
					{ id = "east", label = "East" },
				},
				answer = "south",
				clue = "The return direction is opposite to north.",
				success = "South is opposite north, so the playground is south of the library.",
			},
			{
				id = "c3_landforms",
				kind = "match",
				title = "Compare landforms",
				prompt = "Match each landform with its broad description.",
				left = {
					{ id = "plain", label = "Plain" },
					{ id = "plateau", label = "Plateau" },
					{ id = "coast", label = "Coast" },
				},
				right = {
					{ id = "sea_edge", label = "Land beside the sea" },
					{ id = "flat_low", label = "Broad, mostly level low land" },
					{ id = "high_flat", label = "Raised land with a broad top" },
				},
				answers = { plain = "flat_low", plateau = "high_flat", coast = "sea_edge" },
				clue = "Compare height, shape, and whether the land meets the sea.",
				success = "Landforms can be compared by their shape, height, and relation to water.",
			},
			{
				id = "c3_river_journey",
				kind = "route",
				title = "Follow the river valley",
				prompt = "Travel from the spring to the sea. Cross at the bridge; rocky hills block the upper bank.",
				map = {
					cols = 5, rows = 4, start = { 1, 1 }, goal = { 5, 4 },
					answer = { "S", "S", "E", "E", "S", "E", "E" },
					blocked = { { 2, 1, "hill" }, { 3, 1, "hill" }, { 4, 2, "hill" } },
					landmarks = { { 3, 3, "bridge" } }, checkpoint = { 3, 3 },
				},
				clue = "Move south along the valley and cross the bridge before heading east.",
				success = "You followed lower land past a crossing to the larger body of water.",
			},
		},
	},
	{
		class = 4,
		name = "Map tools and cycles",
		intro = "Use scale and map evidence, then connect stages in a natural cycle.",
		exercises = {
			{
				id = "c4_scale_distance",
				kind = "choose",
				title = "Use a map scale",
				prompt = "On a map, 1 centimetre stands for 5 kilometres. Two villages are 3 centimetres apart. What ground distance does the map suggest?",
				options = {
					{ id = "8", label = "8 kilometres" },
					{ id = "15", label = "15 kilometres" },
					{ id = "35", label = "35 kilometres" },
				},
				answer = "15",
				clue = "Each of the 3 centimetres represents 5 kilometres.",
				success = "Three groups of 5 kilometres make a suggested distance of 15 kilometres.",
			},
			{
				id = "c4_map_tools",
				kind = "match",
				title = "Choose the map tool",
				prompt = "Match each map tool with the question it helps answer.",
				left = {
					{ id = "compass", label = "Compass rose" },
					{ id = "scale", label = "Scale" },
					{ id = "contours", label = "Contour lines" },
				},
				right = {
					{ id = "steep", label = "Where may the slope be steep?" },
					{ id = "direction", label = "Which direction is the lake?" },
					{ id = "distance", label = "How far apart are two places?" },
				},
				answers = { compass = "direction", scale = "distance", contours = "steep" },
				clue = "Direction, distance, and height patterns each need a different map tool.",
				success = "Map tools answer different questions: direction, distance, and the shape of land.",
			},
			{
				id = "c4_water_cycle",
				kind = "route",
				title = "Water-cycle field route",
				prompt = "Take the survey path from the warm lake to the rain gauge in 8 moves or fewer. Marsh squares are closed.",
				map = {
					cols = 5, rows = 5, start = { 1, 5 }, goal = { 5, 1 }, max_steps = 8,
					answer = { "N", "N", "E", "E", "N", "N", "E", "E" },
					blocked = { { 2, 4, "marsh" }, { 3, 4, "marsh" }, { 4, 2, "marsh" } },
					landmarks = { { 1, 3, "cloud" }, { 3, 3, "rain" } }, checkpoint = { 3, 3 },
				},
				clue = "Visit the rain marker in the centre and keep the route within 8 moves.",
				success = "You read obstacles, a waypoint, and route length together on one field map.",
			},
		},
	},
	{
		class = 5,
		name = "Evidence and preparedness",
		intro = "Apply geographic evidence to planning, climate, and safety decisions.",
		exercises = {
			{
				id = "c5_heavy_rain_planning",
				kind = "choose",
				title = "Plan for heavy rain",
				prompt = "A town often receives very heavy rain. Which planning idea best helps water move safely?",
				options = {
					{ id = "block", label = "Build across every low water path" },
					{ id = "drains", label = "Keep drains clear and leave low water paths open" },
					{ id = "ignore", label = "Ignore where rainwater collects" },
				},
				answer = "drains",
				clue = "Look for the choice that works with the route water already takes.",
				success = "Clear drainage and open flow paths can reduce water collecting in unsafe places.",
			},
			{
				id = "c5_read_evidence",
				kind = "match",
				title = "Read geographic evidence",
				prompt = "Match each observation with the careful conclusion it supports.",
				left = {
					{ id = "weather", label = "Many years of weather records" },
					{ id = "close_contours", label = "Contour lines close together" },
					{ id = "river_settlement", label = "A settlement beside a river" },
				},
				right = {
					{ id = "investigate", label = "Investigate water benefits and flood risk" },
					{ id = "climate", label = "Study longer-term climate patterns" },
					{ id = "slope", label = "Expect a steeper slope" },
				},
				answers = { weather = "climate", close_contours = "slope", river_settlement = "investigate" },
				clue = "A good conclusion stays close to what the evidence can actually show.",
				success = "Geographic evidence supports questions and careful conclusions, not guesses beyond the data.",
			},
			{
				id = "c5_cyclone_readiness",
				kind = "route",
				title = "Reach the cyclone shelter",
				prompt = "Follow the safe route from home to the shelter. Avoid flooded squares and pass the warning station.",
				map = {
					cols = 6, rows = 5, start = { 1, 5 }, goal = { 6, 1 }, max_steps = 11,
					answer = { "N", "E", "E", "N", "E", "E", "E", "N", "N" },
					blocked = { { 1, 3, "flood" }, { 2, 3, "flood" }, { 4, 4, "flood" }, { 5, 2, "flood" } },
					landmarks = { { 3, 4, "warning" }, { 4, 2, "supplies" } }, checkpoint = { 3, 4 },
				},
				clue = "Go through the warning station, then use the dry corridor towards the shelter.",
				success = "You used verified guidance and avoided unsafe low areas on the way to shelter.",
			},
		},
	},
}

local by_id = {}
for _, level in ipairs(classes) do
	for _, exercise in ipairs(level.exercises) do
		by_id[exercise.id] = exercise
	end
end

return { classes = classes, by_id = by_id }
