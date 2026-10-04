return function(t)
	t.step(1)
	t.expect(t.text("title") == "Scratch", "title renders")
	t.expect(t.text("note-count") == "2", "guarded seed creates two notes")
	t.expect(t.centre_of("add") ~= nil, "add button is reachable")
end
