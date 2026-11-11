-- Character 1 - Paka.json (Spine 3.8.75), skin 'classic', converted by scripts/spine_to_lua.py.
-- Rest pose: every bone unrotated and unscaled at its setup offset from its parent; rotation and
-- scale live in the clips (setup.lua is the setup pose). List order is draw order.
-- The box is the setup pose, the root bone at its bottom centre; clips reach outside it (report.json: reach).
return {
	size = { 396, 239 },
	parts = {
		{ id = "root", pivot = { 198, 239 } },
		{ id = "body", parent = "root", pivot = { 196.868, 163.412 } },
		{ id = "bone", parent = "root", pivot = { 196.554, 169.374 } },
		{ id = "leg_back", parent = "bone", pivot = { 222.88, 146.192 } },
		{ id = "hoof-back", parent = "leg_back", pivot = { 255.43, 146.45 } },
		{ id = "leg_front", parent = "bone", pivot = { 225.17, 184.562 } },
		{ id = "hoof_front", parent = "leg_front", pivot = { 257.548, 184.542 } },
		{ id = "body2", parent = "body", pivot = { 196.876, 158.936 } },
		{ id = "head_neck", parent = "body", pivot = { 197.334, 138.868 } },
		{ id = "head_main", parent = "head_neck", pivot = { 221.162, 138.434 } },
		{ id = "body4", parent = "body", pivot = { 204.08, 154.58 } },
		{ id = "amulet", parent = "body", pivot = { 213.382, 130.53 } },
		{ id = "shoulder_front", parent = "body", pivot = { 168.962, 121.346 } },
		{ id = "arm_front", parent = "shoulder_front", pivot = { 203.76, 121.034 } },
		{ id = "palm_front", parent = "arm_front", pivot = { 239.85, 121.002 } },
		{ id = "palm_front3", parent = "palm_front", pivot = { 240.014, 119.578 } },
		{ id = "palm_front4", parent = "palm_front", pivot = { 240.714, 122.906 } },
		{ id = "shoulder_back", parent = "body", pivot = { 228.276, 128.216 } },
		{ id = "arm_back", parent = "shoulder_back", pivot = { 254.864, 129.826 } },
		{ id = "palm_back", parent = "arm_back", pivot = { 288.982, 130.01 } },
		{ id = "palm_back3", parent = "palm_back", pivot = { 289.106, 128.432 } },
		{ id = "palm_back4", parent = "palm_back", pivot = { 288.752, 131.396 } },
		{ id = "tie", parent = "amulet", pivot = { 227.906, 130.672 } },
		{ id = "tie2", parent = "tie", pivot = { 243.842, 130.794 } },
		{ id = "tie3", parent = "tie2", pivot = { 256.864, 130.81 } },
		{ id = "sword", parent = "body", pivot = { 120.912, 69.704 } },
		{ id = "head_ciolca", parent = "head_main", pivot = { 307.576, 111.098 } },
		{ id = "head_ciolca2", parent = "head_ciolca", pivot = { 309.904, 106.47 } },
		{ id = "head_ciolca3", parent = "head_ciolca", pivot = { 329.096, 102.976 } },
		{ id = "head_ciolca4", parent = "head_ciolca", pivot = { 354.736, 101.086 } },
		{ id = "head_ear_back", parent = "head_main", pivot = { 288.732, 173.882 } },
		{ id = "head_ear_back2", parent = "head_ear_back", pivot = { 303.286, 173.884 } },
		{ id = "head_ear_back3", parent = "head_ear_back2", pivot = { 312.834, 174.104 } },
		{ id = "head_ear_back4", parent = "head_ear_back3", pivot = { 324.08, 174.21 } },
		{ id = "head_ear_front", parent = "head_main", pivot = { 288.396, 104.706 } },
		{ id = "head_ear_front2", parent = "head_ear_front", pivot = { 303.026, 104.8 } },
		{ id = "head_ear_front3", parent = "head_ear_front2", pivot = { 317.348, 104.594 } },
		{ id = "head_ear_front4", parent = "head_ear_front3", pivot = { 332.782, 104.494 } },
		{ id = "head_nose", parent = "head_main", pivot = { 242.274, 155.618 } },
		{ id = "eyes_control", parent = "head_main", pivot = { 262.078, 155.246 } },
		{ id = "eye_white_back", parent = "head_main", pivot = { 245.894, 244.602 } },
		{ id = "eye_front", parent = "head_main", pivot = { 243.362, 232.642 } },
		{ id = "head_smile", parent = "head_nose", pivot = { 238.92, 155.854 } },
		{ id = "head_smile3", parent = "head_smile", pivot = { 243.416, 154.786 } },
		{ id = "head_smile2", parent = "head_smile", pivot = { 243.482, 157.208 } },
		{ id = "dark_eye_back", parent = "eyes_control", pivot = { 237.428, 179.308 } },
		{ id = "dark_eye_front", parent = "eyes_control", pivot = { 240.974, 125.02 } },
		{ id = "head_control_front", parent = "head_main", pivot = { 251.668, 155.26 } },
		{ id = "head_control_back", parent = "head_main", pivot = { 285.596, 95.872 } },
		{ id = "target2", parent = "root", pivot = { 217.702, 243.464 } },
		{ id = "target3", parent = "root", pivot = { 175.966, 243.276 } },
		{ id = "sword_forward", parent = "palm_back", pivot = { 297.234, 129.984 } },
		{ id = "eye_white_front", parent = "dark_eye_front", pivot = { 256.752, 129.46 } },
		{ id = "eye_front2", parent = "dark_eye_front", pivot = { 250.626, 134.334 } },
		{ id = "eye_white_back2", parent = "dark_eye_back", pivot = { 253.554, 180.606 } },
		{ id = "eye_back", parent = "dark_eye_back", pivot = { 253.474, 182.96 } },
		{ id = "blink_front", parent = "dark_eye_front", pivot = { 247.616, 121.626 } },
		{ id = "blink_back", parent = "dark_eye_back", pivot = { 242.646, 181.072 } },
		{ id = "slot:sword", parent = "sword", pivot = { 120.912, 69.704 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 2, 161, 472, 747 },
			  transform = { 0.101596, -0.172275, 0.172273, 0.101598, 109.227691, 72.235091 } },
		} },
		{ id = "slot:palm_back", parent = "palm_back3", pivot = { 289.106, 128.432 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 743, 55, 119, 128 },
			  transform = { 0.16094, -0.118731, 0.118728, 0.160944, 274.569473, 128.800881 } },
		} },
		{ id = "slot:sword_forward", parent = "sword_forward", pivot = { 297.234, 129.984 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 2, 161, 472, 747 },
			  transform = { 0.101967, -0.172055, 0.172053, 0.101967, 274.221847, 131.879647 } },
		} },
		{ id = "slot:arm_back", parent = "arm_back", pivot = { 254.864, 129.826 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1332, 44, 170, 208 },
			  transform = { 0.063714, -0.188335, 0.189453, 0.06409, 249.364529, 137.063523 } },
		} },
		{ id = "slot:shoulder_back", parent = "shoulder_back", pivot = { 228.276, 128.216 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1104, 236, 204, 248 },
			  transform = { 0.100026, -0.173192, 0.173188, 0.100021, 205.762511, 136.373918 } },
		} },
		{ id = "slot:head_neck", parent = "head_neck", pivot = { 197.334, 138.868 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 2, 5, 335, 154 },
			  transform = { 0.013209, 0.199564, -0.199564, 0.01321, 230.412652, 104.791644 } },
		} },
		{ id = "slot:hoof-back", parent = "hoof-back", pivot = { 255.43, 146.45 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 339, 2, 162, 157 },
			  transform = { -0.014386, -0.198244, 0.199479, -0.014481, 237.270615, 165.007017 } },
		} },
		{ id = "slot:leg_back", parent = "leg_back", pivot = { 222.88, 146.192 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 933, 2, 198, 198 },
			  transform = { 0.004381, -0.199952, 0.199952, 0.00438, 217.589781, 165.233825 } },
		} },
		{ id = "slot:hoof_front", parent = "hoof_front", pivot = { 257.548, 184.542 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 503, 4, 161, 157 },
			  transform = { -0.021158, -0.198873, 0.198879, -0.021159, 239.657531, 202.86721 } },
		} },
		{ id = "slot:leg_front", parent = "leg_front", pivot = { 225.17, 184.562 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1620, 264, 198, 198 },
			  transform = { -0.013837, -0.199522, 0.199518, -0.013836, 221.240202, 205.851124 } },
		} },
		{ id = "slot:body", parent = "body4", pivot = { 204.08, 154.58 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1033, 486, 421, 422 },
			  transform = { 0.200475, 0, 0, 0.199999, 154.741586, 126.424651 } },
		} },
		{ id = "slot:tie", parent = "tie", pivot = { 227.906, 130.672 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1310, 254, 98, 230 },
			  transform = { 0.014692, -0.201503, 0.199465, 0.014551, 221.946032, 140.427085 } },
		} },
		{ id = "slot:amulet", parent = "amulet", pivot = { 213.382, 130.53 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1456, 464, 347, 130 },
			  transform = { 0.017485, -0.199812, 0.199238, 0.01743, 196.532069, 175.253752 } },
		} },
		{ id = "slot:palm_front", parent = "palm_front3", pivot = { 240.014, 119.578 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1744, 147, 112, 115 },
			  transform = { 0.087671, -0.179743, 0.17975, 0.087693, 227.849595, 127.886686 } },
		} },
		{ id = "slot:arm_front", parent = "arm_front", pivot = { 203.76, 121.034 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1133, 10, 197, 224 },
			  transform = { -0.050291, -0.194623, 0.193641, -0.050032, 205.46361, 144.927826 } },
		} },
		{ id = "slot:head_ear_front", parent = "head_ear_front", pivot = { 288.396, 104.706 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 736, 185, 195, 268 },
			  transform = { -0.136673, 0.146013, -0.146009, -0.136674, 346.476469, 115.542085 } },
		} },
		{ id = "slot:head_ear_back", parent = "head_ear_back", pivot = { 288.732, 173.882 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 933, 202, 169, 251 },
			  transform = { 0.118535, 0.161083, -0.161083, 0.118546, 321.370657, 141.540894 } },
		} },
		{ id = "slot:head_main", parent = "head_main", pivot = { 221.162, 138.434 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 476, 455, 555, 453 },
			  transform = { -0.012226, 0.199987, -0.199627, -0.012204, 312.681874, 82.597945 } },
		} },
		{ id = "slot:dark_eye_front", parent = "dark_eye_front", pivot = { 240.974, 125.02 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1410, 270, 208, 192 },
			  transform = { -0.154688, 0.125254, -0.125857, -0.155434, 287.460921, 132.034901 } },
		} },
		{ id = "slot:dark_eye_back", parent = "dark_eye_back", pivot = { 237.428, 179.308 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1504, 87, 104, 181 },
			  transform = { 0.045334, 0.192824, -0.194685, 0.045774, 269.361312, 162.443409 } },
		} },
		{ id = "slot:head_ciolca", parent = "head_ciolca", pivot = { 307.576, 111.098 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1456, 596, 397, 312 },
			  transform = { 0.193499, -0.05841, 0.055854, 0.191418, 291.335003, 84.623893 } },
		} },
		{ id = "slot:eye_white_front", parent = "eye_white_front", pivot = { 256.752, 129.46 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1610, 138, 132, 124 },
			  transform = { 0.233333, -2e-06, -6e-06, 0.235481, 242.220551, 113.928473 } },
		} },
		{ id = "slot:eye_front", parent = "eye_front2", pivot = { 250.626, 134.334 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1719, 65, 70, 71 },
			  transform = { 0.234286, 0, 0, 0.233803, 240.886, 125.64 } },
		} },
		{ id = "slot:head_smile", parent = "head_smile2", pivot = { 243.482, 157.208 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1805, 465, 27, 129 },
			  transform = { 0.053084, -0.192784, 0.19282, 0.053102, 231.089789, 155.804151 } },
		} },
		{ id = "slot:head_nose", parent = "head_smile", pivot = { 238.92, 155.854 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 864, 74, 65, 109 },
			  transform = { 0.19967, -0.011528, 0.011516, 0.199676, 229.157906, 146.767139 } },
		} },
		{ id = "slot:eye_white_back", parent = "eye_white_back2", pivot = { 253.554, 180.606 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1610, 49, 107, 87 },
			  transform = { 0, 0.233645, -0.234483, 0, 263.114, 167.542 } },
		} },
		{ id = "slot:eye_back", parent = "eye_back", pivot = { 253.474, 182.96 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1719, 2, 60, 61 },
			  transform = { 0.23333, -7e-06, 7e-06, 0.232783, 244.853599, 175.424212 } },
		} },
		{ id = "slot:shoulder_front", parent = "shoulder_front", pivot = { 168.962, 121.346 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 476, 163, 258, 290 },
			  transform = { -0.066378, -0.188663, 0.18866, -0.066382, 166.537774, 153.441083 } },
		} },
		{ id = "slot:path_death", parent = "root", pivot = { 198, 239 } },
		{ id = "slot:blink_front", parent = "blink_front", pivot = { 247.616, 121.626 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 666, 25, 75, 136 },
			  transform = { -0.125852, -0.155433, 0.155438, -0.125863, 245.912849, 139.773507 } },
		} },
		{ id = "slot:blink_back", parent = "blink_back", pivot = { 242.646, 181.072 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 1504, 24, 94, 61 },
			  transform = { 0.045778, 0.194687, -0.194691, 0.045784, 251.152161, 170.310188 } },
		} },
	},
}
