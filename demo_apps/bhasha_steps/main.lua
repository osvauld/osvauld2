local C = require("theme")
local Content = require("content")
local Model = require("model")
local W = require("widgets")

local S = {
	class = 1,
	activity = 1,
	selected = nil,
	built = {},
	used = {},
	draft = "",
	feedback = nil,
	drag = nil,
	drop = nil,
}

local function current()
	local level = Content.classes[S.class]
	return level, level.exercises[S.activity]
end

local function reset_attempt()
	S.selected = nil
	S.built = {}
	S.used = {}
	S.draft = ""
	S.feedback = nil
	S.drag = nil
	S.drop = nil
end

local function open(class_index, activity_index)
	S.class = class_index
	S.activity = activity_index
	reset_attempt()
end

local function respond(exercise, correct)
	Model.record(exercise.id, correct)
	S.feedback = {
		kind = correct and "correct" or "wrong",
		title = correct and "Lovely work!" or "Almost there — try again.",
		body = correct and exercise.explanation or exercise.clue,
	}
end

local function nudge(text)
	S.feedback = { kind = "nudge", title = "One small step first", body = text }
end

local function next_activity()
	if S.activity < 3 then
		open(S.class, S.activity + 1)
	elseif S.class < #Content.classes then
		open(S.class + 1, 1)
	end
end

local function kind_badge(exercise)
	local names = {
		choose = "CHOOSE ONE",
		build = "BUILD A SENTENCE",
		type = "TYPE A SHORT ANSWER",
	}
	local fills = { choose = C.blue_soft, build = C.gold_soft, type = C.purple_soft }
	local inks = { choose = C.blue, build = C.gold, type = C.purple }
	return W.badge(names[exercise.kind], fills[exercise.kind], inks[exercise.kind])
end

local function choose_activity(exercise)
	local options = {}
	for i, option in ipairs(exercise.options) do
		local state = nil
		if S.feedback and S.feedback.kind == "correct" and i == exercise.answer then
			state = "correct"
		elseif S.feedback and S.feedback.kind == "wrong" and i == S.selected then
			state = "wrong"
		end
		options[#options + 1] = W.option(
			"option:" .. exercise.id .. ":" .. i,
			option,
			S.selected == i,
			state,
			function()
				S.selected = i
				S.feedback = nil
			end
		)
	end
	return ui.col({
		gap = 10,
		options,
		ui.row({
			wrap = true,
			gap = 9,
			W.button("check:" .. exercise.id, "Check answer", function()
				if not S.selected then
					nudge("Choose an answer, then check it.")
					return
				end
				respond(exercise, S.selected == exercise.answer)
			end, true),
		}),
	})
end

