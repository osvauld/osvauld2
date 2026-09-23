-- PALETTE
--
-- Pure data, required by everything that draws. First file to split out precisely because it
-- depends on nothing: if `require` were broken, this is the one that would still work.
--
-- `model.lua` requires it too, which looks like a layering mistake and is not: a resize drag has
-- to clamp *while it is happening*, or the pointer runs past the limit and the column does not
-- start moving again until it comes back. The clamp is where the write is, and the numbers are
-- here with the rest of the board's geometry.
return {
	bg = "#0d1117",
	panel = "#161b22",
	card = "#1c2128",
	card_hi = "#22282f",
	sunken = "#0d1117",
	line = "#30363d",
	line_soft = "#21262d",
	text = "#e6edf3",
	muted = "#8b949e",
	accent = "#2f81f7",
	accent_hi = "#4493f8",
	danger = "#f85149",

	-- Column geometry. `col_w` is what a column starts at; the bounds are what a drag may write.
	-- The floor is the one that matters — a column dragged to nothing has no grip left to drag
	-- back by, and there is no undo for it.
	col_w = 300,
	col_w_min = 180,
	col_w_max = 620,
}
