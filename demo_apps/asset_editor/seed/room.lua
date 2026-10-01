-- A stage: assets placed together. Positions are the top-left of each asset's box; a mounted
-- entity has no position of its own — it sits on its host's socket.
return {
	scene = {
		size = { 720, 400 },
		entities = {
			{ id = "chest", asset = "chest", pos = { 460, 250 } },
			{ id = "hero", asset = "hero", pos = { 140, 110 }, clip = "idle" },
			{ id = "hat", asset = "hat", mount = { to = "hero", socket = "head_top" } },
		},
	},
}
