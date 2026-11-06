local M = {}

M.nodes = {
	{
		id = "edit", title = "App document", caption = "An edit enters Loro", x = 24, y = 80,
		body = "The Lua handler writes to the app's state document. The live document changes locally before the node is contacted. App source and app data are separate documents.",
		source = "app_host/src/crdt.rs",
	},
	{
		id = "vault", title = "Local vault", caption = "Sealed local persistence", x = 24, y = 250,
		body = "The app thread saves its document snapshot in the account's sealed vault. Losing a connection does not undo an ordinary local save. Playback and selection in this explainer are private viewer state.",
		source = "shell2/src/app_thread.rs",
	},
	{
		id = "policy", title = "Permission check", caption = "Identity, scope, document rules", x = 308, y = 80,
		body = "The node verifies the signed request and the token chain, scope and revocations. It evaluates document access from its recorded grants and the app manifest. A local edit does not grant permission to sync it.",
		source = "courier/src/access.rs · courier/src/proof.rs",
	},
	{
		id = "merge", title = "Merge & persist", caption = "Accepted CRDT changes", x = 308, y = 250,
		body = "Accepted changes merge into the node's document. The caller persists the resulting snapshot. SyncAck returns changes the sender lacks and the node's version vector; the sender uses that acknowledged version for its next diff.",
		source = "courier/src/sync.rs · kunki/src/admin.rs",
	},
	{
		id = "listen", title = "Subscriber delivery", caption = "Read access checked again", x = 592, y = 80,
		body = "The node pushes to subscribed desktops that may still read this document. A push is best effort, not proof of receipt. A receiver's later Sync repairs a lost delivery; Listen reconnect also resubscribes and syncs.",
		source = "kunki/src/push.rs · shell2/src/node.rs",
	},
	{
		id = "peer", title = "Peer app", caption = "Import, save, repaint", x = 592, y = 250,
		body = "The receiving desktop imports the accepted CRDT changes. An open app refreshes its document mirror and renders the new state. Peers can edit concurrently; each next sync exchanges the changes the other side lacks.",
		source = "shell2/src/app_thread.rs · shell2/src/node.rs",
	},
}

M.by_id = {}
for _, node in ipairs(M.nodes) do M.by_id[node.id] = node end

local edit = {
	node = "edit", title = "Make a local change",
	body = "A person or agent changes the app's state. The edit enters the local Loro document immediately.",
	changes = { edit = "Local change" }, outcome = "The edit starts on your desktop",
}
local save = {
	node = "vault", edge = "save", title = "Keep it locally",
	body = "The app thread saves a snapshot in the sealed account vault. This local save does not wait for a network round trip.",
	changes = { vault = "Saved locally" }, outcome = "Your local edit is saved",
}
local authorize = {
	node = "policy", edge = "sync", title = "Ask the node to accept the edit",
	body = "Send a signed Sync request with changes since the last acknowledged version. The node verifies the request, token scope and document read/write rules.",
	changes = { policy = "Allowed" }, outcome = "The request passes the access checks",
}
local merge = {
	node = "merge", edge = "merge", title = "Merge and acknowledge",
	body = "The node merges accepted changes and persists its snapshot. Its acknowledgment returns missing changes and a version vector to the sender. The node is trusted with document content.",
	changes = { merge = "Accepted & saved", vault = "Acknowledged" }, outcome = "The node has accepted the update",
}
local push = {
	node = "listen", edge = "push", title = "Deliver to an authorized subscriber",
	body = "Before pushing, the node checks the subscriber's current read access. Delivery is best effort: a later Sync recovers a lost push.",
	changes = { listen = "Push sent" }, outcome = "An authorized subscriber receives the update",
}
local apply = {
	node = "peer", edge = "apply", title = "Apply and show the result",
	body = "The other desktop imports the CRDT update, saves locally and refreshes the app. This is the delivered path; a missed push catches up through a later Sync.",
	changes = { peer = "Updated" }, outcome = "Both desktops have the accepted update",
}

M.scenarios = {
	success = {
		label = "Successful sync", short = "SUCCESS", color = "mint",
		stages = { edit, save, authorize, merge, push, apply },
	},
	offline = {
		label = "Sender offline", short = "OFFLINE", color = "amber",
		stages = {
			edit, save,
			{
				node = "vault", edge = "sync", waiting = true, title = "The node cannot be reached",
				body = "The local edit remains saved. No request has reached the node, so the node and the peer are unchanged. Reconnect to continue the explanation.",
				changes = { vault = "Saved; offline", policy = "Not reached" },
				outcome = "Saved locally; waiting for the node",
			},
			authorize, merge, push, apply,
		},
	},
	denied = {
		label = "Write denied", short = "DENIED", color = "red",
		stages = {
			edit, save,
			{
				node = "policy", edge = "sync", refused = true, title = "The document write is refused",
				body = "In this example the writer still has read access, but lacks write access. The node rejects this batch without persisting it or pushing it to the peer.",
				changes = { policy = "Write denied" }, outcome = "The node does not accept this write",
			},
			{
				node = "vault", edge = "rollback", refused = true, title = "Recover the writer's accepted state",
				body = "The rejection supplies the readable node state. The writer performs a rollback to accepted state and saves it locally. The node and peer keep their accepted content; the refused batch is not shared as a compensating edit.",
				changes = { vault = "Rolled back", edit = "Accepted state" },
				outcome = "Write refused; peer unchanged",
			},
		},
	},
}

function M.statuses(scenario, step)
	local out = {
		edit = "Ready", vault = "Not saved", policy = "Not checked",
		merge = "Unchanged", listen = "Waiting", peer = "Unchanged",
	}
	for i = 1, step do
		for id, status in pairs(M.scenarios[scenario].stages[i].changes) do out[id] = status end
	end
	return out
end

return M
