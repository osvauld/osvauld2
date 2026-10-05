-- What search sees: one record per message, across every channel doc. The host runs this in
-- its own read-only VM (docs/design/search.md §4) — nothing here can write or draw.
--
-- The author is indexed as the id it is, not a display name: a rename then re-indexes nothing.
return {
	doc = "channel/*",
	each = { "messages" },
	key = function(m)
		return m.id
	end,
	fields = function(_id, m, _channel)
		return {
			body = m.text,
			facet = { author = m.author },
			time = m.sent_at,
		}
	end,
	rank = "recent",
}
