local M = {}

function M.display(raw)
	return (raw:gsub("\t", "    "))
end

-- Translate a display boundary back to raw bytes. Partial-tab edits replace the whole tab;
-- unchanged prefixes/suffixes retain their original tabs, including those inside strings.
local function boundary(raw, at, toward_end)
	local pos = 0
	for i = 1, #raw do
		if at == pos then return i - 1, pos end
		local size = raw:byte(i) == 9 and 4 or 1
		if at < pos + size then
			if toward_end then return i, pos + size end
			return i - 1, pos
		end
		pos = pos + size
	end
	return #raw, pos
end

function M.apply(raw, value)
	local display = M.display(raw)
	if display == value then return raw end
	local prefix, suffix = 0, 0
	local limit = math.min(#display, #value)
	while prefix < limit and display:byte(prefix + 1) == value:byte(prefix + 1) do
		prefix = prefix + 1
	end
	while suffix < limit - prefix and display:byte(#display - suffix) == value:byte(#value - suffix) do
		suffix = suffix + 1
	end
	local first, display_first = boundary(raw, prefix, false)
	local last, display_last = boundary(raw, #display - suffix, true)
	local inserted = value:sub(display_first + 1, #value - (#display - display_last))
	return raw:sub(1, first) .. inserted .. raw:sub(last + 1)
end

return M
