-- Who is on stage, each a rig and a folder of clips by path. Every character plays the clip of
-- the same name; the robots have Paka's part ids and play Paka's own clip files.
local M = {}

-- The converter's file names (scripts/spine_to_lua.py); Lua can't list a folder.
M.names = {
	"attack", "attack_no_weapon", "blink_need_track_combination", "defeat", "defeat_no_weapon",
	"defeat_weapon", "fall_swim", "fall_swim_no_weapon", "fall_swim_weapon", "idle",
	"idle_no_weapon", "idle_weapon", "interact", "interact_no_weapon", "jump", "jump_no_weapon",
	"jump_weapon", "run", "run_no_weapon", "run_weapon", "talk", "talk_no_weapon", "walk",
	"walk_no_weapon", "walk_weapon",
}

M.characters = {
	{ id = "paka", rig = "characters/paka/rig", clips = "characters/paka/clips/", slot = "wide" },
	{ id = "scout", rig = "characters/robot/scout", clips = "characters/paka/clips/", slot = "narrow" },
	{ id = "brute", rig = "characters/robot/brute", clips = "characters/paka/clips/", slot = "narrow" },
	{ id = "volt", rig = "characters/robot/volt", clips = "characters/paka/clips/", slot = "narrow" },
}

for _, c in ipairs(M.characters) do
	c.drawing = gfx.drawing(require(c.rig))
end

-- Clips load on first use: all 25 are 55k keys, and a module load has a CPU budget.
local loaded = {}
function M.clip(c, name)
	local path = c.clips .. name
	local h = loaded[path]
	if not h then
		h = gfx.clip(require(path))
		loaded[path] = h
	end
	return h
end

return M
