-- Every feel number, per tick (1/120 s), in drawing units. Multiples of 1/8, so sums stay exact.
-- Plan: docs/design/brawl-movement.md §0.
return {
	run_speed = 7, -- top ground and air speed
	ground_accel = 0.875, -- 8 ticks from rest to run_speed
	ground_friction = 1, -- 7 ticks from run_speed to rest
	air_accel = 0.5,
	air_friction = 0.125,
	turn_min = 4, -- reversing at this speed or more skids first
	skid = 1.5, -- 5 ticks from run_speed to 0
	gravity = 0.25,
	jump_speed = 13, -- full jump peaks 331.5 up
	jump_cut = 7, -- let go while rising faster than this: vy = -jump_cut (peak 107.25 if on tick 2)
	double_jump_speed = 12, -- peaks 282 above where it was used, from rest
	max_fall = 14,
	fast_fall = 20,
	coyote = 8, -- ticks after leaving a ledge a jump is still the ground jump
	buffer = 10, -- ticks a press waits for the ground
	dash_speed = 14, -- 12 ticks: 168 units straight
	dash_diag = 10, -- each axis, diagonal
	dash_ticks = 12,
	dash_keep = 0.5, -- of the dash's velocity, kept when it ends
	wall_slide = 3, -- fall cap while holding into a wall
	wall_jump_x = 8, -- away from the wall
	wall_jump_y = 12,
	wall_lock = 10, -- ticks steering toward the wall is ignored after a wall jump
	drop_ticks = 12, -- after Down on a platform, platforms are ignored
	respawn_ticks = 120,
	-- the picture's acts (look.lua), no effect on movement
	land_ticks = 12, -- a landing with no direction held is `land` this long
	apex_vy = 2, -- in the air, |vy| up to this is `apex`, between `rise` and `fall`
	stocks = 3,
	-- per airtime, refreshed by landing and respawn
	air_jumps = 1,
	air_dashes = 1,
	wall_jumps = 3,
	-- fighting (docs/design/brawl-fighting.md §1): hits, knockback, the buffer
	hitstun_k = 2, -- hitstun ticks per unit of kb, floored
	hitlag_base = 4, -- hitlag = this + floor(dmg / 2), both fighters frozen
	kb_decay = 0.125, -- launch speed lost a tick
	atk_buffer = 8, -- a press this many ticks before a move or lag ends starts on the first free tick
	respawn_inv = 120, -- invulnerable ticks after a respawn
	dmg_max = 999,
	-- dodges (fighting step 2): lengths and invulnerable ticks are 1-based dodge ticks
	dodge_spot_total = 30,
	dodge_spot_from = 3,
	dodge_spot_to = 22,
	dodge_dir_total = 32,
	dodge_dir_from = 3,
	dodge_dir_to = 20,
	dodge_speed = 8, -- directional: this a tick for dodge_run ticks (128), then x dodge_keep
	dodge_run = 16,
	dodge_keep = 0.25,
	dodge_cd = 48, -- ground: ticks from one dodge's start to the next's
	air_dodges = 1, -- per airtime, refreshed by landing, respawn and a hit
	-- the jetpack (fighting step 4): heavy in the air, neutral or Up, is the ignite; K held burns the tank
	jet_kick = 8, -- ignite: vy = -jet_kick on the press tick (12 ticks rise 76.5)
	jet_cost = 24, -- fuel an ignite takes
	jet_fuel = 120, -- a full tank: jet ticks after the ignite are jet_fuel - jet_cost
	jet_accel = 0.375, -- how fast hover, climb and thrust reach their target speed
	jet_rise = 6, -- climb speed, Up held
	jet_side = 8, -- thrust speed, a side held
	jet_refill = 4, -- fuel back a tick on the ground (30 ticks from empty)
	cut_ticks = 10, -- `jet_cut` length
	-- the CPU (fighting step 5, bot.lua)
	bot_react = 12, -- ticks late it sees the others
	bot_think = 10, -- ticks between plans
	bot_dodge = 0.5, -- odds it dodges an attack it sees coming (on the 1/1024 grid)
	kill_pct = 100, -- from here it prefers heavies
	spacing = 140, -- the farthest it closes to
	edge_margin = 120, -- how close to a ledge it walks
	body_w = 60, -- the box, feet at its bottom centre; the robot's chassis and boots are 81.5 wide, drawn at Look.SCALE (0.75)
	body_h = 156, -- 208 (measured from the robot, step 4: helmet top 210 above the feet) * Look.SCALE
}
