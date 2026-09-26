local C = require("theme")
local M = require("model")
local compass = require("compass")
local terrain = require("terrain")

local S = M.state
local mechanic_names = { choose = "Choose", match = "Match", route = "Navigate" }
local mechanic_notes = {
	choose = "Choose one answer, then check your thinking.",
	match = "Connect each card on the left with one card on the right.",
	route = "Use north, east, south, and west to move the traveller.",
}

local function text(label, color, size, no_wrap)
	return ui.text({ label, color = color or C.ink, font_size = size or 13, no_wrap = no_wrap or false })
end

local function button(spec)
	return ui.button({
		id = spec.id,
		min_h = spec.h or 42,
		min_w = spec.min_w,
		grow = spec.grow,
		w_full = spec.w_full,
		px = spec.px or 15,
		py = spec.py or 10,
		gap = spec.gap or 8,
		radius = spec.radius or 10,
		align_center = true,
		center = spec.center,
		fill = spec.fill or C.panel,
		hover_fill = spec.hover_fill or C.panel_warm,
		press_fill = spec.press_fill,
		stroke = { spec.stroke_w or 1, spec.stroke or C.line },
		hover_stroke = { 1, spec.hover_stroke or C.saffron },
		press_scale = 0.975,
		tint = 100,
		on_click = spec.on_click,
		spec.children,
	})
end

local function pill(label, fill, color)
	return ui.row({
		no_shrink = true,
		px = 10,
		py = 5,
		radius = 20,
		fill = fill,
		text(label, color, 11, true),
	})
end

local function header()
	local done = M.total_mastered()
	return ui.row({
		w_full = true,
		wrap = true,
		gap = 14,
		px = 22,
		py = 15,
		align_center = true,
		fill = C.panel,
		ui.frame({ visual = compass, no_shrink = true }),
		ui.col({
			gap = 3,
			grow = true,
			min_w = 250,
			text("BHARAT COMPASS", C.green, 11, true),
			text("Explore places. Read clues. Find your way.", C.ink, 20),
			text("Geography and EVS practice for Classes 1-5", C.muted, 12),
		}),
		ui.col({
			w = 190,
			gap = 7,
			ui.row({
				align_center = true,
				text("Journey progress", C.muted, 11, true),
				ui.col({ grow = true }),
				text(done .. " / 15", C.green, 12, true),
			}),
			ui.row({
				w = 190,
				h = 7,
				radius = 4,
				fill = C.bg_deep,
				ui.col({ w = 190 * done / 15, h = 7, radius = 4, fill = C.green }),
			}),
		}),
	})
end

local function class_picker()
	local children = {}
	for i, level in ipairs(M.content.classes) do
		local active = i == S.class
		local done = M.mastered_count(i)
		children[#children + 1] = button({
			id = "class:" .. i,
			min_w = 135,
			grow = true,
			fill = active and C.green or C.panel,
			hover_fill = active and C.green_hi or C.green_soft,
			stroke = active and C.green or C.line,
			hover_stroke = C.green,
			on_click = function() M.go(i, 1) end,
			children = {
				ui.col({
					gap = 2,
					grow = true,
					text("Class " .. i, active and C.white or C.ink, 13, true),
					text(level.name, active and "#dff2e8" or C.muted, 10, true),
				}),
				pill(done .. "/3", active and "#ffffff22" or C.green_soft, active and C.white or C.green),
			},
		})
	end
	return ui.row({ wrap = true, gap = 8, stretch = true, children })
end

local function activity_picker(level)
	local children = {}
	for i, exercise in ipairs(level.exercises) do
		local active = i == S.activity
		local done = M.mastered(exercise.id)
		children[#children + 1] = button({
			id = "activity:" .. exercise.id,
			min_w = 190,
			grow = true,
			fill = active and C.saffron_soft or C.panel,
			hover_fill = C.saffron_soft,
			stroke = active and C.saffron or C.line_soft,
			hover_stroke = C.saffron,
			on_click = function() M.go(S.class, i) end,
			children = {
				pill(tostring(i), active and C.saffron or C.bg_deep, active and C.white or C.muted),
				ui.col({
					grow = true,
					gap = 2,
					text(mechanic_names[exercise.kind], C.ink, 12, true),
					text(exercise.title, C.muted, 10, true),
				}),
				done and pill("Complete", C.green_soft, C.green) or false,
			},
		})
	end
	return ui.row({ wrap = true, gap = 8, stretch = true, children })
end

