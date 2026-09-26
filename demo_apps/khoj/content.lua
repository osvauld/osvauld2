local exercises = {
	{
		id = "c1-living", class = 1, kind = "classify", tab = "Living or not?",
		title = "Who grows and needs food?",
		prompt = "Sort each familiar thing into the right group.",
		categories = {
			{ id = "living", label = "Living" },
			{ id = "nonliving", label = "Non-living" },
		},
		items = {
			{ id = "butterfly", label = "Butterfly", answer = "living" },
			{ id = "kite", label = "Paper kite", answer = "nonliving" },
			{ id = "neem", label = "Neem tree", answer = "living" },
			{ id = "pot", label = "Clay pot", answer = "nonliving" },
		},
		clue = "Ask: does it grow, breathe, or need food and water?",
		explanation = "Plants and animals are living. Kites and pots do not grow or need food.",
	},
	{
		id = "c1-sprout", class = 1, kind = "sequence", tab = "A seed wakes up",
		title = "From seed to seedling",
		prompt = "Tap the pictures-in-words in the order they happen.",
		steps = {
			{ id = "leaves", label = "First leaves open" },
			{ id = "seed", label = "A seed rests in moist soil" },
			{ id = "shoot", label = "A shoot grows upward" },
			{ id = "root", label = "A tiny root grows downward" },
		},
		answer = { "seed", "root", "shoot", "leaves" },
		clue = "Begin with the seed in soil. The root appears before the shoot.",
		explanation = "The root anchors the plant and takes in water before the shoot opens its first leaves.",
	},
	{
		id = "c1-sunlight", class = 1, kind = "quiz", tab = "Plant helper",
		title = "What gives a plant energy?",
		prompt = "Tap the picture that helps a green plant make food.",
		diagram = "plant-needs",
		choices = {
			{ id = "sun", label = "Sunlight" },
			{ id = "water", label = "Water" },
			{ id = "ball", label = "Toy ball" },
		},
		answer = "sun",
		clue = "Look for something that shines in the sky during the day.",
		explanation = "Green leaves use sunlight to make food. A plant also needs water and air to live.",
	},
	{
		id = "c2-natural", class = 2, kind = "classify", tab = "Made by whom?",
		title = "Nature-made or people-made",
		prompt = "Sort what comes from nature and what people shape or build.",
		categories = {
			{ id = "natural", label = "From nature" },
			{ id = "made", label = "Made by people" },
		},
		items = {
			{ id = "river", label = "River", answer = "natural" },
			{ id = "bus", label = "City bus", answer = "made" },
			{ id = "cotton", label = "Cotton plant", answer = "natural" },
			{ id = "spoon", label = "Steel spoon", answer = "made" },
		},
		clue = "Think about whether it can exist without somebody building or shaping it.",
		explanation = "Rivers and plants occur in nature. People manufacture buses and shape steel into spoons.",
	},
	{
		id = "c2-handwash", class = 2, kind = "sequence", tab = "Clean hands",
		title = "Wash away germs",
		prompt = "Put these handwashing actions in a useful order.",
		steps = {
			{ id = "scrub", label = "Scrub with soap" },
			{ id = "dry", label = "Dry with a clean cloth" },
			{ id = "wet", label = "Wet hands" },
			{ id = "rinse", label = "Rinse well and close the tap" },
		},
		answer = { "wet", "scrub", "rinse", "dry" },
		clue = "Water comes first, and drying works best after rinsing.",
		explanation = "Wet, scrub, rinse, then dry. Closing the tap while soaping and after rinsing saves water.",
	},
	{
		id = "c2-frog-home", class = 2, kind = "quiz", tab = "A frog's home",
		title = "Where would a frog thrive?",
		prompt = "Tap the habitat that offers water, insects, and damp shelter.",
		diagram = "frog-home",
		choices = {
			{ id = "pond", label = "Pond edge" },
			{ id = "desert", label = "Dry desert" },
			{ id = "cupboard", label = "Cupboard" },
		},
		answer = "pond",
		clue = "Frogs have moist skin and lay their eggs in water.",
		explanation = "A pond edge gives a frog water, food, and places to hide on land nearby.",
	},
	{
		id = "c3-matter", class = 3, kind = "classify", tab = "Solid or liquid",
		title = "Does it flow?",
		prompt = "Classify each material as a solid or a liquid at room temperature.",
		categories = {
			{ id = "solid", label = "Solid" },
			{ id = "liquid", label = "Liquid" },
		},
		items = {
			{ id = "brick", label = "Brick", answer = "solid" },
			{ id = "milk", label = "Milk", answer = "liquid" },
			{ id = "rice", label = "One grain of rice", answer = "solid" },
			{ id = "oil", label = "Cooking oil", answer = "liquid" },
		},
		clue = "A liquid flows and takes the shape of its container. A tiny grain is still a solid.",
		explanation = "Milk and oil flow. A brick and each rice grain keep their own shape.",
	},
	{
		id = "c3-water-cycle", class = 3, kind = "sequence", tab = "Water's round trip",
		title = "Follow water through the sky",
		prompt = "Build one simple round of the water cycle.",
		steps = {
			{ id = "cloud", label = "Droplets gather into clouds" },
			{ id = "collect", label = "Water collects in rivers, lakes, and seas" },
			{ id = "rain", label = "Rain falls to the ground" },
			{ id = "warm", label = "Sun warms surface water" },
			{ id = "vapour", label = "Water vapour rises" },
		},
		answer = { "warm", "vapour", "cloud", "rain", "collect" },
		clue = "Start where sunlight warms water in a pond, lake, or sea.",
		explanation = "Water evaporates, condenses into clouds, falls as rain, and collects before the cycle begins again.",
	},
	{
		id = "c3-leaf", class = 3, kind = "quiz", tab = "Plant parts",
		title = "The plant's food factory",
		prompt = "Tap the part that uses sunlight to make food.",
		diagram = "plant-parts",
		choices = {
			{ id = "root", label = "Root" },
			{ id = "stem", label = "Stem" },
			{ id = "leaf", label = "Leaf" },
		},
		answer = "leaf",
		clue = "Choose the usually green part with a broad surface facing light.",
		explanation = "Leaves contain chlorophyll and use light, water, and carbon dioxide to make food.",
	},
	{
		id = "c4-conductors", class = 4, kind = "classify", tab = "Will it conduct?",
		title = "Electrical conductors and insulators",
		prompt = "Sort these materials. Imagine a teacher testing them safely in a low-voltage circuit.",
		categories = {
			{ id = "conductor", label = "Conductor" },
			{ id = "insulator", label = "Insulator" },
		},
		items = {
			{ id = "coin", label = "Metal coin", answer = "conductor" },
			{ id = "rubber", label = "Rubber eraser", answer = "insulator" },
			{ id = "steel", label = "Steel spoon", answer = "conductor" },
			{ id = "wood", label = "Dry wooden ruler", answer = "insulator" },
		},
		clue = "Metals usually carry electric current; dry rubber and wood resist it.",
		explanation = "The metal objects conduct. Rubber and dry wood are insulators. Never test a wall socket.",
	},
	{
		id = "c4-butterfly", class = 4, kind = "sequence", tab = "Butterfly changes",
		title = "A complete life cycle",
		prompt = "Arrange the stages from one generation to the next.",
		steps = {
			{ id = "adult", label = "Adult butterfly" },
			{ id = "egg", label = "Egg on a leaf" },
			{ id = "pupa", label = "Pupa (chrysalis)" },
			{ id = "larva", label = "Larva (caterpillar)" },
		},
		answer = { "egg", "larva", "pupa", "adult" },
		clue = "The caterpillar hatches from the egg; the winged adult emerges last.",
		explanation = "Butterflies undergo metamorphosis: egg, larva, pupa, then adult. The adult can lay eggs again.",
	},
	{
		id = "c4-circuit", class = 4, kind = "quiz", tab = "Control a circuit",
		title = "Open and close the path",
		prompt = "Drag the green switch contact to open or close a simple circuit.",
		diagram = "circuit-parts",
		choices = {
			{ id = "cell", label = "Electric cell" },
			{ id = "switch", label = "Switch" },
			{ id = "bulb", label = "Bulb" },
		},
		answer = "switch",
		clue = "Look for the part with a movable contact that can break the path.",
		explanation = "A switch controls the path. A closed switch completes it; an open switch breaks it.",
	},
	{
		id = "c5-energy", class = 5, kind = "classify", tab = "Energy sources",
		title = "Renewable or non-renewable",
		prompt = "Sort each source by whether nature replaces it on a human timescale.",
		categories = {
			{ id = "renewable", label = "Renewable" },
			{ id = "nonrenewable", label = "Non-renewable" },
		},
		items = {
			{ id = "sunlight", label = "Sunlight", answer = "renewable" },
			{ id = "coal", label = "Coal", answer = "nonrenewable" },
			{ id = "wind", label = "Wind", answer = "renewable" },
			{ id = "petrol", label = "Petrol", answer = "nonrenewable" },
		},
		clue = "Ask whether using it now can exhaust a store that took millions of years to form.",
		explanation = "Sunlight and wind are replenished. Coal and petroleum fuels form extremely slowly and can run out.",
	},
	{
		id = "c5-food-chain", class = 5, kind = "sequence", tab = "Energy in a food chain",
		title = "Trace energy through a grassland",
		prompt = "Begin with the energy source and build this food chain.",
		steps = {
			{ id = "frog", label = "Frog" },
			{ id = "sun", label = "Sun" },
			{ id = "grasshopper", label = "Grasshopper" },
			{ id = "grass", label = "Grass" },
		},
		answer = { "sun", "grass", "grasshopper", "frog" },
		clue = "Plants capture the starting energy; then ask who eats whom.",
		explanation = "Sunlight powers grass. A grasshopper eats the grass, and a frog can eat the grasshopper.",
	},
	{
		id = "c5-digestion", class = 5, kind = "quiz", tab = "Digestive journey",
		title = "Where are most nutrients absorbed?",
		prompt = "Tap the organ where most digested nutrients pass into the blood.",
		diagram = "digestion",
		choices = {
			{ id = "mouth", label = "Mouth" },
			{ id = "stomach", label = "Stomach" },
			{ id = "intestine", label = "Small intestine" },
		},
		answer = "intestine",
		clue = "It is the long, coiled organ after the stomach, with a very large inner surface.",
		explanation = "Most nutrient absorption happens through the lining of the small intestine after digestion breaks food down.",
	},
}

local M = { all = exercises }

function M.get(class, kind)
	for _, exercise in ipairs(exercises) do
		if exercise.class == class and exercise.kind == kind then
			return exercise
		end
	end
end

return M
