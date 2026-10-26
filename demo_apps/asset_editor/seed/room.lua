-- A stage: assets placed together. Positions are the top-left of each asset's box; a mounted
-- entity has no position of its own — it sits on its host's socket. An entity's world fields
-- (`collider`, `sensor`, `loose`, `group`, `blocks`) replace its asset's `world` ones; `false`
-- takes one away. `controller` is the room's alone.
return {
	scene = {
		size = { 720, 400 },
		entities = {
			{ id = "chest", asset = "chest", pos = { 460, 264 } },
			-- The room's keys drive this hero; the rival, the same asset, has none.
			{ id = "hero", asset = "hero", pos = { 140, 110 }, clip = "idle", controller = {
				speed = 160, axis_x = { neg = "KeyA", pos = "KeyD" }, axis_y = { neg = "KeyW", pos = "KeyS" },
			} },
			{ id = "crate", asset = "crate", pos = { 40, 308 } },
			-- On the floor: drag its brim onto the hero's head and it equips onto `head_top`.
			{ id = "hat", asset = "hat", pos = { 320, 300 } },
			-- The same hero asset in a teal shirt: an entity's `vars` make a variant of its asset.
			{ id = "rival", asset = "hero", pos = { 560, 100 }, clip = "idle",
				vars = { shirt = "#2ec4b6", shirt_shade = "#25a89c", shirt_far = "#1f8f85" } },
		},
	},
}
