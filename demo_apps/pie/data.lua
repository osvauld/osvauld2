-- what the pie is of. Hard-coded: this app is about touching shapes, not about data.
local M = {}

M.slices = {
	{ key = "search", label = "Search", value = 4210, color = "#58a6ff" },
	{ key = "direct", label = "Direct", value = 2880, color = "#3fb950" },
	{ key = "social", label = "Social", value = 1640, color = "#d29922" },
	{ key = "mail", label = "Email", value = 980, color = "#f778ba" },
	{ key = "refer", label = "Referral", value = 610, color = "#a371f7" },
	{ key = "other", label = "Other", value = 240, color = "#8b949e" },
}

M.total = 0
for _, s in ipairs(M.slices) do
	M.total = M.total + s.value
end

function M.find(key)
	for i, s in ipairs(M.slices) do
		if s.key == key then return s, i end
	end
end

return M