local function built_words()
	local words = {}
	for _, item in ipairs(S.built) do words[#words + 1] = item.word end
	return words
end

local function remove_built(position)
	local item = table.remove(S.built, position)
	if item then S.used[item.index] = nil end
end

local function place_drag(slot)
	local drag = S.drag
	if not drag then return end
	if drag.from then
		table.remove(S.built, drag.from)
		if slot > drag.from then slot = slot - 1 end
	else
		S.used[drag.index] = true
	end
	table.insert(S.built, slot, { index = drag.index, word = drag.word })
	S.feedback = nil
	S.drag, S.drop = nil, nil
end

local function drag_handler(item, from)
	return function(e)
		if e.phase == "start" then
			S.drag = {
				index = item.index,
				word = item.word,
				from = from,
				x = e.origin_x,
				y = e.origin_y,
				scale = e.scale,
			}
		elseif e.phase == "move" and S.drag then
			S.drag.x, S.drag.y, S.drag.scale = e.origin_x, e.origin_y, e.scale
			S.drop = nil
		elseif e.phase == "end" then
			S.drag, S.drop = nil, nil
		end
	end
end

local function drop_handler(slot)
	return function(e)
		if not S.drag then return end
		if e.phase == "over" then
			S.drop = { kind = "slot", slot = slot }
		else
			place_drag(slot)
		end
	end
end

local function build_activity(exercise)
	local sentence = {}
	for slot = 1, #S.built + 1 do
		local active = S.drop and S.drop.kind == "slot" and S.drop.slot == slot
		sentence[#sentence + 1] = ui.col({
			id = "slot:" .. exercise.id .. ":" .. slot,
			w = active and 28 or 18,
			min_h = 38,
			radius = 7,
			fill = active and C.saffron or C.saffron_soft,
			stroke_dash = { 1, C.saffron, 3, 3 },
			on_drop = drop_handler(slot),
		})
		local item = S.built[slot]
		if item then
			local position = slot
			local dragging = S.drag and S.drag.from == position
			sentence[#sentence + 1] = ui.button({
				id = "built:" .. exercise.id .. ":" .. item.index,
				px = 11,
				py = 7,
				radius = 9,
				fill = C.navy,
				hover_fill = C.navy_hi,
				opacity = dragging and 0.3 or 1,
				press_scale = 0.97,
				on_drag = drag_handler(item, position),
				on_click = function()
					remove_built(position)
					S.feedback = nil
				end,
				ui.text({ item.word, color = C.white, font_size = 13, no_wrap = true }),
			})
		end
	end

	local bank = {}
	for i, word in ipairs(exercise.tokens) do
		local item = { index = i, word = word }
		if not S.used[i] then
			bank[#bank + 1] = ui.button({
				id = "token:" .. exercise.id .. ":" .. i,
				px = 11,
				py = 7,
				radius = 9,
				fill = C.paper,
				hover_fill = C.gold_soft,
				stroke = { 1, C.line_strong },
				press_scale = 0.96,
				on_drag = drag_handler(item, nil),
				on_click = function()
					S.used[i] = true
					S.built[#S.built + 1] = item
					S.feedback = nil
				end,
				ui.text({ word, color = C.ink, font_size = 13, no_wrap = true }),
			})
		end
	end

	local removing = S.drop and S.drop.kind == "remove"
	local remove_zone = S.drag and S.drag.from and ui.row({
		id = "remove:" .. exercise.id,
		min_h = 38,
		px = 12,
		radius = 9,
		center = true,
		fill = removing and C.wrong or C.wrong_soft,
		stroke_dash = { 1, C.wrong, 4, 3 },
		on_drop = function(e)
			if e.phase == "over" then
				S.drop = { kind = "remove" }
			elseif S.drag and S.drag.from then
				remove_built(S.drag.from)
				S.feedback = nil
				S.drag, S.drop = nil, nil
			end
		end,
		ui.text({ "Drop here to remove", color = removing and C.white or C.wrong, font_size = 11, no_wrap = true }),
	}) or false

	return ui.col({
		gap = 13,
		ui.col({
			min_h = 82,
			pad = 13,
			gap = 8,
			radius = 12,
			fill = C.paper_warm,
			stroke = { 1, C.line },
			ui.text({ "YOUR SENTENCE · DROP BETWEEN WORDS", color = C.faint, font_size = 9, no_wrap = true }),
			ui.row({ wrap = true, gap = 5, align_center = true, sentence }),
			remove_zone,
		}),
		ui.text({ "Drag every card into a slot. Drag placed cards to reorder or remove them.", color = C.muted, font_size = 11 }),
		ui.row({ wrap = true, gap = 8, bank }),
		ui.text({ "Click fallback: a bank card appends; a placed card returns to the bank.", color = C.faint, font_size = 10 }),
		ui.row({
			wrap = true,
			gap = 9,
			W.button("check:" .. exercise.id, "Check sentence", function()
				if #S.built < #exercise.answer then
					nudge("Use every word before you check the sentence.")
					return
				end
				local correct = Model.build_correct(exercise, built_words())
				respond(exercise, correct)
				if not correct then S.feedback.title = "Not yet — move the cards and retry." end
			end, true),
			W.button("undo:" .. exercise.id, "Undo", function()
				remove_built(#S.built)
				S.feedback = nil
			end, false),
			W.button("clear:" .. exercise.id, "Clear", function()
				S.built, S.used, S.feedback, S.drag, S.drop = {}, {}, nil, nil, nil
			end, false),
		}),
	})
end

local function typed_activity(exercise)
	return ui.col({
		gap = 12,
		ui.input({
			id = "answer:" .. exercise.id,
			value = S.draft,
			on_input = function(value)
				S.draft = value
				S.feedback = nil
			end,
			on_enter = function()
				if S.draft == "" then
					nudge("Type a short answer before checking.")
				else
					respond(exercise, Model.typed_correct(exercise, S.draft))
				end
			end,
			w_full = true,
			h = 48,
			px = 14,
			radius = 12,
			fill = C.paper,
			stroke = { 1, C.line_strong },
			color = C.ink,
			font_size = 14,
		}),
		ui.text({ exercise.placeholder .. " · capital letters are okay", color = C.faint, font_size = 10 }),
		ui.row({
			wrap = true,
			gap = 9,
			W.button("check:" .. exercise.id, "Check answer", function()
				if S.draft == "" then
					nudge("Type a short answer before checking.")
					return
				end
				respond(exercise, Model.typed_correct(exercise, S.draft))
			end, true),
		}),
	})
end

local function feedback(exercise)
	if not S.feedback then return false end
	local good = S.feedback.kind == "correct"
	local nudge_only = S.feedback.kind == "nudge"
	local fill = good and C.green_soft or (nudge_only and C.gold_soft or C.wrong_soft)
	local ink = good and C.green or (nudge_only and C.gold or C.wrong)
	return ui.col({
		id = "feedback:" .. exercise.id,
		gap = 5,
		pad = 13,
		radius = 12,
		fill = fill,
		stroke = { 1, ink },
		fade_in = 140,
		slide_in = { { 0, 6 }, 140 },
		ui.text({ S.feedback.title, color = ink, font_size = 13, no_wrap = true }),
		ui.text({ S.feedback.body, color = C.ink, font_size = 12 }),
		good and ui.row({
			mt = 4,
			W.button("next:" .. exercise.id, S.class == 5 and S.activity == 3 and "Replay anytime" or "Next activity →", function()
				if S.class == 5 and S.activity == 3 then
					reset_attempt()
				else
					next_activity()
				end
			end, false),
		}) or false,
	})
end

local function exercise_card(level, exercise)
	local body
	if exercise.kind == "choose" then
		body = choose_activity(exercise)
	elseif exercise.kind == "build" then
		body = build_activity(exercise)
	else
		body = typed_activity(exercise)
	end
	local done = Model.mastered(exercise.id)
	return ui.col({
		id = "exercise:" .. exercise.id,
		grow = 2,
		min_w = 300,
		gap = 17,
		pad = 20,
		radius = C.radius,
		fill = C.paper,
		stroke = { 1, C.line },
		fade_in = 180,
		ui.row({
			wrap = true,
			gap = 9,
			align_center = true,
			kind_badge(exercise),
			done and W.badge("✓ MASTERED", C.green_soft, C.green) or false,
		}),
		ui.col({
			gap = 6,
			ui.text({ exercise.title, color = C.ink, font_size = 22 }),
			ui.text({ exercise.prompt, color = C.muted, font_size = 15 }),
		}),
		ui.col({ h = 1, w_full = true, fill = C.line }),
		body,
		feedback(exercise),
	})
end

local function guide_card(level, exercise)
	local steps = {
		choose = { "Read the whole question", "Choose the best answer", "Use the clue and try again" },
		build = { "Drag a card into any open slot", "Move or remove placed cards", "Check, use the clue, and retry" },
		type = { "Type only the missing idea", "Spelling matters; case does not", "Your answer is never saved" },
	}
	local rows = {}
	for i, text in ipairs(steps[exercise.kind]) do
		rows[#rows + 1] = ui.row({
			gap = 10,
			align_center = true,
			W.badge(tostring(i), C.paper, C.muted),
			ui.col({ grow = true, ui.text({ text, color = C.muted, font_size = 11 }) }),
		})
	end
	return ui.col({
		grow = true,
		min_w = 230,
		max_w = 310,
		gap = 13,
		pad = 17,
		radius = C.radius,
		fill = level.colour .. "12",
		stroke = { 1, level.colour .. "55" },
		ui.text({ "A gentle learning loop", color = C.ink, font_size = 15 }),
		rows,
		ui.col({ h = 1, w_full = true, fill = level.colour .. "44" }),
		ui.text({ "A correct answer explains why. A missed answer offers a clue, never a penalty.", color = C.muted, font_size = 11 }),
	})
