local C = require("theme")
local Content = require("content")
local model = require("model")
local W = require("ui/widgets")
local Manipulation = require("ui/manipulation")

local S = model.state

local function class_selector()
	local buttons = {}
	for class_no = 1, 5 do
		local selected = S.class_no == class_no
		local done = model.completed_count(class_no)
		buttons[#buttons + 1] = ui.button({
			id = "class:" .. class_no,
			h = 48,
			px = 16,
			gap = 8,
			center = true,
			no_shrink = true,
			radius = 14,
			fill = selected and C.ink or C.paper,
			hover_fill = selected and "#34415c" or C.saffron_soft,
			press_scale = 0.97,
			stroke = { 1, selected and C.ink or C.line },
			on_click = function()
				model.select_class(class_no)
			end,
			ui.text({ "Class " .. class_no, color = selected and "#ffffff" or C.ink, font_size = 13, no_wrap = true }),
			ui.row({
				px = 7,
				py = 2,
				radius = 9,
				fill = selected and C.saffron or C.saffron_soft,
				ui.text({ done .. "/3", color = selected and "#ffffff" or C.saffron, font_size = 10, no_wrap = true }),
			}),
		})
	end
	return ui.row({ id = "class-selector", gap = 9, wrap = true, buttons })
end

local function activity_selector()
	local tabs = {}
	for _, kind in ipairs(Content.kinds) do
		local meta = Content.kind_meta[kind]
		local selected = S.kind == kind
		tabs[#tabs + 1] = ui.button({
			id = "activity:" .. kind,
			grow = true,
			min_w = 150,
			h = 56,
			px = 15,
			gap = 11,
			align_center = true,
			radius = 14,
			fill = selected and C.teal_soft or C.paper_warm,
			hover_fill = C.teal_soft,
			press_scale = 0.98,
			stroke = { 1, selected and C.teal or C.line },
			on_click = function()
				model.select_kind(kind)
			end,
			ui.row({
				w = 34,
				h = 30,
				center = true,
				radius = 9,
				fill = selected and C.teal or C.paper,
				ui.text({ meta.glyph, color = selected and "#ffffff" or C.teal, font_size = 10, no_wrap = true }),
			}),
			ui.col({
				gap = 2,
				ui.text({ meta.short, color = C.ink, font_size = 13, no_wrap = true }),
				ui.text({ meta.label, color = selected and C.teal or C.muted, font_size = 10, no_wrap = true }),
			}),
		})
	end
	return ui.row({ gap = 10, wrap = true, stretch = true, tabs })
end

local function choice_body(exercise)
	local options = {}
	for index, option in ipairs(exercise.options) do
		local selected = S.picked == option.id
		options[#options + 1] = ui.button({
			id = "choose:" .. exercise.id .. ":" .. option.id,
			w_full = true,
			min_h = 52,
			px = 14,
			py = 10,
			gap = 12,
			align_center = true,
			radius = 14,
			fill = selected and C.blue_soft or C.paper_warm,
			hover_fill = C.blue_soft,
			press_scale = 0.985,
			stroke = { selected and 2 or 1, selected and C.blue or C.line },
			on_click = function()
				S.picked = option.id
				S.feedback = nil
			end,
			ui.row({
				w = 30,
				h = 30,
				center = true,
				radius = 15,
				fill = selected and C.blue or C.paper,
				stroke = { 1, selected and C.blue or C.line_strong },
				ui.text({ string.char(64 + index), color = selected and "#ffffff" or C.muted, font_size = 12, no_wrap = true }),
			}),
			ui.text({ option.label, color = C.ink, font_size = 14 }),
		})
	end
	return ui.col({ gap = 10, options, W.button("check:" .. exercise.id, "Check my choice", model.check_choice, "primary") })
end

local function numeric_body(exercise)
	return ui.col({
		gap = 14,
		ui.row({
			w_full = true,
			min_h = 82,
			center = true,
			radius = 16,
			fill = C.pink_soft,
			stroke = { 1, "#f5c7d3" },
			ui.text({ exercise.equation, color = C.pink, font_size = 30, no_wrap = true }),
		}),
		ui.row({
			gap = 10,
			wrap = true,
			align_center = true,
			ui.input({
				id = "numeric-input:" .. exercise.id,
				value = S.numeric_draft,
				w = 210,
				h = 46,
				px = 14,
				radius = 12,
				fill = C.paper_warm,
				stroke = { 1, C.line_strong },
				color = C.ink,
				font_size = 17,
				on_input = function(value)
					S.numeric_draft = value
					S.feedback = nil
				end,
				on_enter = function()
					model.check_numeric(S.numeric_draft)
				end,
			}),
			W.label(exercise.suffix, C.muted, 12),
			W.button("check:" .. exercise.id, "Check my answer", function()
				model.check_numeric(S.numeric_draft)
			end, "primary"),
		}),
	})
end

local function manipulation_body(exercise)
	return Manipulation.view(exercise, model, S)
end

