-- Soft clouds drawn as SVG (gfx.svg): rings of translucent lobes, no blur. `size` is the file's
-- width and height; the anchor is the bottom centre of the core, not of the faint outer ring.
-- cloud_soft_2 and _3 are written by scripts/brawl_soft_clouds.py; _1 was drawn by hand.
return {
	cloud_soft_1 = { kind = "cloud", svg = "art/cloud_soft_1.svg", size = { 480, 220 }, anchor = { 255, 194 } },
	cloud_soft_2 = { kind = "cloud", svg = "art/cloud_soft_2.svg", size = { 520, 200 }, anchor = { 265, 176 } },
	cloud_soft_3 = { kind = "cloud", svg = "art/cloud_soft_3.svg", size = { 360, 210 }, anchor = { 200, 178 } },
}
