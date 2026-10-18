local view = require("spaces")
local create = require("workspace_create")

return function()
	local snapshot = shell.snapshot()
	return view(snapshot.workspaces, shell.select_workspace, create(snapshot.creation))
end