end

local function header()
	local total = Model.completed_total()
	return ui.col({
		w_full = true,
		align_center = true,
		fill = C.navy,
		ui.row({
			w_full = true,
			max_w = C.content_w,
			wrap = true,
			gap = 18,
			px = 20,
			py = 18,
			align_center = true,
			ui.col({
				grow = true,
				min_w = 260,
				gap = 3,
				ui.text({ "BHASHA STEPS", color = "#f2b67f", font_size = 10, no_wrap = true }),
				ui.text({ "Small steps. Stronger English.", color = C.white, font_size = 22 }),
			}),
			ui.col({
				gap = 5,
				ui.text({ "LOCAL PROGRESS", color = "#aebdca", font_size = 9, no_wrap = true }),
				W.progress(total, 15),
			}),
		}),
	})
end

return function()
	local level, exercise = current()
	local class_buttons = {}
	for i, item in ipairs(Content.classes) do
		class_buttons[#class_buttons + 1] = W.class_button(item, i == S.class, Model.completed_in(item), function()
			open(i, 1)
		end)
	end
	local activities = {}
	for i, item in ipairs(level.exercises) do
		activities[#activities + 1] = W.activity_button(item, i == S.activity, Model.mastered(item.id), i, function()
			open(S.class, i)
		end)
	end
	local ghost = S.drag and ui.row({
		absolute = true,
		left = S.drag.x,
		top = S.drag.y,
		scale = S.drag.scale or 1,
		opacity = 0.9,
		px = 11,
		py = 7,
		radius = 9,
		fill = C.navy_hi,
		stroke = { 1, C.saffron },
		ui.text({ S.drag.word, color = C.white, font_size = 13, no_wrap = true }),
	}) or false

	return ui.col({
		id = "bhasha-steps",
		full = true,
		align_center = true,
		fill = C.bg,
		header(),
		ui.col({
			id = "bhasha-scroll",
			w_full = true,
			grow = true,
			scroll_y = true,
			align_center = true,
			ui.col({
				w_full = true,
				max_w = C.content_w,
			gap = 15,
			px = 20,
			py = 18,
			ui.row({
				wrap = true,
				gap = 9,
				align_center = true,
				ui.text({ "Choose your class", color = C.ink, font_size = 14, no_wrap = true }),
				ui.text({ "Progress stays on this device.", color = C.muted, font_size = 11 }),
			}),
			ui.row({ wrap = true, gap = 8, stretch = true, class_buttons }),
			ui.row({ wrap = true, gap = 8, stretch = true, activities }),
			ui.row({
				wrap = true,
				gap = 14,
				stretch = true,
				exercise_card(level, exercise),
				guide_card(level, exercise),
			}),
				ui.row({
					wrap = true,
					gap = 8,
					py = 7,
					ui.text({ "English practice today.", color = C.ink, font_size = 11, no_wrap = true }),
					ui.text({ "Hindi lessons are a future content path; typed-Hindi grading is not supported.", color = C.muted, font_size = 11 }),
				}),
			}),
		}),
		ghost,
	})
end
