-- e2e_mp.py's probe (jetpack-mp.md M1–M3): send on a declared channel, flood it, send on an
-- undeclared one, and show what arrived and who is here.
local got, count = {}, 0
net.on("ping", function(body, from)
	count += 1
	got[#got + 1] = string.sub(from, 1, 8) .. ":" .. tostring(body.text)
	if #got > 4 then
		table.remove(got, 1)
	end
end)

return function()
	local draft = ui.state("draft", { text = "" })
	local ids = {}
	for _, p in ipairs(net.peers()) do
		ids[#ids + 1] = string.sub(p.id, 1, 8) .. (p.me and "*" or "")
	end
	return ui.col({
		pad = 16,
		gap = 8,
		ui.input({
			id = "say",
			value = draft.text,
			on_input = function(v)
				draft.text = v
			end,
			on_enter = function()
				net.send("ping", { text = draft.text })
			end,
		}),
		ui.button({ "flood", id = "flood", on_click = function()
			for i = 1, 50 do
				net.send("ping", { text = "f" .. i })
			end
		end }),
		ui.button({ "bad", id = "bad", on_click = function()
			net.send("nope", 1)
		end }),
		ui.text({ "count " .. count, id = "count", no_wrap = true }),
		ui.text({ "got " .. table.concat(got, " "), id = "got", no_wrap = true }),
		ui.text({ "peers " .. table.concat(ids, ","), id = "peers", no_wrap = true }),
	})
end
