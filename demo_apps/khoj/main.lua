local C = require("theme")
local Content = require("content")
local Diagrams = require("diagrams")

local progress = doc:open("khoj-progress")
if not progress.progress then
	progress:set({ "progress" }, doc.map({}))
end

local kinds = { "classify", "sequence", "quiz" }
local kind_labels = {
	classify = "Sort & discover",
	sequence = "Put in order",
	quiz = "Explore a diagram",
}
local kind_notes = {
	classify = "Notice useful properties",
	sequence = "Follow how things change",
	quiz = "Look closely and choose",
}

local S = {
	class = 1,
	kind = "classify",
	class_answers = {},
	order = {},
	quiz_answer = nil,
	circuit = 0,
	circuit_drag = nil,
	feedback = nil,
}

local function reset_attempt()
	S.class_answers = {}
	S.order = {}
	S.quiz_answer = nil
	S.circuit = 0
	S.circuit_drag = nil
	S.feedback = nil
end

local function open_exercise(class, kind)
	S.class = class
	S.kind = kind
	reset_attempt()
end

local function mastered(id)
	local entry = progress.progress and progress.progress[id]
	return entry and entry.mastered == true
end

local function mastered_count(class)
	local count = 0
	for _, exercise in ipairs(Content.all) do
		if (not class or exercise.class == class) and mastered(exercise.id) then
			count = count + 1
		end
	end
	return count
end

local function record_check(exercise, correct)
	if correct then
		progress:set({ "progress", exercise.id }, doc.map({ mastered = true }))
	end
end

local function set_feedback(ok, title, body)
	S.feedback = { ok = ok, title = title, body = body }
end

local function text(label, color, size, nowrap)
	return ui.text({
		label,
		color = color or C.ink,
		font_size = size or 13,
		no_wrap = nowrap == true,
	})
end

local function pill(id, label, active, on_click)
	return ui.button({
		id = id,
		h = 38,
		px = 16,
		no_shrink = true,
		center = true,
		radius = 19,
		fill = active and C.green or C.paper,
		stroke = { 1, active and C.green or C.line },
		hover_fill = active and C.green_hi or C.green_soft,
		press_scale = 0.96,
		tint = 100,
		on_click = on_click,
		text(label, active and C.white or C.ink, 13, true),
	})
end

local function action_button(id, label, on_click, primary)
	return ui.button({
		id = id,
		h = 40,
		px = 20,
		no_shrink = true,
		center = true,
		radius = 10,
		fill = primary and C.saffron or C.paper,
		stroke = { 1, primary and C.saffron or C.line },
		hover_fill = primary and C.saffron_hi or C.saffron_soft,
		press_scale = 0.96,
		tint = 100,
		on_click = on_click,
		text(label, primary and C.white or C.ink, 13, true),
	})
end

local function status_badge(label, done)
	return ui.row({
		h = 28,
		px = 11,
		center = true,
		no_shrink = true,
		radius = 14,
		fill = done and C.green_soft or C.blue_soft,
		text(label, done and C.green or C.blue, 11, true),
	})
end

local function feedback_card()
	if not S.feedback then
		return false
	end
	local good = S.feedback.ok
	return ui.row({
		id = "feedback-card",
		w_full = true,
		wrap = true,
		gap = 12,
		px = 16,
		py = 13,
		align_center = true,
		radius = 12,
		fill = good and C.green_soft or C.saffron_soft,
		stroke = { 1, good and "#b8dfcc" or "#edc999" },
		fade_in = 120,
		ui.col({
			w = 34,
			h = 34,
			center = true,
			no_shrink = true,
			radius = 17,
			fill = good and C.green or C.saffron,
			text(good and "✓" or "?", C.white, 17, true),
		}),
		ui.col({
			grow = true,
			min_w = 220,
			gap = 3,
			text(S.feedback.title, good and C.green or C.saffron, 14, true),
			text(S.feedback.body, C.ink, 12, false),
		}),
	})
end

local function check_row(exercise, check)
	local done = mastered(exercise.id)
	return ui.row({
		w_full = true,
		wrap = true,
		gap = 10,
		align_center = true,
		ui.col({ grow = true, min_w = 20 }),
		done and status_badge("Mastered • replay anytime", true) or false,
		action_button("check:" .. exercise.id, done and "Check again" or "Check my work", check, true),
	})
end

