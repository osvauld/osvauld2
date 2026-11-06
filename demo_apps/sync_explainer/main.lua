local C = require("theme")
local story = require("story")
local diagram = require("diagram")

local function text(value, size, color, id)
	return ui.text({ value, id = id, font_size = size, color = color or C.text })
end

local function stop(s)
	s.playing, s.anchor, s.phase = false, nil, 0
end

local function go(s, step)
	local stages = story.scenarios[s.scenario].stages
	s.step = math.max(1, math.min(#stages, step))
	s.selected = stages[s.step].node
	s.phase, s.anchor = 0, nil
	if stages[s.step].waiting or s.step == #stages then stop(s) end
end

local function button(id, title, action, active, color)
	return ui.button({
		id = id, h = 34, px = 13, gap = 7, radius = 7, center = true,
		fill = active and C.active or C.node, stroke = { 1, active and (color or C.blue) or C.line },
		hover_stroke = { 1, C.blue }, cursor = "pointer", on_click = action,
		text(title, 12),
	})
end

local function inspector(s)
	local scenario = story.scenarios[s.scenario]
	local stage = scenario.stages[s.step]
	local node = story.by_id[s.selected]
	local accent = C[scenario.color]
	return ui.col({
		w = 290, min_w = 260, grow = 1, no_shrink = true, pad = 20, gap = 13,
		fill = C.panel, radius = 12, stroke = { 1, C.line },
		text("THIS STEP", 10, accent),
		text(stage.title, 19),
		text(stage.body, 13, C.muted, "sync-step-body"),
		ui.col({ h = 1, w_full = true, fill = C.line, mt = 4, mb = 3 }),
		text("INSPECT COMPONENT", 10, C.blue),
		text(node.title, 17, C.text, "sync-inspector-title"),
		text(node.body, 13, C.muted),
		text("IMPLEMENTATION", 10, C.blue),
		text(node.source, 11, C.muted, "sync-source"),
	})
end

return function()
	local s = ui.state("sync-explainer-view", { scenario = "success", step = 1, selected = "edit", playing = false, phase = 0 })
	local scenario = story.scenarios[s.scenario]
	local stage = scenario.stages[s.step]
	local choices = {}
	for _, id in ipairs({ "success", "offline", "denied" }) do
		local option = story.scenarios[id]
		choices[#choices + 1] = button("sync-scenario:" .. id, option.label, function()
			s.scenario = id
			stop(s)
			go(s, 1)
		end, s.scenario == id, C[option.color])
	end
	local function tick(e)
		if not s.playing then return end
		if not s.anchor then s.anchor = e.elapsed end
		local elapsed = math.max(0, e.elapsed - s.anchor)
		local count = math.floor(elapsed / C.step_seconds)
		if count > 0 then
			-- Visit each boundary so a large clock jump cannot skip the offline stop.
			for _ = 1, math.min(count, #scenario.stages) do
				local anchor = s.anchor
				go(s, s.step + 1)
				if not s.playing then return end
				s.anchor = anchor + C.step_seconds
			end
		end
		s.phase = math.max(0, math.min(1, (e.elapsed - s.anchor) / C.step_seconds))
	end
	return ui.col({
		full = true, fill = C.bg,
		ui.col({
			id = "sync-explainer", grow = true, scroll_y = true, pad = 20, gap = 14,
			on_frame = s.playing and tick or nil,
			ui.row({
				align_center = true, gap = 18, wrap = true,
				text("How sync works", 28, C.text, "sync-title"),
				ui.col({ grow = true }),
				text("OSVAULD / EXPLAINED", 11, C.blue),
			}),
			text("Illustrative walkthrough · explore each component or follow an update, one step at a time.", 12, C.muted, "sync-disclaimer"),
			ui.row({ gap = 8, wrap = true, choices }),
			ui.row({
				gap = 8, wrap = true, align_center = true,
				button("sync-prev", "Previous", function() stop(s) go(s, s.step - 1) end),
				ui.button({
					id = "sync-play", h = 34, px = 15, radius = 7, center = true,
					fill = C.active, stroke = { 1, C.blue }, cursor = "pointer",
					on_click = function()
						if s.playing then stop(s) return end
						if stage.waiting then return end
						if s.step == #scenario.stages then go(s, 1) end
						s.playing, s.anchor = true, nil
					end,
					text(s.playing and "Pause" or "Play", 12, C.text, "sync-play-label"),
				}),
				stage.waiting and button("sync-reconnect", "Reconnect & continue", function() stop(s) go(s, s.step + 1) end, true, C.amber)
					or button("sync-next", "Next step", function() stop(s) go(s, s.step + 1) end),
				button("sync-restart", "Restart", function() stop(s) go(s, 1) end),
				ui.col({ grow = true }),
				text(string.format("Step %d of %d", s.step, #scenario.stages), 12, C.blue, "sync-stage"),
			}),
			ui.row({
				gap = 16, wrap = true, stretch = true,
				ui.col({
					min_w = C.canvas_w, grow = 3, no_shrink = true, radius = 12, fill = C.panel, stroke = { 1, C.line },
					ui.row({ h = 44, px = 18, gap = 10, align_center = true,
						text("THE PATH OF AN UPDATE", 11, C.muted), ui.col({ grow = true }),
						text(scenario.short, 10, C[scenario.color]),
					}),
					diagram.view(s, function(id) s.selected = id end),
					ui.row({ pad = 14, gap = 12, align_center = true,
						ui.col({ w = 7, h = 7, radius = 4, fill = C[scenario.color] }),
						text(stage.outcome, 12, C.text, "sync-outcome"),
					}),
				}),
				inspector(s),
			}),
			text("Already-published item · receiver already subscribed · today's transport: local Unix socket; remote transport is not built.", 11, C.muted, "sync-transport"),
		}),
	})
end
