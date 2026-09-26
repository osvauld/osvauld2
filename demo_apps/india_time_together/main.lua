local C = require("theme")
local Content = require("content")

local progress = doc:open("india-time-together-progress")
if not progress.items then
	progress:set({ "items" }, doc.map({}))
end

local S = {
	class = 1,
	mechanic = "source",
	choice = nil,
	selected = {},
	order = {},
	drag = nil,
	over_slot = nil,
	feedback = nil,
}

local function reset_attempt()
	S.choice = nil
	S.selected = {}
	S.order = {}
	S.drag = nil
	S.over_slot = nil
	S.feedback = nil
end

local function current()
	return Content.classes[S.class].exercises[S.mechanic]
end

local function entry(id)
	return progress.items and progress.items[id]
end

local function is_mastered(id)
	local p = entry(id)
	return p and p.mastered == true
end

local function record_check(ex, correct)
	local old = entry(ex.id)
	progress:set({ "items", ex.id }, doc.map({
		mastered = correct or (old and old.mastered == true) or false,
		attempts = (old and old.attempts or 0) + 1,
	}))
end

local function choose_class(n)
	S.class = n
	reset_attempt()
end

local function choose_mechanic(id)
	S.mechanic = id
	reset_attempt()
end

local function check_source(ex)
	local ok = S.choice == ex.answer
	record_check(ex, ok)
	S.feedback = { ok = ok, text = ok and ex.explanation or ex.clue }
end

local function same_answers(chosen, answers)
	local expected = {}
	for _, id in ipairs(answers) do expected[id] = true end
	local count = 0
	for id, on in pairs(chosen) do
		if on then
			count = count + 1
			if not expected[id] then return false end
		end
	end
	return count == #answers
end

local function check_civics(ex)
	local ok = same_answers(S.selected, ex.answers)
	record_check(ex, ok)
	S.feedback = { ok = ok, text = ok and ex.explanation or ex.clue }
end

local function timeline_finished(ex)
	for i = 1, #ex.steps do
		if not S.order[i] then return false end
	end
	return true
end

local function finish_timeline(ex)
	if not timeline_finished(ex) then return end
	for i, step in ipairs(ex.steps) do
		if S.order[i] ~= step.id then
			S.feedback = { ok = false, text = ex.clue }
			return
		end
	end
	if not is_mastered(ex.id) then
		local old = entry(ex.id)
		progress:set({ "items", ex.id }, doc.map({
			mastered = true,
			attempts = (old and old.attempts or 0) + 1,
		}))
	end
	S.feedback = { ok = true, text = ex.explanation }
end

local function place_timeline_card(ex, slot)
	local id = S.drag and S.drag.id
	if not id then return end
	local from
	for i = 1, #ex.steps do
		if S.order[i] == id then from = i break end
	end
	local displaced = S.order[slot]
	if from then S.order[from] = displaced end
	S.order[slot] = id
	S.feedback = nil
	finish_timeline(ex)
end

local function button(id, label, on_click, style)
	style = style or {}
	return ui.button({
		id = id,
		h = style.wrap_text and nil or (style.h or 40),
		min_h = style.wrap_text and (style.min_h or 54) or style.min_h,
		w_full = style.w_full,
		min_w = style.min_w,
		px = style.px or 16,
		py = style.wrap_text and 10 or nil,
		radius = style.radius or 10,
		center = true,
		no_shrink = not style.wrap_text,
		fill = style.fill or C.paper,
		hover_fill = style.hover_fill or C.paper_warm,
		press_fill = style.press_fill or style.hover_fill or C.paper_warm,
		stroke = { 1, style.stroke or C.line },
		press_scale = 0.97,
		tint = 100,
		on_click = on_click,
		ui.text({
			label,
			color = style.color or C.ink,
			font_size = style.font_size or 13,
			no_wrap = not style.wrap_text,
		}),
	})
end

local function completion_count(class_number)
	local done = 0
	local exercises = Content.classes[class_number].exercises
	for _, mechanic in ipairs(Content.mechanics) do
		if is_mastered(exercises[mechanic.id].id) then done = done + 1 end
	end
	return done
end

local function all_completion()
	local done = 0
	for class_number = 1, 5 do done = done + completion_count(class_number) end
	return done
end

