return {
	bg = "#12131c",
	card = "#1c1e2b",
	hot = "#2a2e44",
	on = "#3d6fd8",
	text = "#e7e9f3",
	muted = "#8a8fa8",
	-- The stage is a colour no Paka pixel comes near (nearest is 124 away), so S-C4 can tell
	-- the character from the background.
	stage = "#00ffff",
	-- One slot per character. The drawing box sits `pad_x`, `pad_y` into its slot: poses reach
	-- past the box (characters/paka/report.json: reach), above it most. The box's left 100 units
	-- are empty, so a narrow slot paints it `shift` to the left; only `attack`'s blade reaches the
	-- next slot.
	slots = { wide = { w = 480, shift = 0 }, narrow = { w = 226, shift = -84 } },
	slot_h = 440,
	pad_x = 20,
	pad_y = 100,
}
