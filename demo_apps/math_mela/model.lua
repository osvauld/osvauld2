local Content = require("content")

local progress = doc:open("math_mela_progress")
if not progress.mastered then
	progress:set({ "mastered" }, doc.map({}))
end

local S = {
	class_no = 1,
	kind = "choose",
	picked = nil,
	numeric_draft = "",
	feedback = nil,
	manip = { marker = 0, step = 1, digits = { 0, 0, 0, 0 }, rows = {} },
}

local function current()
	return Content.get(S.class_no, S.kind)
end

local function reset_activity()
	S.picked = nil
	S.numeric_draft = ""
	S.feedback = nil
	S.manip = { marker = 0, step = 1, digits = { 0, 0, 0, 0 }, rows = {} }
end

local function select_class(class_no)
	S.class_no = class_no
	reset_activity()
end

local function select_kind(kind)
	S.kind = kind
	reset_activity()
end

local function mastered(id)
	return progress.mastered and progress.mastered[id] == true
end

local function complete(exercise)
	progress:set({ "mastered", exercise.id }, true)
	S.feedback = { correct = true, text = exercise.explanation }
end

local function retry(exercise)
	S.feedback = { correct = false, text = exercise.hint }
end

local function check_choice()
	local exercise = current()
	if not S.picked then
		S.feedback = { correct = false, text = "Choose one answer first." }
	elseif S.picked == exercise.answer then
		complete(exercise)
	else
		retry(exercise)
	end
end

local function check_numeric(value)
	local exercise = current()
	local number = tonumber(value)
	if not number then
		S.feedback = { correct = false, text = "Type a number, then check it." }
	elseif number == exercise.answer then
		complete(exercise)
	else
		retry(exercise)
	end
end

local function move_marker(value, phase)
	local exercise = current()
	local spec = exercise.manip
	S.manip.marker = math.max(spec.min, math.min(spec.max, value))
	if phase ~= "end" then
		S.feedback = nil
		return
	end
	local target = spec.targets[S.manip.step]
	local tolerance = (spec.max - spec.min) / 20
	if math.abs(S.manip.marker - target) <= tolerance then
		S.manip.marker = target
		if S.manip.step == #spec.targets then
			complete(exercise)
		else
			S.manip.step = S.manip.step + 1
			S.feedback = { correct = false, text = "Good landing. Now find " .. spec.labels[S.manip.step] .. "." }
		end
	else
		retry(exercise)
	end
end

local function cycle_digit(place)
	local exercise = current()
	local digits = S.manip.digits
	digits[place] = (digits[place] + 1) % 10
	for i, wanted in ipairs(exercise.manip.digits) do
		if digits[i] ~= wanted then
			S.feedback = { correct = false, text = exercise.hint }
			return
		end
	end
	complete(exercise)
end

local function toggle_row(row)
	local exercise = current()
	local split = exercise.manip.split
	if row > split then
		for i = 1, split do
			if not S.manip.rows[i] then
				S.feedback = { correct = false, text = "Build the first 4 orange rows before adding 18." }
				return
			end
		end
	end
	S.manip.rows[row] = not S.manip.rows[row]
	if row <= split and not S.manip.rows[row] then
		for i = split + 1, exercise.manip.rows do S.manip.rows[i] = false end
	end
	local first, count = true, 0
	for i = 1, exercise.manip.rows do
		if S.manip.rows[i] then count = count + 1 end
		if i <= split and not S.manip.rows[i] then first = false end
	end
	if count == exercise.manip.rows then
		complete(exercise)
	elseif first and count == split then
		S.feedback = { correct = false, text = "You made 4 rows of 6 = 24. Add 3 teal rows for 18." }
	else
		S.feedback = { correct = false, text = exercise.hint }
	end
end

local function next_activity()
	if S.kind == "choose" then
		select_kind("numeric")
	elseif S.kind == "numeric" then
		select_kind("order")
	elseif S.class_no < 5 then
		S.class_no = S.class_no + 1
		S.kind = "choose"
		reset_activity()
	else
		S.class_no = 1
		S.kind = "choose"
		reset_activity()
	end
end

local function completed_count(class_no)
	local count = 0
	for _, kind in ipairs(Content.kinds) do
		if mastered(Content.get(class_no, kind).id) then
			count = count + 1
		end
	end
	return count
end

local function total_completed()
	local count = 0
	for class_no = 1, 5 do
		count = count + completed_count(class_no)
	end
	return count
end

return {
	progress = progress,
	state = S,
	current = current,
	select_class = select_class,
	select_kind = select_kind,
	mastered = mastered,
	check_choice = check_choice,
	check_numeric = check_numeric,
	move_marker = move_marker,
	cycle_digit = cycle_digit,
	toggle_row = toggle_row,
	next_activity = next_activity,
	completed_count = completed_count,
	total_completed = total_completed,
}
