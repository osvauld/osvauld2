-- Single pictures, placed by their anchor (default: the bottom centre of the picture, which is its
-- opaque box's bottom centre but for the vase and the boulder). `layer` is the default draw layer; a map
-- piece may override it where the kind allows. `collide` is a template in picture pixels from the anchor
-- (a solid {x, y, w, h}, or a one-way platform {x, y, w}); a placement multiplies it by its scale.
return {
	tree = { kind = "prop", image = "art/tree.png", size = { 440, 603 } },
	tree_1 = { kind = "prop", image = "art/tree_1.png", size = { 596, 611 } },
	tree_2 = { kind = "prop", image = "art/tree_2.png", size = { 445, 615 } },
	bush = { kind = "prop", image = "art/bush.png", size = { 376, 224 } },
	bush_1 = { kind = "prop", image = "art/bush_1.png", size = { 376, 224 } },
	bush_2 = { kind = "prop", image = "art/bush_2.png", size = { 326, 231 } },
	bush_3 = { kind = "prop", image = "art/bush_3.png", size = { 313, 231 } },
	rock = { kind = "prop", image = "art/rock.png", size = { 196, 120 } },
	rock_1 = { kind = "prop", image = "art/rock_1.png", size = { 120, 92 } },
	rock_2 = { kind = "prop", image = "art/rock_2.png", size = { 110, 121 } },
	-- Boulders are solid: a box inside the picture, standing on the anchor. `boulder` at its default
	-- scale 0.75 is the 150 x 150 block of the Twin Isles plan; the others are 0.75 and 1.
	boulder = { kind = "prop", image = "art/boulder.png", size = { 234, 234 }, anchor = { 117, 233 }, scale = 0.75,
		collide = { solid = { -100, -200, 200, 200 } } },
	boulder_2 = { kind = "prop", image = "art/boulder_2.png", size = { 240, 239 }, scale = 0.75,
		collide = { solid = { -110, -220, 220, 220 } } },
	boulder_3 = { kind = "prop", image = "art/boulder_3.png", size = { 117, 117 },
		collide = { solid = { -52, -104, 104, 104 } } },
	boulder_5 = { kind = "prop", image = "art/boulder_5.png", size = { 120, 118 },
		collide = { solid = { -52, -104, 104, 104 } } },
	grass = { kind = "prop", image = "art/grass.png", size = { 117, 118 }, layer = "front" },
	grass_1 = { kind = "prop", image = "art/grass_1.png", size = { 117, 109 }, layer = "front" },
	grass_2 = { kind = "prop", image = "art/grass_2.png", size = { 116, 119 }, layer = "front" },
	grass_3 = { kind = "prop", image = "art/grass_3.png", size = { 116, 119 }, layer = "front" },
	grass_4 = { kind = "prop", image = "art/grass_4.png", size = { 116, 119 }, layer = "front" },
	grass_5 = { kind = "prop", image = "art/grass_5.png", size = { 115, 119 }, layer = "front" },
	sign = { kind = "prop", image = "art/sign.png", size = { 115, 127 } },
	flag_1 = { kind = "prop", image = "art/flag_1.png", size = { 103, 243 } },
	chest = { kind = "prop", image = "art/chest.png", size = { 192, 104 } },
	vase = { kind = "prop", image = "art/vase.png", size = { 84, 121 }, anchor = { 42, 120 } },
	-- The tilemap's side walls under grass_block's corner caps (cells 0,1 and 2,1).
	ground_wall_l = { kind = "prop", image = "art/ground_wall_l.png", size = { 128, 128 } },
	ground_wall_r = { kind = "prop", image = "art/ground_wall_r.png", size = { 128, 128 } },
	-- The pack's flat clouds; the soft SVG ones are in clouds.lua.
	cloud = { kind = "prop", image = "art/cloud.png", size = { 500, 167 }, layer = "far" },
	cloud_1 = { kind = "prop", image = "art/cloud_1.png", size = { 500, 218 }, layer = "far" },
	cloud_2 = { kind = "prop", image = "art/cloud_2.png", size = { 469, 241 }, layer = "far" },
	cloud_3 = { kind = "prop", image = "art/cloud_3.png", size = { 456, 248 }, layer = "far" },
}
