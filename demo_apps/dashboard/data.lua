-- 24 months of hard-coded data, three series.
local M = {}

M.months = {
	"2024-01", "2024-02", "2024-03", "2024-04", "2024-05", "2024-06",
	"2024-07", "2024-08", "2024-09", "2024-10", "2024-11", "2024-12",
	"2025-01", "2025-02", "2025-03", "2025-04", "2025-05", "2025-06",
	"2025-07", "2025-08", "2025-09", "2025-10", "2025-11", "2025-12",
}

M.series = {
	{
		key = "users",
		title = "Active users",
		y_label = "users / month",
		color = "#58a6ff",
		stat = "avg", -- summary tile shows the average
		stat_label = "Average users",
		fmt = "%.0f",
		values = {
			1200, 1320, 1410, 1560, 1640, 1710,
			1880, 1930, 2100, 2240, 2310, 2480,
			2520, 2610, 2790, 2860, 3010, 3180,
			3240, 3400, 3520, 3680, 3810, 3990,
		},
	},
	{
		key = "revenue",
		title = "Revenue",
		y_label = "USD / month",
		color = "#3fb950",
		stat = "total",
		stat_label = "Total revenue",
		fmt = "%.0f",
		prefix = "$",
		values = {
			9200, 9800, 10400, 11500, 12100, 12800,
			13900, 14300, 15600, 16800, 17200, 18400,
			19100, 19800, 21000, 21600, 22800, 24100,
			24600, 25800, 26900, 28100, 29200, 30500,
		},
	},
	{
		key = "errors",
		title = "Error rate",
		y_label = "% of requests",
		color = "#f778ba",
		stat = "avg",
		stat_label = "Average error rate",
		fmt = "%.2f",
		suffix = "%",
		values = {
			2.4, 2.2, 2.5, 2.1, 1.9, 2.0,
			1.7, 1.8, 1.6, 1.5, 1.7, 1.4,
			1.3, 1.5, 1.2, 1.1, 1.3, 1.0,
			1.1, 0.9, 1.0, 0.8, 0.9, 0.7,
		},
	},
}

M.n = #M.months

-- formatted number in the series' own units
function M.fmt_value(s, v)
	if v == nil then return "-" end
	return (s.prefix or "") .. string.format(s.fmt, v) .. (s.suffix or "")
end

return M
