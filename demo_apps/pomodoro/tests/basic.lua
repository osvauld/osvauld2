return function(t)
	t.step(1)
	t.expect(t.text("timer") == "25:00", "starts at a work session")
	t.expect(t.text("sessions") == "0 sessions done", "stats seed renders")
	local x, y = t.centre_of("toggle")
	t.click_at(x, y)
	t.step(1)
	t.expect(t.text("toggle:label") == "Pause", "start click reaches the timer")
end
