-- Stretchable solid blocks: `top` tiled along the placement, `body` below it. `lift` is the top
-- picture's first opaque row (measured by scripts/brawl_pack.py): that row sits on the collision top.
return {
	grass_ground = { kind = "ground", top = { image = "art/ground_top.png", size = { 128, 97 } }, lift = 0,
		body = "#463c56" },
	-- The tilemap's block (cells 1,0 top strip; 0,0 and 2,0 the corner caps), cropped in step 3 for step 4
	-- to judge against `grass_ground`. A cap's opaque part is inset (cap_l starts at column 18, cap_r ends
	-- at 113): the side walls below them are the props ground_wall_l and ground_wall_r.
	grass_block = { kind = "ground", top = { image = "art/ground_block_top.png", size = { 128, 128 } }, lift = 2,
		left = { image = "art/ground_cap_l.png", size = { 128, 128 } }, right = { image = "art/ground_cap_r.png", size = { 128, 128 } },
		body = "#463c56" },
}