local function classification(exercise)
	local rows = {}
	for _, item in ipairs(exercise.items) do
		local choices = {}
		for _, category in ipairs(exercise.categories) do
			local selected = S.class_answers[item.id] == category.id
			choices[#choices + 1] = ui.button({
				id = "answer:" .. exercise.id .. ":" .. item.id .. ":" .. category.id,
				h = 34,
				px = 12,
				center = true,
				no_shrink = true,
				radius = 9,
				fill = selected and C.blue or C.paper,
				stroke = { 1, selected and C.blue or C.line },
				hover_fill = selected and "#347eaa" or C.blue_soft,
				press_scale = 0.97,
				on_click = function()
					S.class_answers[item.id] = category.id
					S.feedback = nil
				end,
				text(category.label, selected and C.white or C.ink, 12, true),
			})
		end
		rows[#rows + 1] = ui.row({
			w_full = true,
			wrap = true,
			gap = 10,
			px = 14,
			py = 11,
			align_center = true,
			radius = 11,
			fill = C.paper_warm,
			stroke = { 1, C.line },
			ui.col({ grow = true, min_w = 170, text(item.label, C.ink, 14, true) }),
			choices,
		})
	end

	local function check()
		for _, item in ipairs(exercise.items) do
			if not S.class_answers[item.id] then
				set_feedback(false, "A few are still waiting", "Choose a group for every item, then check again.")
				return
			end
		end
		local correct = true
		for _, item in ipairs(exercise.items) do
			if S.class_answers[item.id] ~= item.answer then
				correct = false
				break
			end
		end
		record_check(exercise, correct)
		if correct then
			set_feedback(true, "Thoughtful sorting!", exercise.explanation)
		else
			set_feedback(false, "Good try — inspect one property", exercise.clue)
		end
	end

	return ui.col({
		w_full = true,
		gap = 10,
		rows,
		feedback_card(),
		check_row(exercise, check),
	})
end

