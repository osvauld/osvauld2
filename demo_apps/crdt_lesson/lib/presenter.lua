local C = require("lib/theme")
local M = { version = 1 }

local function text(value, size, color, id)
	return ui.text({ value, id = id, font_size = size, color = color or C.ink })
end

local function button(id, label, action, active)
	return ui.button({
		id = id, h = 32, px = 12, radius = 7, center = true, cursor = "pointer",
		fill = active and C.soft or C.panel, stroke = { 1, active and C.accent or C.line },
		hover_stroke = { 1, C.accent }, on_click = action,
		text(label, 12, active and C.accent or C.ink),
	})
end

local function validate(spec)
	assert(type(spec.id) == "string" and spec.id ~= "", "presenter: id is required")
	assert(type(spec.prepare) == "function" and type(spec.view) == "function", "presenter: prepare and view functions are required")
	assert(type(spec.actions) == "table", "presenter: actions table is required")
	assert(type(spec.scenes) == "table" and #spec.scenes > 0, "presenter: scenes must not be empty")
	local seen = {}
	for _, scene in ipairs(spec.scenes) do
		assert(type(scene.id) == "string" and scene.id ~= "", "presenter: scene id is required")
		assert(not seen[scene.id], "presenter: duplicate scene id " .. scene.id)
		seen[scene.id] = true
		for _, field in ipairs({ "title", "summary", "narration", "cue" }) do
			assert(type(scene[field]) == "string", "presenter: " .. scene.id .. "." .. field .. " must be text")
		end
		assert(type(scene.seconds) == "number" and scene.seconds > 0 and scene.seconds <= 3600, "presenter: invalid duration for " .. scene.id)
		local previous = -1
		for _, cue in ipairs(scene.cues or {}) do
			assert(type(cue.at) == "number" and cue.at >= 0 and cue.at <= scene.seconds and cue.at >= previous, "presenter: invalid cue time in " .. scene.id)
			assert(type(spec.actions[cue.action]) == "function", "presenter: unknown cue action " .. tostring(cue.action) .. " in " .. scene.id)
			previous = cue.at
		end
	end
end

function M.app(spec)
	validate(spec)
	local first = spec.scenes[1]
	local initial = { scene = first.id, mode = "explore", model = spec.prepare(first.id), elapsed = 0, cue = 0, playing = false, started = false, effect_at = -10 }
	local function index_of(id)
		for i, scene in ipairs(spec.scenes) do if scene.id == id then return i end end
		return 1
	end
	return function()
		local s = ui.state("presenter:" .. spec.id, initial)
		local index = index_of(s.scene)
		local scene = spec.scenes[index]
		local function pause() s.playing, s.anchor = false, nil end
		local function reset()
			pause()
			s.model, s.elapsed, s.cue, s.started, s.effect_at = spec.prepare(s.scene), 0, 0, false, -10
			s.last_action = nil
		end
		local function select(i)
			i = math.max(1, math.min(#spec.scenes, i))
			s.scene = spec.scenes[i].id
			reset()
		end
		local function act(id)
			local fn = spec.actions[id]
			assert(type(fn) == "function", "presenter: unknown action " .. tostring(id))
			fn(s.model)
			s.last_action, s.effect_at = id, s.elapsed
		end
		local function play()
			if s.playing then pause() return end
			if not s.started or s.elapsed >= scene.seconds then reset() end
			s.playing, s.started, s.anchor = true, true, nil
		end
		local function tick(e)
			if not s.playing then return end
			if not s.anchor then s.anchor = e.elapsed - s.elapsed end
			s.elapsed = math.min(scene.seconds, math.max(s.elapsed, e.elapsed - s.anchor))
			local cues = scene.cues or {}
			while cues[s.cue + 1] and cues[s.cue + 1].at <= s.elapsed do
				s.cue = s.cue + 1
				act(cues[s.cue].action)
				s.effect_at = cues[s.cue].at
			end
			if s.elapsed >= scene.seconds then pause() end
		end
		local function mode(value) pause() s.mode = value end
		local context = {
			mode = s.mode, scene = scene, playing = s.playing, last_action = s.last_action,
			motion = math.max(0, math.min(1, (s.elapsed - s.effect_at) / 0.8)),
			act = function(id) pause() s.started = false act(id) end,
		}
		local notes = false
		if s.mode == "presenter" then
			local cues = {}
			for i, cue in ipairs(scene.cues or {}) do
				cues[#cues + 1] = text(string.format("%.1fs  ·  %s", cue.at, cue.label or cue.action), 11, C.muted, "presenter-action:" .. i)
			end
			notes = ui.col({
				w = 300, min_w = 280, grow = 1, pad = 20, gap = 14, radius = 12, fill = C.panel,
				text("YOUR SCRIPT", 10, C.accent),
				text(scene.narration, 14, C.ink, "presenter-narration"),
				ui.col({ h = 1, w_full = true, fill = C.line }),
				text("DEMONSTRATION CUE", 10, C.accent),
				text(scene.cue, 12, C.muted, "presenter-cue"),
				text("PLAYBACK ACTIONS", 10, C.accent), cues,
			})
		end
		local tabs = {}
		for i, entry in ipairs(spec.scenes) do
			tabs[#tabs + 1] = button("scene:" .. entry.id, string.format("%02d  %s", i, entry.short or entry.title), function() select(i) end, s.scene == entry.id)
		end
		return ui.col({
			id = "lesson-root", full = true, fill = C.bg,
			on_frame = s.playing and tick or nil,
			on_key = function(e)
				if not e.down or e.repeated then return end
				if e.code == "Escape" then mode("presenter")
				elseif e.code == "ArrowRight" then select(index + 1)
				elseif e.code == "ArrowLeft" then select(index - 1)
				elseif e.code == "Space" then play() end
			end,
			s.mode ~= "record" and ui.row({
				h = 48, px = 20, gap = 8, align_center = true, fill = C.panel,
				text(spec.title, 13, C.ink), ui.col({ grow = true }),
				button("mode:explore", "Explore", function() mode("explore") end, s.mode == "explore"),
				button("mode:presenter", "Presenter", function() mode("presenter") end, s.mode == "presenter"),
				button("mode:record", "Record view", function() mode("record") end, false),
			}) or false,
			ui.col({
				id = "lesson-scroll", grow = true, scroll_y = true, pad = 22, gap = 14,
				ui.row({ gap = 14, align_center = true,
					text(string.format("%02d / %02d", index, #spec.scenes), 12, C.accent, "lesson-scene-index"),
					text(scene.title, 28, C.ink, "lesson-scene-title"),
				}),
				text(scene.summary, 14, C.muted, "lesson-summary"),
				s.mode ~= "record" and ui.row({ gap = 6, wrap = true, tabs }) or false,
				ui.row({ gap = 16, wrap = true, stretch = true,
					ui.col({ min_w = 650, grow = 3, spec.view(s.model, context) }), notes,
				}),
				text(scene.takeaway or scene.summary, 13, C.accent, "lesson-takeaway"),
			}),
			s.mode ~= "record" and ui.row({
				h = 52, px = 20, gap = 8, align_center = true, fill = C.panel,
				button("lesson-prev", "Previous", function() select(index - 1) end),
				button("lesson-play", s.playing and "Pause" or "Play scene", play, s.playing),
				button("lesson-reset", "Reset scene", reset),
				button("lesson-next", "Next", function() select(index + 1) end),
				ui.col({ grow = true }),
				text(string.format("%.1fs / %ds", s.elapsed, scene.seconds), 11, C.muted, "lesson-time"),
			}) or false,
		})
	end
end

return M
