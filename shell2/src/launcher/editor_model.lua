local text = require("editor_text")
local M = {}

function M.state()
	return ui.state("launcher:editor", {
		open = false, path = "theme.lua", draft = "", base = "", revision = "", generation = 0,
	})
end

function M.open(path)
	local source = shell.read_launcher_file(path)
	local S = M.state()
	S.open, S.path = true, path
	S.draft, S.base = source.content, source.content
	S.revision, S.generation = source.revision, source.generation
end

function M.visible()
	local S = M.state()
	if not S.open then return false end
	return shell.read_launcher_file(S.path).generation == S.generation
end

function M.input(value)
	local S = M.state()
	S.draft = text.apply(S.draft, value)
end

function M.value()
	return text.display(M.state().draft)
end

function M.cancel()
	M.state().open = false
end

function M.save()
	local S = M.state()
	shell.edit_launcher_file(S.path, S.revision, {
		{ old_text = S.base, new_text = S.draft },
	})
end

return M
