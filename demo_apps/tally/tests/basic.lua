return function(t)
	t.step(1)
	t.expect(t.text("title") == "TALLY", "title renders")
	t.expect(t.text("count") == "0", "starts at zero")
	local x, y = t.centre_of("plus")
	t.click_at(x, y)
	t.step(1)
	t.expect(t.text("count") == "1", "plus increments through the UI")
end
