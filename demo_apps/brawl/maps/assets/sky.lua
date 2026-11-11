-- Skies: vector, a gradient from the top of the view to its bottom (offsets 0..1), and an optional
-- sun glow at a fraction of the view (x, y), r pixels. `day` is today's yard sky.
return {
	day = { kind = "sky", stops = { { 0, "#78c4d6" }, { 1, "#eef5df" } } },
}