local function choice_view(exercise)
	local options = {}
	for _, option in ipairs(exercise.options) do
		local selected = S.choice == option.id
		options[#options + 1] = button({
			id = "option:" .. exercise.id .. ":" .. option.id,
			min_w = C.choice_min,
			grow = true,
			fill = selected and C.blue_soft or C.panel,
			hover_fill = C.blue_soft,
			stroke = selected and C.blue or C.line,
			hover_stroke = C.blue,
			on_click = function() M.choose(option.id) end,
			children = {
				pill(selected and "Chosen" or "Pick", selected and C.blue or C.bg_deep, selected and C.white or C.muted),
				text(option.label, C.ink, 13),
			},
		})
	end
	return ui.row({ wrap = true, gap = 10, stretch = true, options })
end

local function right_label(exercise, id)
	for _, item in ipairs(exercise.right) do
		if item.id == id then return item.label end
	end
	return nil
end

local function right_owner(id)
	for left, right in pairs(S.matches) do
		if right == id then return left end
	end
	return nil
end

local function match_view(exercise)
	local left, right = {}, {}
	for _, item in ipairs(exercise.left) do
		local selected = S.match_left == item.id
		local assigned = S.matches[item.id]
		left[#left + 1] = button({
			id = "match-left:" .. exercise.id .. ":" .. item.id,
			w_full = true,
			fill = selected and C.saffron_soft or (assigned and C.green_soft or C.panel),
			hover_fill = selected and C.saffron_soft or C.green_soft,
			stroke = selected and C.saffron or (assigned and C.green or C.line),
			hover_stroke = selected and C.saffron or C.green,
			on_click = function() M.select_left(item.id) end,
			children = {
				pill(selected and "Now choose right" or (assigned and "Linked" or "Choose"), selected and C.saffron or (assigned and C.green or C.bg_deep), (selected or assigned) and C.white or C.muted),
				ui.col({
					grow = true,
					gap = 2,
					text(item.label, C.ink, 13),
					assigned and text(right_label(exercise, assigned), C.green, 10) or false,
				}),
			},
		})
	end
	for _, item in ipairs(exercise.right) do
		local owner = right_owner(item.id)
		right[#right + 1] = button({
			id = "match-right:" .. exercise.id .. ":" .. item.id,
			w_full = true,
			fill = owner and C.blue_soft or C.panel,
			hover_fill = C.blue_soft,
			stroke = owner and C.blue or C.line,
			hover_stroke = C.blue,
			on_click = function() M.select_right(item.id) end,
			children = {
				pill(owner and "Linked" or "Match", owner and C.blue or C.bg_deep, owner and C.white or C.muted),
				text(item.label, C.ink, 12),
			},
		})
	end
	return ui.row({
		wrap = true,
		gap = 14,
		stretch = true,
		ui.col({ grow = true, min_w = 270, gap = 8, text("START HERE", C.saffron, 10, true), left }),
		ui.col({ grow = true, min_w = 270, gap = 8, text("THEN CHOOSE HERE", C.blue, 10, true), right }),
	})
end

local function direction_button(exercise, direction, label)
	return button({
		id = "route:" .. exercise.id .. ":" .. direction,
		min_w = 70,
		center = true,
		fill = C.green_soft,
		hover_fill = C.green,
		stroke = C.green,
		hover_stroke = C.green,
		on_click = function() M.move(direction) end,
		children = { text(label, C.ink, 13, true) },
	})
end

