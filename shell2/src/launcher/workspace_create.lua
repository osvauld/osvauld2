local C = require("theme")

local function button(id, label, action, accent)
	return ui.button({
		id = id, h = 36, px = 14, center = true, radius = 8,
		fill = accent and C.accent or C.panel, hover_fill = accent and C.accent or C.hover,
		stroke = { 1, C.line }, on_click = action,
		ui.text({ label, font_size = 13, no_wrap = true, color = accent and C.bg or C.text }),
	})
end

return function(status)
	local S = ui.state("launcher:workspace-create", { open = false, draft = "", submitted = -1, error = "" })
	local succeeded = S.submitted >= 0 and status.generation > S.submitted and not status.error
	local opened = S.open and not succeeded
	local function cancel()
		if status.pending then return end
		S.open, S.draft, S.error, S.submitted = false, "", "", -1
	end
	local function submit()
		if status.pending then return end
		if #S.draft > 256 then S.error = "workspace name must be at most 256 bytes"; return end
		if not S.draft:find("%S") then S.error = "enter a workspace name"; return end
		local ok, error = pcall(shell.create_workspace, S.draft)
		if not ok then S.error = tostring(error); return end
		S.submitted, S.error = status.generation, ""
	end
	local body
	if opened then
		local error = S.error ~= "" and S.error or (S.submitted >= 0 and status.generation > S.submitted and status.error or nil)
		body = ui.col({
			gap = 10, stretch = true,
			ui.text({ "Workspace name", color = C.muted, font_size = 12 }),
			ui.input({
				id = "launcher:create:name", value = S.draft, h = 40, px = 10,
				fill = C.panel, color = C.text, radius = 8, stroke = { 1, C.line }, font_size = 14,
				autofocus = true, on_enter = not status.pending and submit or nil, on_esc = cancel,
				on_input = function(value) if not status.pending then S.draft, S.error = value, "" end end,
			}),
			error and ui.text({ error, color = C.accent, font_size = 12 }) or false,
			ui.row({ wrap = true, gap = 8,
				button("launcher:create:submit", status.pending and "Creating…" or "Create", not status.pending and submit or nil, true),
				button("launcher:create:cancel", "Cancel", not status.pending and cancel or nil, false),
			}),
		})
	else
		body = button("launcher:new-workspace", status.pending and "Creating…" or "+ New workspace", not status.pending and function()
			if succeeded then S.draft = "" end
			S.open, S.submitted, S.error = true, -1, ""
		end or nil, true)
	end
	return ui.col({ id = "launcher:workspace-create", stretch = true, body })
end
