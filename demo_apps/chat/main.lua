-- CHAT
--
-- Channels on the left, messages or search results on the right. The search box is the point
-- of this app: it is the reference for `index.lua` + `search.query` (docs/design/search.md).
local C = require("theme")
local M = require("model")
local S = M.S

local function channel_button(ch)
	local on = ch.id == S.channel
	return ui.button({
		id = "ch:" .. ch.id,
		px = 10,
		py = 6,
		radius = 6,
		fill = on and C.card_hi or C.panel,
		hover_fill = C.card,
		ui.text({ "# " .. ch.name, color = on and C.text or C.muted, font_size = 13, no_wrap = true }),
		on_click = function()
			S.channel = ch.id
			S.editing = nil
		end,
	})
end

local function small_button(id, label, color, on_click)
	return ui.button({
		id = id,
		px = 6,
		py = 2,
		radius = 4,
		hover_fill = C.card_hi,
		no_shrink = true,
		ui.text({ label, color = color, font_size = 11, no_wrap = true }),
		on_click = on_click,
	})
end

local function message_row(m)
	return ui.row({
		id = "msg:" .. m.id,
		gap = 8,
		px = 10,
		py = 6,
		radius = 6,
		fill = S.editing == m.id and C.card_hi or C.card,
		align_center = true,
		ui.text({ m.author, color = C.accent_hi, font_size = 12, no_wrap = true, no_shrink = true }),
		ui.col({ grow = true, ui.text({ m.text, id = "text:" .. m.id, color = C.text, font_size = 13 }) }),
		small_button("edit:" .. m.id, "edit", C.muted, function()
			S.editing = m.id
		end),
		small_button("del:" .. m.id, "delete", C.danger, function()
			M.delete(m.id)
		end),
	})
end

local function hit_row(h)
	local cid = M.channel_of(h.doc)
	return ui.button({
		id = "hit:" .. h.id,
		gap = 8,
		px = 10,
		py = 6,
		radius = 6,
		fill = C.card,
		hover_fill = C.card_hi,
		ui.text({ "#" .. (cid or h.doc), color = C.hit, font_size = 12, no_wrap = true, no_shrink = true }),
		ui.text({ h.snippet or "", id = "snippet:" .. h.id, color = C.text, font_size = 13 }),
		on_click = function()
			if cid then
				S.channel = cid
			end
		end,
	})
end

local function body(query)
	local rows = {}
	if query ~= "" then
		local hits = search.query(query, { limit = 50 })
		for _, h in ipairs(hits) do
			rows[#rows + 1] = hit_row(h)
		end
		if #hits == 0 then
			rows[1] = ui.text({ "no results", id = "no-results", color = C.muted, font_size = 13 })
		end
		return ui.col({ id = "results", grow = true, scroll_y = true, gap = 6, pad = 12, rows })
	end
	local msgs = M.channel(S.channel).messages or {}
	for _, m in ipairs(msgs) do
		rows[#rows + 1] = message_row(m)
	end
	return ui.col({ id = "messages", grow = true, scroll_y = true, gap = 6, pad = 12, rows })
end

return function()
	local find = ui.state("search", { q = "" })
	local draft = ui.state("composer", { text = "" })
	local send = function()
		M.send(draft.text)
		draft.text = ""
	end

	local sidebar = {}
	for _, ch in ipairs(M.chat.channels or {}) do
		sidebar[#sidebar + 1] = channel_button(ch)
	end

	return ui.row({
		id = "chat",
		grow = true,
		stretch = true,
		fill = C.bg,
		ui.col({ w = 160, no_shrink = true, gap = 4, pad = 10, fill = C.panel, sidebar }),
		ui.col({
			grow = true,
			stretch = true,
			ui.row({
				pad = 10,
				gap = 8,
				ui.input({
					id = "search",
					value = find.q,
					grow = true,
					on_input = function(v)
						find.q = v
					end,
					on_esc = function()
						find.q = ""
					end,
				}),
			}),
			body(find.q),
			ui.row({
				pad = 10,
				gap = 8,
				ui.input({
					id = "composer",
					value = draft.text,
					grow = true,
					on_input = function(v)
						draft.text = v
					end,
					on_enter = send,
				}),
				ui.button({
					id = "send",
					px = 14,
					radius = 6,
					center = true,
					fill = C.accent,
					hover_fill = C.accent_hi,
					ui.text({ S.editing and "save" or "send", color = "#ffffff", font_size = 13, no_wrap = true }),
					on_click = send,
				}),
			}),
		}),
	})
end