local function sequence(exercise)
	local selected = {}
	for _, id in ipairs(S.order) do
		selected[id] = true
	end

	local ordered = {}
	if #S.order == 0 then
		ordered[1] = ui.col({
			w_full = true,
			h = 62,
			center = true,
			radius = 11,
			fill = C.paper_warm,
			stroke_dash = { 1, C.line, 7, 5 },
			text("Your process will appear here", C.muted, 12, true),
		})
	else
		for index, id in ipairs(S.order) do
			local label = id
			for _, step in ipairs(exercise.steps) do
				if step.id == id then label = step.label end
			end
			ordered[#ordered + 1] = ui.button({
				id = "ordered:" .. exercise.id .. ":" .. id,
				w_full = true,
				min_h = 48,
				gap = 12,
				px = 13,
				py = 8,
				align_center = true,
				radius = 11,
				fill = C.green_soft,
				hover_fill = "#d1ebdf",
				press_scale = 0.99,
				on_click = function()
					for i = #S.order, index, -1 do table.remove(S.order, i) end
					S.feedback = nil
				end,
				ui.col({ w = 30, h = 30, center = true, no_shrink = true, radius = 15, fill = C.green, text(tostring(index), C.white, 12, true) }),
				ui.col({ grow = true, min_w = 140, text(label, C.ink, 13, false) }),
				text("change from here", C.green, 10, true),
			})
		end
	end

	local available = {}
	for _, step in ipairs(exercise.steps) do
		if not selected[step.id] then
			available[#available + 1] = ui.button({
				id = "step:" .. exercise.id .. ":" .. step.id,
				min_h = 40,
				px = 14,
				py = 8,
				center = true,
				no_shrink = true,
				radius = 10,
				fill = C.paper,
				stroke = { 1, C.line },
				hover_fill = C.green_soft,
				press_scale = 0.97,
				on_click = function()
					S.order[#S.order + 1] = step.id
					S.feedback = nil
				end,
				text(step.label, C.ink, 12, true),
			})
		end
	end
	if #available == 0 then
		available[1] = text("All steps placed • tap a placed step to revise from there", C.muted, 11, false)
	end

	local function check()
		if #S.order < #exercise.answer then
			set_feedback(false, "The chain is not complete yet", "Place every step before checking the process.")
			return
		end
		local correct = true
		for i, id in ipairs(exercise.answer) do
			if S.order[i] ~= id then correct = false break end
		end
		record_check(exercise, correct)
		if correct then
			set_feedback(true, "The whole process connects!", exercise.explanation)
		else
			set_feedback(false, "Nearly — revisit the starting point", exercise.clue)
		end
	end

	return ui.col({
		w_full = true,
		gap = 12,
		ui.row({ w_full = true, align_center = true, text("Your order", C.muted, 11, true), ui.col({ grow = true }), text(tostring(#S.order) .. " / " .. tostring(#exercise.answer), C.green, 11, true) }),
		ui.col({ w_full = true, gap = 7, ordered }),
		text("Steps to place", C.muted, 11, true),
		ui.row({ w_full = true, wrap = true, gap = 8, available }),
		feedback_card(),
		check_row(exercise, check),
	})
end

local function circuit_lab(exercise)
	local closed = S.circuit >= 0.96
	local gap = math.floor((1 - S.circuit) * 100 + 0.5)

	local function drag(e)
		if e.phase == "start" then
			if e.shape ~= "switch-handle" then return end
			S.circuit_drag = S.circuit
		elseif S.circuit_drag then
			S.circuit = math.max(0, math.min(1, S.circuit_drag + e.dx / 80))
			S.feedback = nil
			if e.phase == "end" then S.circuit_drag = nil end
		end
	end

	local function check()
		if closed then
			record_check(exercise, true)
			set_feedback(true, "Circuit complete — the bulb lights!", exercise.explanation)
		else
			set_feedback(false, "There is still a gap", "Drag the green contact until it touches the orange terminal, then check.")
		end
	end

	return ui.col({
		w_full = true,
		gap = 12,
		ui.col({
			id = "diagram-scroll:" .. exercise.id,
			w_full = true,
			scroll_x = true,
			ui.row({
				w_full = true,
				min_w = 520,
				no_shrink = true,
				center = true,
				ui.frame({
					id = "circuit-lab:" .. exercise.id,
					visual = Diagrams.circuit_lab(S.circuit),
					on_drag = drag,
				}),
			}),
		}),
		ui.row({
			w_full = true,
			wrap = true,
			gap = 8,
			align_center = true,
			status_badge(closed and "CLOSED • current flows" or "OPEN • no current", closed),
			text(closed and "The metal contacts touch, so the path has no break." or "Contact gap: " .. tostring(gap) .. "% — drag the green handle right.", closed and C.green or C.muted, 11, false),
		}),
		text("Experiment: move the switch slowly. The gap changes continuously, but the bulb turns on only when the conducting path is fully closed.", C.ink, 12, false),
		feedback_card(),
		check_row(exercise, check),
	})
end

local function diagram_quiz(exercise)
	if exercise.diagram == "circuit-parts" then return circuit_lab(exercise) end

	local function choose(id)
		S.quiz_answer = id
		S.feedback = nil
	end

	local labels = {}
	for _, choice in ipairs(exercise.choices) do
		local selected = S.quiz_answer == choice.id
		labels[#labels + 1] = ui.button({
			id = "diagram-label:" .. exercise.id .. ":" .. choice.id,
			w = 160,
			min_h = 38,
			px = 8,
			center = true,
			no_shrink = true,
			radius = 9,
			fill = selected and C.blue or C.paper,
			stroke = { 1, selected and C.blue or C.line },
			hover_fill = selected and "#347eaa" or C.blue_soft,
			press_scale = 0.97,
			on_click = function() choose(choice.id) end,
			text(choice.label, selected and C.white or C.ink, 12, true),
		})
	end

	local function check()
		if not S.quiz_answer then
			set_feedback(false, "Choose one picture first", "Tap a picture or its label, then check your idea.")
			return
		end
		local correct = S.quiz_answer == exercise.answer
		record_check(exercise, correct)
		if correct then
			set_feedback(true, "Sharp observation!", exercise.explanation)
		else
			set_feedback(false, "Look once more", exercise.clue)
		end
	end

	return ui.col({
		w_full = true,
		gap = 12,
		ui.col({
			id = "diagram-scroll:" .. exercise.id,
			w_full = true,
			scroll_x = true,
			ui.row({
				w_full = true,
				min_w = 520,
				no_shrink = true,
				center = true,
				ui.col({
					w = 520,
					no_shrink = true,
					gap = 8,
					ui.frame({
						id = "diagram:" .. exercise.id,
						visual = Diagrams[exercise.diagram],
						on_click = function(e)
							if e.shape and e.shape:sub(1, 7) == "choice:" then
								choose(e.shape:sub(8))
							end
						end,
					}),
					ui.row({ w = 520, gap = 20, labels }),
				}),
			}),
		}),
		text("Tap the illustration itself — each picture is an interactive shape.", C.muted, 10, false),
		feedback_card(),
		check_row(exercise, check),
	})
end

local function activity_tab(kind)
	local active = S.kind == kind
	local exercise = Content.get(S.class, kind)
	local done = mastered(exercise.id)
	return ui.button({
		id = "activity:" .. kind,
		grow = true,
		min_w = 190,
		min_h = 66,
		gap = 10,
		px = 14,
		py = 10,
		align_center = true,
		radius = 12,
		fill = active and C.green_soft or C.paper,
		stroke = { 1, active and C.green or C.line },
		hover_fill = C.green_soft,
		press_scale = 0.98,
		on_click = function() open_exercise(S.class, kind) end,
		ui.col({
			grow = true,
			min_w = 120,
			gap = 2,
			text(kind_labels[kind], active and C.green or C.ink, 13, true),
			text(kind_notes[kind], C.muted, 10, true),
		}),
		done and status_badge("✓", true) or false,
	})
end

local function header()
	local classes = {}
	for class = 1, 5 do
		local done = mastered_count(class)
		classes[#classes + 1] = pill(
			"class:" .. tostring(class),
			"Class " .. tostring(class) .. "  ·  " .. tostring(done) .. "/3",
			S.class == class,
			function() open_exercise(class, S.kind) end
		)
	end
	return ui.col({
		w_full = true,
		fill = C.paper,
		stroke = { 1, C.line },
		ui.row({
			w_full = true,
			wrap = true,
			gap = 14,
			px = 24,
			py = 16,
			align_center = true,
			ui.col({ w = 44, h = 44, center = true, no_shrink = true, radius = 14, fill = C.saffron, text("K", C.white, 22, true) }),
			ui.col({
				grow = true,
				min_w = 220,
				gap = 2,
				text("Khoj", C.ink, 21, true),
				text("Science & EVS • observe, order, explain", C.muted, 11, true),
			}),
			status_badge(tostring(mastered_count()) .. " of 15 mastered", mastered_count() == 15),
		}),
		ui.row({ w_full = true, wrap = true, gap = 8, px = 24, mb = 14, classes }),
	})
end

return function()
	local exercise = Content.get(S.class, S.kind)
	local activity
	if exercise.kind == "classify" then
		activity = classification(exercise)
	elseif exercise.kind == "sequence" then
		activity = sequence(exercise)
	else
		activity = diagram_quiz(exercise)
	end

	local tabs = {}
	for _, kind in ipairs(kinds) do tabs[#tabs + 1] = activity_tab(kind) end

	return ui.col({
		id = "khoj",
		full = true,
		fill = C.bg,
		header(),
		ui.col({
			id = "lesson-scroll",
			grow = true,
			w_full = true,
			scroll_y = true,
			align_center = true,
			px = 22,
			py = 18,
			ui.col({
				w_full = true,
				max_w = 940,
				gap = 14,
				ui.row({ w_full = true, wrap = true, stretch = true, gap = 10, tabs }),
				ui.col({
					id = "exercise:" .. exercise.id,
					w_full = true,
					gap = 15,
					pad = 20,
					radius = 16,
					fill = C.paper,
					stroke = { 1, C.line },
					fade_in = 120,
					ui.row({
						w_full = true,
						wrap = true,
						gap = 10,
						align_center = true,
						ui.col({
							grow = true,
							min_w = 240,
							gap = 4,
							text("CLASS " .. tostring(exercise.class) .. "  •  " .. string.upper(exercise.tab), C.saffron, 10, true),
							text(exercise.title, C.ink, 22, false),
							text(exercise.prompt, C.muted, 13, false),
						}),
						mastered(exercise.id) and status_badge("✓ Completed", true) or status_badge("Ready to explore", false),
					}),
					ui.col({ h = 1, w_full = true, fill = C.line }),
					activity,
				}),
				ui.row({
					w_full = true,
					wrap = true,
					gap = 8,
					px = 4,
					text("Khoj means exploration.", C.green, 11, true),
					text("Your work stays on this device; only activity progress is saved.", C.muted, 11, false),
				}),
			}),
		}),
	})
end
