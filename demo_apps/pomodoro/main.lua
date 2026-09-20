-- POMODORO
--
-- A work timer: 25 minutes, then 5, counting completed sessions into the document.
--
-- The whole app is shaped by one gap: a click cannot read the clock. `on_click` carries no
-- time, and the only monotonic clock an app is handed is `on_frame`'s `e.elapsed` — so
-- "start counting from now" cannot be written where the start happens. Every `S.pending`
-- below is that detour: the click records an intent, and the next frame, which does know
-- what time it is, turns it into a deadline. See NOTES.md.

local C = require("theme")

local d = doc:open("pomodoro")

if not d.stats then
	d:set({ "stats" }, doc.map({ done = 0 }))
end

local S = {
	mode = "work",
	running = false,
	-- Monotonic deadline, on `e.elapsed`'s clock. Nil while paused, when `left` holds instead.
	ends_at = nil,
	left = nil,
	-- "start" | "reset" | nil — an intent a click could not carry out itself.
	pending = nil,
	clock = 0,
}

local function full(mode)
	return (mode == "work" and C.work_mins or C.rest_mins) * 60
end

-- Seconds still to run. While the timer is live this is derived from the deadline rather than
-- accumulated per frame: `e.dt` is clamped after a stall, so a summed countdown quietly loses
-- time it can never get back, and over 25 minutes that is visible.
local function remaining()
	if S.running and S.ends_at then
		return math.max(S.ends_at - S.clock, 0)
	end
	return S.left or full(S.mode)
end

local function clock_face(secs)
	local whole = math.ceil(secs)
	return string.format("%d:%02d", math.floor(whole / 60), whole % 60)
end

local function finish()
	if S.mode == "work" then
		d:set({ "stats", "done" }, (d.stats and d.stats.done or 0) + 1)
	end
	S.mode = S.mode == "work" and "rest" or "work"
	S.running, S.ends_at, S.left = false, nil, nil
end

local function tick(e)
	S.clock = e.elapsed
	if S.pending == "start" then
		S.ends_at = e.elapsed + (S.left or full(S.mode))
		S.running, S.left, S.pending = true, nil, nil
	elseif S.pending == "reset" then
		S.running, S.ends_at, S.left, S.pending = false, nil, nil, nil
	end
	if S.running and S.ends_at and e.elapsed >= S.ends_at then
		finish()
	end
end

local function toggle()
	if S.running then
		-- Pausing can be done in the handler: the remaining time is already known without
		-- asking what time it is.
		S.left, S.running, S.ends_at = remaining(), false, nil
	else
		S.pending = "start"
	end
end

local function accent(hover)
	if S.mode == "work" then
		return hover and C.work_hi or C.work
	end
	return hover and C.rest_hi or C.rest
end

local function bar()
	local left = remaining()
	local done = 1 - left / full(S.mode)
	return ui.row({
		w = C.bar_w,
		h = 6,
		radius = 3,
		fill = C.track,
		ui.col({ w = C.bar_w * done, h = 6, radius = 3, fill = accent(false) }),
	})
end

local function button(id, label, on_click, filled)
	return ui.button({
		id = id,
		h = 36,
		px = 18,
		radius = 8,
		center = true,
		no_shrink = true,
		fill = filled and accent(false) or C.panel,
		hover_fill = filled and accent(true) or C.line,
		press_scale = 0.96,
		tint = 90,
		on_click = on_click,
		ui.text({
			label,
			no_wrap = true,
			color = filled and "#ffffff" or C.text,
			font_size = 13,
		}),
	})
end

return function()
	local done = d.stats and d.stats.done or 0
	local live = S.running or S.pending ~= nil
	return ui.col({
		id = "pomodoro",
		full = true,
		center = true,
		gap = 22,
		fill = C.bg,
		-- Declaring `on_frame` is what keeps the app repainting, so it is declared only while
		-- the timer is live. That is also the whole scheduling story: there is no way to ask
		-- for a wake at a time, or for a repaint at the 1Hz this display actually changes at,
		-- so a running timer repaints at the display's rate to move one digit per second.
		on_frame = live and tick or nil,

		ui.text({
			S.mode == "work" and "Focus" or "Break",
			no_wrap = true,
			color = C.muted,
			font_size = 13,
		}),
		ui.text({
			clock_face(remaining()),
			no_wrap = true,
			color = C.text,
			font_size = 76,
		}),
		bar(),
		ui.row({
			gap = 10,
			button("toggle", S.running and "Pause" or "Start", toggle, true),
			button("reset", "Reset", function()
				S.pending = "reset"
			end, false),
		}),
		ui.text({
			done == 1 and "1 session done" or (done .. " sessions done"),
			no_wrap = true,
			color = C.muted,
			font_size = 12,
		}),
	})
end
