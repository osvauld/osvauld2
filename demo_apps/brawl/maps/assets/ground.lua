-- Stretchable solid blocks: `top` tiled along the placement, `body` below it. `lift` is the top
-- picture's first opaque row (measured by scripts/brawl_pack.py): that row sits on the collision top.
return {
	grass_ground = { kind = "ground", top = { image = "art/ground_top.png", size = { 128, 97 } }, lift = 0,
		body = "#463c56" },
	-- The tilemap's block (cells 1,0 top strip; 0,0 and 2,0 the corner caps), cropped in step 3. A cap's
	-- opaque part is inset (cap_l starts at column 18, cap_r ends at 113: the grass overhangs the walls).
	-- Step 4: the side walls (cells 0,1 and 2,1) hang under the caps, and `inset` puts the walls' outer
	-- edge on the solid's (wall_l is opaque from column 29, wall_r to column 102: 25 columns past it).
	-- The grass's tufts stand 21 rows above its surface (`rail`), so the lift is row 3 + 21.
	grass_block = { kind = "ground", top = { image = "art/ground_block_top.png", size = { 128, 128 } }, lift = 24, rail = 21,
		left = { image = "art/ground_cap_l.png", size = { 128, 128 } }, right = { image = "art/ground_cap_r.png", size = { 128, 128 } },
		left_wall = { image = "art/ground_wall_l.png", size = { 128, 128 } }, right_wall = { image = "art/ground_wall_r.png", size = { 128, 128 } },
		inset = { 29, 25 }, body = "#463c56" },
}
