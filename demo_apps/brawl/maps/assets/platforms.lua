-- One-way ledges. `lift` is the picture's first opaque row (10 transparent rows above the planks in
-- platform_4.png), so the grass sits on the collision line; `rail` is decoration above the walking
-- surface (the bridge's fence), so its lift is the deck's row. `slice = { left, right }` are the columns
-- of the 3-slice: the ends (supports, posts) kept, the middle between them tiled or stretched.
return {
	wood_ledge = { kind = "platform", image = "art/platform_4.png", size = { 256, 79 }, lift = 10, slice = { 40, 216 } },
	platform = { kind = "platform", image = "art/platform.png", size = { 234, 50 }, lift = 1, slice = { 40, 194 } },
	platform_1 = { kind = "platform", image = "art/platform_1.png", size = { 232, 48 }, lift = 0, slice = { 40, 192 } },
	platform_2 = { kind = "platform", image = "art/platform_2.png", size = { 256, 68 }, lift = 0, slice = { 40, 216 } },
	platform_3 = { kind = "platform", image = "art/platform_3.png", size = { 256, 79 }, lift = 0, slice = { 40, 216 } },
	-- The fence's posts start at row 0; the deck's top is row 93. The angled supports are the end slices.
	bridge = { kind = "platform", image = "art/bridge.png", size = { 467, 180 }, lift = 93, rail = 93, slice = { 60, 407 } },
}