local function feedback(exercise)
	if not S.feedback then
		return ui.row({
			id = "feedback:" .. exercise.id,
			min_h = 48,
			px = 14,
			py = 10,
			gap = 9,
			align_center = true,
			radius = 13,
			fill = C.saffron_soft,
			ui.text({ "Tip", color = C.saffron, font_size = 11, no_wrap = true }),
			ui.text({ "Take your time. You can try as often as you like.", color = C.muted, font_size = 12 }),
		})
	end
	local correct = S.feedback.correct
	return ui.col({
		id = "feedback:" .. exercise.id,
		gap = 8,
		px = 15,
		py = 13,
		radius = 14,
		fill = correct and C.green_soft or C.red_soft,
		stroke = { 1, correct and "#b8dfc7" or "#f3c4c4" },
		fade_in = 120,
		ui.text({ correct and "Wonderful work!" or "Not yet — try this clue", color = correct and C.green or C.red, font_size = 13, no_wrap = true }),
		ui.text({ S.feedback.text, color = C.ink, font_size = 12 }),
		correct and W.button("next-activity", "Continue to the next activity", model.next_activity, "teal") or false,
	})
end

local function exercise_panel(exercise)
	local body
	if S.kind == "choose" then
		body = choice_body(exercise)
	elseif S.kind == "numeric" then
		body = numeric_body(exercise)
	else
		body = manipulation_body(exercise)
	end
	local done = model.mastered(exercise.id)
	return ui.col({
		id = "exercise:" .. exercise.id,
		grow = 2,
		min_w = 240,
		gap = 17,
		pad = 22,
		radius = 20,
		fill = C.paper,
		stroke = { 1, C.line },
		ui.row({
			gap = 9,
			wrap = true,
			align_center = true,
			W.badge("CLASS " .. S.class_no, "saffron"),
			W.badge(Content.kind_meta[S.kind].label, "pink"),
			ui.col({ grow = true }),
			done and W.badge("Mastered · replaying", "green") or W.badge("Ready to explore", "blue"),
		}),
		ui.col({
			gap = 7,
			ui.text({ exercise.title, color = C.ink, font_size = 24 }),
			ui.text({ exercise.prompt, color = C.muted, font_size = 14 }),
		}),
		W.divider(),
		body,
		feedback(exercise),
	})
end

local function progress_card()
	local total = model.total_completed()
	local dots = {}
	for class_no = 1, 5 do
		for _, kind in ipairs(Content.kinds) do
			local ex = Content.get(class_no, kind)
			local done = model.mastered(ex.id)
			dots[#dots + 1] = ui.col({
				id = "progress:" .. ex.id,
				w = 13,
				h = 13,
				radius = 7,
				fill = done and C.green or C.line,
			})
		end
	end
	local rows = {}
	for class_no = 1, 5 do
		local done = model.completed_count(class_no)
		rows[#rows + 1] = ui.row({
			align_center = true,
			ui.text({ "Class " .. class_no, color = class_no == S.class_no and "#ffffff" or "#aeb9ce", font_size = 12, no_wrap = true }),
			ui.col({ grow = true }),
			ui.text({ done .. " of 3", color = done == 3 and C.green or C.muted, font_size = 11, no_wrap = true }),
		})
	end
	return ui.col({
		grow = true,
		min_w = 230,
		max_w = 320,
		gap = 14,
		pad = 19,
		radius = 20,
		fill = C.ink,
		ui.text({ "Your mela map", color = "#ffffff", font_size = 18, no_wrap = true }),
		ui.text({ total .. " of 15 activities mastered", color = "#c9d1e2", font_size = 12 }),
		ui.row({ gap = 6, wrap = true, dots }),
		ui.col({ h = 1, w_full = true, fill = "#3b4762" }),
		ui.col({ gap = 10, rows }),
		ui.col({
			mt = 3,
			pad = 12,
			gap = 4,
			radius = 13,
			fill = "#34415c",
			ui.text({ "Gentle practice", color = "#ffffff", font_size = 12, no_wrap = true }),
			ui.text({ "No timer, no score race. Every activity stays open for another try.", color = "#c9d1e2", font_size = 11 }),
		}),
	})
end

local function header()
	local total = model.total_completed()
	return ui.col({
		gap = 16,
		ui.row({
			gap = 14,
			wrap = true,
			align_center = true,
			ui.row({
				w = 52,
				h = 52,
				center = true,
				radius = 18,
				fill = C.saffron,
				ui.text({ "×+", color = "#ffffff", font_size = 18, no_wrap = true }),
			}),
			ui.col({
				gap = 2,
				ui.text({ "MATH MELA", color = C.saffron, font_size = 11, no_wrap = true }),
				ui.text({ "Small puzzles. Bright ideas.", color = C.ink, font_size = 25 }),
				ui.text({ "A playful mathematics journey for Classes 1–5", color = C.muted, font_size = 12 }),
			}),
			ui.col({ grow = true }),
			ui.row({
				gap = 9,
				px = 14,
				py = 9,
				align_center = true,
				radius = 14,
				fill = C.green_soft,
				ui.text({ "✓", color = C.green, font_size = 15, no_wrap = true }),
				ui.text({ total .. "/15 complete", color = C.green, font_size = 12, no_wrap = true }),
			}),
		}),
		class_selector(),
		activity_selector(),
	})
end

return function()
	local exercise = model.current()
	return ui.col({
		id = "math-mela",
		full = true,
		scroll_y = true,
		fill = C.bg,
		ui.row({ h = 7, w_full = true, fill = C.saffron }),
		ui.col({
			w_full = true,
			gap = 20,
			px = 26,
			py = 22,
			header(),
			ui.row({
				gap = 18,
				wrap = true,
				stretch = true,
				exercise_panel(exercise),
				progress_card(),
			}),
			ui.row({
				w_full = true,
				center = true,
				py = 8,
				ui.text({ "Progress stays on this device. Answers and personal details are never saved.", color = C.muted, font_size = 10 }),
			}),
		}),
	})
end