local function header()
	local dots = {}
	for i = 1, 15 do
		dots[#dots + 1] = ui.col({
			w = 18,
			h = 6,
			radius = 3,
			fill = i <= all_completion() and C.green or C.line,
		})
	end
	return ui.col({
		id = "showcase-header",
		w_full = true,
		gap = 14,
		pad = 22,
		fill = C.paper,
		stroke = { 1, C.line },
		radius = 18,
		fade_in = 180,
		ui.row({
			w_full = true,
			gap = 14,
			align_center = true,
			wrap = true,
			ui.col({
				w = 10,
				h = 54,
				radius = 5,
				fill = C.saffron,
			}),
			ui.col({
				grow = true,
				min_w = 190,
				gap = 4,
				ui.text({ "INDIA: TIME & TOGETHER", color = C.saffron, font_size = 11, no_wrap = true }),
				ui.text({ "Read the past. Practise living together.", color = C.ink, font_size = 26 }),
				ui.text({ "15 short history and civics investigations for Classes 1–5", color = C.muted, font_size = 13 }),
			}),
			ui.col({
				min_w = 150,
				gap = 3,
				align_center = true,
				ui.text({ all_completion() .. " / 15", color = C.green, font_size = 24, no_wrap = true }),
				ui.text({ "activities completed", color = C.muted, font_size = 11, no_wrap = true }),
			}),
		}),
		ui.row({ gap = 5, wrap = true, dots }),
	})
end

local function class_nav()
	local tabs = {}
	for n = 1, 5 do
		local active = S.class == n
		local done = completion_count(n)
		tabs[#tabs + 1] = button("class:" .. n, "Class " .. n .. "  ·  " .. done .. "/3", function()
			choose_class(n)
		end, {
			min_w = 122,
			fill = active and C.ink or C.paper,
			hover_fill = active and C.ink or C.paper_warm,
			stroke = active and C.ink or C.line,
			color = active and C.white or C.ink,
		})
	end
	return ui.col({
		w_full = true,
		gap = 8,
		ui.text({ "CHOOSE A LEVEL", color = C.muted, font_size = 10, no_wrap = true }),
		ui.row({ gap = 8, wrap = true, tabs }),
		ui.text({ Content.classes[S.class].intro, color = C.muted, font_size = 13 }),
	})
end

local function activity_nav()
	local tabs = {}
	local exercises = Content.classes[S.class].exercises
	for _, mechanic in ipairs(Content.mechanics) do
		local active = mechanic.id == S.mechanic
		local mastered = is_mastered(exercises[mechanic.id].id)
		tabs[#tabs + 1] = button("activity:" .. mechanic.id, (mastered and "DONE  " or "") .. mechanic.label, function()
			choose_mechanic(mechanic.id)
		end, {
			min_w = 170,
			fill = active and C.blue or (mastered and C.green_soft or C.paper),
			hover_fill = active and C.blue_hi or C.blue_soft,
			stroke = active and C.blue or (mastered and C.green or C.line),
			color = active and C.white or (mastered and C.green or C.ink),
		})
	end
	return ui.row({ gap = 9, wrap = true, tabs })
end

local function option_button(ex, choice, mode)
	local selected = mode == "source" and S.choice == choice.id or S.selected[choice.id] == true
	local mark = mode == "civics" and (selected and "SELECTED" or "CHOOSE") or (selected and "MY ANSWER" or "OPTION")
	return ui.button({
		id = "choice:" .. ex.id .. ":" .. choice.id,
		w_full = true,
		min_h = 54,
		gap = 12,
		px = 14,
		py = 10,
		radius = 11,
		align_center = true,
		fill = selected and C.blue_soft or C.paper,
		hover_fill = selected and C.blue_soft or C.paper_warm,
		press_fill = C.blue_soft,
		stroke = { selected and 2 or 1, selected and C.blue or C.line },
		press_scale = 0.99,
		tint = 100,
		on_click = function()
			if mode == "source" then
				S.choice = choice.id
			else
				S.selected[choice.id] = not S.selected[choice.id]
			end
			S.feedback = nil
		end,
		ui.col({
			w = 76,
			no_shrink = true,
			ui.text({ mark, color = selected and C.blue or C.muted, font_size = 9, no_wrap = true }),
		}),
		ui.col({ grow = true, ui.text({ choice.text, color = C.ink, font_size = 14 }) }),
	})
end

local function feedback(ex)
	if not S.feedback then return false end
	local ok = S.feedback.ok
	return ui.col({
		id = "feedback:" .. ex.id,
		w_full = true,
		gap = 7,
		pad = 14,
		radius = 12,
		fill = ok and C.green_soft or C.wrong_soft,
		stroke = { 1, ok and C.green or C.wrong },
		fade_in = 140,
		slide_in = { { 0, 6 }, 140 },
		ui.text({ ok and "Good reasoning" or "Try another way", color = ok and C.green or C.wrong, font_size = 13, no_wrap = true }),
		ui.text({ S.feedback.text, color = C.ink, font_size = 13 }),
	})
