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
	body_w = 80, -- the box, feet at its bottom centre; the robot's chassis and boots are 81.5 wide
	body_h = 208, -- measured from the robot (step 4): helmet top 210 above the feet
}