local function route_view(exercise)
	local map = exercise.map
	local here = S.route[#S.route]
	local at_goal = here[1] == map.goal[1] and here[2] == map.goal[2]
	local steps = #S.route - 1
	return ui.row({
		wrap = true,
		gap = 18,
		stretch = true,
		ui.col({
			min_w = 0,
			gap = 8,
			align_center = true,
			grow = true,
			ui.row({
				w_full = true,
				align_center = true,
				text("ROUTE MAP", C.green, 10, true),
				ui.col({ grow = true }),
				pill(steps .. (steps == 1 and " move" or " moves"), C.bg_deep, C.muted),
			}),
			ui.col({
				id = "route-map-scroll:" .. exercise.id,
				w_full = true,
				scroll_x = true,
				ui.frame({ visual = terrain.make(exercise, S.route), no_shrink = true }),
			}),
			text("Orange = traveller and trail  •  Green diamond = destination", C.muted, 10),
			text("Blue dots mark landmarks; coloured diamonds mark blocked terrain.", C.muted, 10),
		}),
		ui.col({
			min_w = 245,
			grow = true,
			gap = 10,
			text("COMPASS CONTROLS", C.saffron, 10, true),
			ui.col({
				gap = 8,
				align_center = true,
				direction_button(exercise, "N", "N  ↑"),
				ui.row({
					gap = 8,
					direction_button(exercise, "W", "W  ←"),
					direction_button(exercise, "S", "S  ↓"),
					direction_button(exercise, "E", "E  →"),
				}),
			}),
			ui.row({
				gap = 8,
				button({
					id = "route-undo:" .. exercise.id,
					grow = true,
					on_click = M.undo_move,
					children = { text("Undo", C.ink, 11, true) },
				}),
				button({
					id = "route-reset:" .. exercise.id,
					grow = true,
					on_click = M.reset_route,
					children = { text("Reset", C.ink, 11, true) },
				}),
			}),
			ui.row({
				w_full = true,
				px = 12,
				py = 10,
				radius = 9,
				fill = at_goal and C.green_soft or C.bg,
				text(at_goal and "Destination reached — check your route." or "Traveller: column " .. here[1] .. ", row " .. here[2], at_goal and C.green or C.muted, 11),
			}),
		}),
	})
end

local function feedback(exercise)
	if not S.feedback then return false end
	local correct = S.feedback.kind == "correct"
	local retry = S.feedback.kind == "retry"
	return ui.row({
		w_full = true,
		gap = 11,
		px = 14,
		py = 12,
		radius = 10,
		align_center = true,
		fill = correct and C.green_soft or (retry and C.wrong_soft or C.blue_soft),
		stroke = { 1, correct and C.green or (retry and C.wrong or C.blue) },
		fade_in = 120,
		pill(correct and "Well done" or (retry and "Try again" or "Keep going"), correct and C.green or (retry and C.wrong or C.blue), C.white),
		ui.col({
			grow = true,
			gap = 3,
			text(S.feedback.text, C.ink, 12),
			correct and text("You can replay this activity or continue to the next one.", C.green, 10) or false,
		}),
	})
end

local function action_bar(exercise)
	local first = S.class == 1 and S.activity == 1
	local last = S.class == 5 and S.activity == 3
	local ready = M.can_check(exercise)
	local check = button({
		id = "check:" .. exercise.id,
		min_w = 160,
		center = true,
		fill = ready and C.green or C.bg_deep,
		hover_fill = ready and C.green_hi or C.line,
		stroke = ready and C.green or C.line,
		hover_stroke = ready and C.green or C.muted,
		on_click = M.check,
		children = { text("Check answer", ready and C.white or C.muted, 13, true) },
	})
	return ui.row({
		wrap = true,
		gap = 9,
		align_center = true,
		(not first) and button({
			id = "previous",
			children = { text("Previous", C.ink, 12, true) },
			on_click = M.previous,
		}) or false,
		ui.col({ grow = true, min_w = 10 }),
		check,
		(not last) and button({
			id = "next",
			fill = C.saffron_soft,
			hover_fill = C.saffron,
			stroke = C.saffron,
			hover_stroke = C.saffron,
			children = { text(S.feedback and S.feedback.kind == "correct" and "Continue" or "Next", C.ink, 12, true) },
			on_click = M.next,
		}) or pill("All 15 activities explored", C.saffron_soft, C.saffron),
	})
end

local function exercise_card(exercise)
	local activity
	if exercise.kind == "choose" then activity = choice_view(exercise)
	elseif exercise.kind == "match" then activity = match_view(exercise)
	else activity = route_view(exercise) end

	return ui.col({
		w_full = true,
		gap = 18,
		pad = 22,
		radius = 16,
		fill = C.panel,
		stroke = { 1, C.line },
		ui.row({
			wrap = true,
			gap = 9,
			align_center = true,
			pill(string.upper(mechanic_names[exercise.kind]), C.blue_soft, C.blue),
			M.mastered(exercise.id) and pill("MASTERED", C.green_soft, C.green) or false,
			ui.col({ grow = true }),
		}),
		ui.col({
			gap = 7,
			text(exercise.title, C.ink, 22),
			text(exercise.prompt, C.ink, 15),
			text(mechanic_notes[exercise.kind], C.muted, 11),
		}),
		ui.col({ h = 1, w_full = true, fill = C.line_soft }),
		activity,
		feedback(exercise),
		action_bar(exercise),
	})
end

return function()
	local level = M.content.classes[S.class]
	local exercise = M.current()
	return ui.col({
		id = "bharat-compass",
		full = true,
		fill = C.bg,
		header(),
		ui.col({ h = 1, w_full = true, fill = C.line }),
		ui.col({
			id = "learning-scroll",
			grow = true,
			w_full = true,
			scroll_y = true,
			px = 18,
			py = 18,
			ui.row({
				w_full = true,
				ui.col({ grow = true, min_w = 0 }),
				ui.col({
					grow = 8,
					max_w = C.content_max,
					min_w = 0,
					gap = 14,
					class_picker(),
					ui.row({
						wrap = true,
						gap = 8,
						align_center = true,
						text("CLASS " .. S.class, C.saffron, 10, true),
						text(level.name, C.ink, 18, true),
						text(level.intro, C.muted, 11),
					}),
					activity_picker(level),
					exercise_card(exercise),
					ui.row({
						center = true,
						py = 8,
						text("Bundled, offline practice. Progress stays in this local workspace.", C.faint, 10),
					}),
				}),
				ui.col({ grow = true, min_w = 0 }),
			}),
		}),
	})
end
