-- MODEL
--
-- `chat` holds the channel list; each channel's messages live in their own doc,
-- `channel/<id>`, so one busy channel never makes another's doc large. Search spans them all
-- because `index.lua` matches `channel/*`.
local M = { S = { channel = "general", editing = nil } }

local chat = doc:open("chat")
if not chat.channels then
	chat:set({ "channels" }, doc.list({
		doc.map({ id = "general", name = "general" }),
		doc.map({ id = "design", name = "design" }),
	}))
end
M.chat = chat

local docs = {}
function M.channel(cid)
	local d = docs[cid]
	if not d then
		d = doc:open("channel/" .. cid)
		docs[cid] = d
	end
	return d
end

-- `author` is a display label, written by the sender; who may edit or delete a message is
-- decided on the Loro ID (`own messages/*` in the manifest), never on this field.
local function message(id, text, sent_at)
	return doc.map({ id = id, author = me().name, text = text, sent_at = sent_at })
end

-- Seeded so a fresh app (and the app-shipped tests' empty tab) has something to find.
local general = M.channel("general")
if not general.messages then
	general:set({ "messages" }, doc.list({
		message("seed-1", "the deploy is green", 1),
		message("seed-2", "lunch at noon?", 2),
	}))
end

-- Read the mirror before writing: `d.messages` is a frame behind, and only decides whether the
-- list exists yet.
function M.send(text)
	if text == "" then
		return
	end
	local S, d = M.S, M.channel(M.S.channel)
	if S.editing then
		d:set({ "messages", S.editing, "text" }, text)
		S.editing = nil
	elseif d.messages then
		d:insert({ "messages" }, message(uuid(), text, now()))
	else
		d:set({ "messages" }, doc.list({ message(uuid(), text, now()) }))
	end
end

-- Asked per row, every view: the engine answers from a cache until the doc changes.
function M.can_delete(m)
	return M.channel(M.S.channel):allowed("delete", { "messages", m.id })
end

function M.delete(id)
	M.channel(M.S.channel):delete({ "messages", id })
	if M.S.editing == id then
		M.S.editing = nil
	end
end

-- "channel/design" → "design"
function M.channel_of(doc_name)
	return doc_name:match("^channel/(.+)$")
end

return M
