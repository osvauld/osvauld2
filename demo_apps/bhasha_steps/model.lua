local content = require("content")

local progress = doc:open("bhasha_steps_progress")

if not progress.progress then
	progress:set({ "progress" }, doc.map({}))
end

local function entry(id)
	return progress.progress and progress.progress[id] or nil
end

local function record(id, correct)
	local old = entry(id)
	local attempts = (old and old.attempts or 0) + 1
	local mastered = correct or (old and old.mastered) or false
	progress:set({ "progress", id }, doc.map({
		mastered = mastered,
		attempts = attempts,
	}))
end

local function mastered(id)
	local item = entry(id)
	return item and item.mastered or false
end

local function completed_total()
	local total = 0
	for id, _ in pairs(content.by_id) do
		if mastered(id) then total = total + 1 end
	end
	return total
end

local function completed_in(level)
	local total = 0
	for _, exercise in ipairs(level.exercises) do
		if mastered(exercise.id) then total = total + 1 end
	end
	return total
end

local function normalize(value)
	local clean = string.lower(value or "")
	clean = string.gsub(clean, "^%s+", "")
	clean = string.gsub(clean, "%s+$", "")
	clean = string.gsub(clean, "[%.%!%?]+$", "")
	return clean
end

local function typed_correct(exercise, value)
	local clean = normalize(value)
	for _, answer in ipairs(exercise.answers) do
		if clean == normalize(answer) then return true end
	end
	return false
end

local function build_correct(exercise, chosen)
	if #chosen ~= #exercise.answer then return false end
	for i = 1, #chosen do
		if chosen[i] ~= exercise.answer[i] then return false end
	end
	return true
end

return {
	progress = progress,
	record = record,
	mastered = mastered,
	completed_total = completed_total,
	completed_in = completed_in,
	typed_correct = typed_correct,
	build_correct = build_correct,
}
