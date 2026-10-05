-- MODEL
--
-- `chat` holds the channel list; each channel's messages live in their own doc,
-- `channel:<id>`, so one busy channel never makes another's doc large. Search spans them all
-- because `index.lua` matches `channel:*`.
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
		d = doc:open("channel:" .. cid)
		docs[cid] = d
	end
	return d
end

-- Seeded so a fresh app (and the app-shipped tests' empty tab) has something to find.
local general = M.channel("general")
if not general.messages then
	general:set({ "messages" }, doc.list({
		doc.map({ id = "seed-1", author = "anu", text = "the deploy is green", sent_at = 1 }),
		doc.map({ id = "seed-2", author = "abe", text = "lunch at noon?", sent_at = 2 }),
	}))
end

local function message(text)
	return doc.map({ id = uuid(), author = "me", text = text, sent_at = now() })
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
		d:insert({ "messages" }, message(text))
	else
		d:set({ "messages" }, doc.list({ message(text) }))
	end
end

function M.delete(id)
	M.channel(M.S.channel):delete({ "messages", id })
	if M.S.editing == id then
		M.S.editing = nil
	end
end

-- "channel:design" → "design"
function M.channel_of(doc_name)
	return doc_name:match("^channel:(.+)$")
end

return M
