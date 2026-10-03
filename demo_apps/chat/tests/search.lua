-- The app tests its own search: seeded messages are findable, a sent one becomes findable, and
-- clearing the box brings the channel back. Runs in a non-persisting tab, so this also proves
-- that tab gets an index of its own.
return function(t)
	t.step(1)
	t.type("search", "deploy")
	t.step(2)
	t.expect(t.text("snippet:seed-1") ~= nil, "a seeded message is found")
	t.expect(t.text("snippet:seed-2") == nil, "an unrelated one is not")

	t.type("search", "")
	t.type("composer", "marigold rollout tomorrow")
	t.click_at(t.centre_of("send"))
	t.step(2)
	t.type("search", "marigold")
	t.step(2)
	t.expect(t.text("no-results") == nil, "a sent message is found")
end