end

local function check_row(ex, check_fn)
	local controls = {
		button("check:" .. ex.id, "Check my thinking", function() check_fn(ex) end, {
			fill = C.green,
			hover_fill = C.green_hi,
			stroke = C.green,
			color = C.white,
			min_w = 178,
		}),
	}
	if S.feedback and S.feedback.ok then
		controls[#controls + 1] = button("replay:" .. ex.id, "Replay", reset_attempt, {
			fill = C.paper,
			hover_fill = C.paper_warm,
		})
	end
	return ui.row({ gap = 9, wrap = true, controls })
end

local function source_view(ex)
	local choices = {}
	for _, choice in ipairs(ex.choices) do choices[#choices + 1] = option_button(ex, choice, "source") end
	return ui.col({
		w_full = true,
		gap = 12,
		ui.col({
			w_full = true,
			gap = 9,
			pad = 16,
			radius = 12,
			fill = C.saffron_soft,
			stroke = { 1, C.saffron },
			ui.text({ "AUTHORED PRACTICE SOURCE  /  " .. ex.source_label:upper(), color = C.saffron, font_size = 10 }),
			ui.text({ ex.source, color = C.ink, font_size = 15 }),
			ui.text({ "Source rule: say what the evidence supports, and no more.", color = C.muted, font_size = 11 }),
		}),
		ui.col({ w_full = true, gap = 8, choices }),
		feedback(ex),
		check_row(ex, check_source),
	})
end

local function step_by_id(ex, id)
	for _, step in ipairs(ex.steps) do if step.id == id then return step end end
end

local function order_has(id)
	for _, chosen in pairs(S.order) do if chosen == id then return true end end
	return false
end

local function timeline_card(ex, step, ghost)
	local dragging = S.drag and S.drag.id == step.id
	local card = ui.row({
		min_h = 58,
		gap = 12,
		px = 14,
		py = 10,
		radius = 11,
		align_center = true,
		fill = ghost and C.blue_soft or C.paper,
		stroke = { ghost and 2 or 1, ghost and C.blue or C.line_strong },
		opacity = dragging and not ghost and 0.28 or 1,
		ui.col({
			w = 42,
			no_shrink = true,
			ui.text({ "DRAG", color = C.blue, font_size = 9, no_wrap = true }),
		}),
		ui.col({ grow = true, ui.text({ step.text, color = C.ink, font_size = 13 }) }),
	})
	if ghost then return card end
	card.id = "timeline-card:" .. ex.id .. ":" .. step.id
	card.hover_fill = C.blue_soft
	card.hover_stroke = { 1, C.blue }
	card.on_drag = function(e)
		if e.phase == "start" then
			S.drag = {
				id = step.id,
				x = e.origin_x,
				y = e.origin_y,
				scale = e.scale,
			}
		elseif e.phase == "move" and S.drag then
			S.drag.x, S.drag.y, S.drag.scale = e.origin_x, e.origin_y, e.scale
		elseif e.phase == "end" then
			S.drag, S.over_slot = nil, nil
		end
	end
	return card
end

local function timeline_slot(ex, n)
	local id = S.order[n]
	local step = id and step_by_id(ex, id)
	local over = S.drag and S.over_slot == n
	return ui.col({
		id = "timeline-slot:" .. ex.id .. ":" .. n,
		min_h = 76,
		stretch = true,
		gap = 8,
		pad = 8,
		radius = 12,
		fill = over and C.saffron_soft or C.paper_warm,
		stroke_dash = { over and 2 or 1, over and C.saffron or C.line_strong, 6, 4 },
		on_drop = function(e)
			if not S.drag then return end
			if e.phase == "over" then
				S.over_slot = n
			else
				place_timeline_card(ex, n)
			end
		end,
		ui.text({ "STEP " .. n, color = over and C.saffron or C.muted, font_size = 9, no_wrap = true }),
		step and timeline_card(ex, step, false) or ui.col({
			grow = true,
			center = true,
			ui.text({ over and "Release to place" or "Drop a card here", color = C.muted, font_size = 12 }),
		}),
	})
end

local function order_view(ex)
	local slots = {}
	for i = 1, #ex.steps do slots[#slots + 1] = timeline_slot(ex, i) end

	local cards = {}
	for _, id in ipairs(ex.start) do
		if not order_has(id) then
			cards[#cards + 1] = timeline_card(ex, step_by_id(ex, id), false)
		end
	end
	if #cards == 0 then
		cards[1] = ui.col({
			w_full = true,
			min_h = 58,
			center = true,
			radius = 11,
			fill = C.green_soft,
			ui.text({ "All cards are on the timeline", color = C.green, font_size = 12 }),
		})
	end

	return ui.col({
		w_full = true,
		gap = 12,
		ui.col({
			w_full = true,
			gap = 4,
			pad = 12,
			radius = 10,
			fill = C.blue_soft,
			ui.text({ "DRAG THE EVIDENCE CARDS", color = C.blue, font_size = 10, no_wrap = true }),
			ui.text({ "Place one card in each numbered drop zone. Drag a placed card again to revise the sequence.", color = C.ink, font_size = 12 }),
		}),
		ui.row({
			w_full = true,
			gap = 12,
			stretch = true,
			ui.col({
				grow = true,
				stretch = true,
				gap = 8,
				ui.text({ "EVIDENCE CARDS", color = C.blue, font_size = 10, no_wrap = true }),
				cards,
			}),
			ui.col({
				grow = true,
				stretch = true,
				gap = 8,
				ui.text({ "TIMELINE", color = C.green, font_size = 10, no_wrap = true }),
				slots,
			}),
		}),
		feedback(ex),
		button("clear:" .. ex.id, "Reset board", function()
			S.order, S.feedback = {}, nil
		end, { h = 34, px = 12, font_size = 11 }),
	})
end

local function civics_view(ex)
	local choices = {}
	for _, choice in ipairs(ex.choices) do choices[#choices + 1] = option_button(ex, choice, "civics") end
	return ui.col({
		w_full = true,
		gap = 12,
		ui.col({
			w_full = true,
			pad = 12,
			radius = 10,
			fill = C.plum_soft,
			ui.text({ "Choose every action that helps. There may be more than one.", color = C.plum, font_size = 12 }),
		}),
		ui.col({ w_full = true, gap = 8, choices }),
		feedback(ex),
		check_row(ex, check_civics),
	})
end

local function exercise_card()
	local ex = current()
	local body
	if S.mechanic == "source" then body = source_view(ex)
	elseif S.mechanic == "order" then body = order_view(ex)
	else body = civics_view(ex) end

	return ui.col({
		id = "exercise:" .. ex.id,
		w_full = true,
		gap = 18,
		pad = 22,
		radius = 18,
		fill = C.paper,
		stroke = { 1, C.line },
		fade_in = 130,
		ui.row({
			w_full = true,
			gap = 12,
			align_center = true,
			wrap = true,
			ui.col({
				grow = true,
				min_w = 160,
				gap = 5,
				ui.text({ Content.classes[S.class].label:upper() .. "  /  " .. S.mechanic:upper(), color = C.blue, font_size = 10, no_wrap = true }),
				ui.text({ ex.title, color = C.ink, font_size = 23 }),
			}),
			is_mastered(ex.id) and ui.col({
				px = 11,
				py = 6,
				radius = 12,
				fill = C.green_soft,
				ui.text({ "COMPLETED", color = C.green, font_size = 10, no_wrap = true }),
			}) or false,
		}),
		ui.text({ ex.prompt, color = C.ink, font_size = 16 }),
		ui.col({ h = 1, w_full = true, fill = C.line }),
		body,
	})
end

return function()
	local ex = current()
	local dragged = S.drag and step_by_id(ex, S.drag.id)
	local ghost = dragged and ui.col({
		absolute = true,
		left = S.drag.x,
		top = S.drag.y,
		w = 320,
		scale = S.drag.scale or 1,
		opacity = 0.92,
		timeline_card(ex, dragged, true),
	}) or false

	return ui.col({
		id = "india-time-together:root",
		full = true,
		fill = C.bg,
		ui.col({
			id = "india-time-together:scroll",
			grow = true,
			w_full = true,
			scroll_y = true,
			ui.row({
				w_full = true,
				px = 18,
				py = 22,
				ui.col({ grow = true }),
				ui.col({
					w_full = true,
					max_w = 980,
					gap = 20,
					header(),
					class_nav(),
					activity_nav(),
					exercise_card(),
					ui.col({
						w_full = true,
						gap = 4,
						align_center = true,
						py = 10,
						ui.text({ "Independent, curriculum-informed practice", color = C.muted, font_size = 11 }),
						ui.text({ "All source cards are authored practice scenarios, not archival reproductions.", color = C.muted, font_size = 10 }),
						ui.text({ "Not official NCERT, CBSE, or state-board material. Progress stays in this local app document.", color = C.muted, font_size = 10 }),
					}),
				}),
				ui.col({ grow = true }),
			}),
		}),
		ghost,
	})
end
