-- Skies: vector, a gradient from the top of the view to its bottom (offsets 0..1), and an optional
-- sun glow at a fraction of the view (x, y), `r` px on a 1 600 px playfield (the glow fades out to 4 r).
return {
	day = { kind = "sky", stops = { { 0, "#5aaed0" }, { 0.55, "#a7dbe2" }, { 1, "#eef5df" } },
		sun = { x = 0.8, y = 0.16, r = 70, color = "#fff6d2" } },
	-- Golden hour (Twin Isles): dusk blue to peach to straw, a low sun on the left.
	golden = { kind = "sky", stops = { { 0, "#4a64a0" }, { 0.5, "#eaa57c" }, { 1, "#f9e0b2" } },
		sun = { x = 0.22, y = 0.42, r = 90, color = "#ffe3a6" } },
}
