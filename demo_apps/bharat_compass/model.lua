local Content = require("content")

local progress = doc:open("bharat_compass_progress")
if not progress.exercises then
	progress:set({ "exercises" }, doc.map({}))
end

local S = {
	class = 1,
	activity = 1,
	choice = nil,
	match_left = nil,
	matches = {},
	route = { { 1, 3 } },
	feedback = nil,
}

local function current()
	return Content.classes[S.class].exercises[S.activity]
end

local function clear_attempt()
	S.choice = nil
	S.match_left = nil
	S.matches = {}
	local exercise = current()
	S.route = exercise.kind == "route" and { { exercise.map.start[1], exercise.map.start[2] } } or {}
	S.feedback = nil
end

local function go(class, activity)
	S.class = math.max(1, math.min(#Content.classes, class))
	S.activity = math.max(1, math.min(3, activity))
	clear_attempt()
end

local function next_exercise()
	if S.activity < 3 then
		go(S.class, S.activity + 1)
	elseif S.class < #Content.classes then
		go(S.class + 1, 1)
	end
end

local function previous_exercise()
	if S.activity > 1 then
		go(S.class, S.activity - 1)
	elseif S.class > 1 then
		go(S.class - 1, 3)
	end
end

local function choose(id)
	S.choice = id
	S.feedback = nil
end

local function select_left(id)
	S.match_left = id
	S.feedback = nil
end

local function select_right(id)
	if not S.match_left then
		S.feedback = { kind = "ready", text = "Choose a card on the left first." }
		return
	end
	for left, right in pairs(S.matches) do
		if right == id then S.matches[left] = nil end
	end
	S.matches[S.match_left] = id
	S.match_left = nil
	S.feedback = nil
end

local function blocked(map, col, row)
	for _, spot in ipairs(map.blocked or {}) do
		if spot[1] == col and spot[2] == row then return true end
	end
	return false
end

local function move(direction)
	local map = current().map
	local here = S.route[#S.route]
	local dx = direction == "E" and 1 or (direction == "W" and -1 or 0)
	local dy = direction == "S" and 1 or (direction == "N" and -1 or 0)
	local col, row = here[1] + dx, here[2] + dy
	if col < 1 or col > map.cols or row < 1 or row > map.rows or blocked(map, col, row) then
		S.feedback = { kind = "ready", text = "That way is blocked. Try another direction." }
		return
	end
	S.route[#S.route + 1] = { col, row }
	S.feedback = nil
end

local function undo_move()
	if #S.route > 1 then table.remove(S.route) end
	S.feedback = nil
end

local function reset_route()
	local start = current().map.start
	S.route = { { start[1], start[2] } }
	S.feedback = nil
end

local function route_correct(exercise)
	local map, here = exercise.map, S.route[#S.route]
	if here[1] ~= map.goal[1] or here[2] ~= map.goal[2] then return false end
	if map.max_steps and #S.route - 1 > map.max_steps then return false end
	local required = map.checkpoints or (map.checkpoint and { map.checkpoint }) or {}
	for _, checkpoint in ipairs(required) do
		local visited = false
		for _, point in ipairs(S.route) do
			if point[1] == checkpoint[1] and point[2] == checkpoint[2] then visited = true break end
		end
		if not visited then return false end
	end
	return true
end

local function matches_complete(exercise)
	for _, item in ipairs(exercise.left) do
		if not S.matches[item.id] then return false end
	end
	return true
end

local function is_correct(exercise)
	if exercise.kind == "choose" then
		return S.choice == exercise.answer
	end
	if exercise.kind == "match" then
		for left, right in pairs(exercise.answers) do
			if S.matches[left] ~= right then return false end
		end
		return true
	end
	return route_correct(exercise)
end

local function can_check(exercise)
	if exercise.kind == "choose" then return S.choice ~= nil end
	if exercise.kind == "match" then return matches_complete(exercise) end
	return #S.route > 1
end

local function check()
	local exercise = current()
	if not can_check(exercise) then
		S.feedback = { kind = "ready", text = "Finish this activity before checking." }
		return
	end

	local correct = is_correct(exercise)
	local saved = progress.exercises and progress.exercises[exercise.id]
	if correct and not (saved and saved.mastered) then
		progress:set({ "exercises", exercise.id }, doc.map({ mastered = true }))
	end
	S.feedback = {
		kind = correct and "correct" or "retry",
		text = correct and exercise.success or exercise.clue,
	}
end

local function mastered(id)
	local entry = progress.exercises and progress.exercises[id]
	return entry and entry.mastered or false
end

local function mastered_count(class)
	local count = 0
	for _, exercise in ipairs(Content.classes[class].exercises) do
		if mastered(exercise.id) then count = count + 1 end
	end
	return count
end

local function total_mastered()
	local count = 0
	for _, level in ipairs(Content.classes) do
		count = count + mastered_count(level.class)
	end
	return count
end

return {
	content = Content,
	progress = progress,
	state = S,
	current = current,
	go = go,
	next = next_exercise,
	previous = previous_exercise,
	choose = choose,
	select_left = select_left,
	select_right = select_right,
	move = move,
	undo_move = undo_move,
	reset_route = reset_route,
	clear_attempt = clear_attempt,
	can_check = can_check,
	check = check,
	mastered = mastered,
	mastered_count = mastered_count,
	total_mastered = total_mastered,
}
