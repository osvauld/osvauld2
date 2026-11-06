local C = require("lib/theme")
local model = require("model")
local M = {}

local function text(value, size, color, id)
	return ui.text({ value, id = id, font_size = size, color = color or C.ink })
end

local function button(id, title, action, color)
	return ui.button({
		id = "action:" .. id, h = 32, px = 12, radius = 7, center = true,
		fill = C.panel, stroke = { 1, color or C.line }, hover_stroke = { 1, C.accent },
		cursor = "pointer", on_click = action, text(title, 12, color or C.ink),
	})
end

local function replica(m, ctx, who)
	local name, color, bg = who == "alice" and "Alice" or "Bob", C[who], C[who .. "_bg"]
	local slots = {}
	if not m.naive then
		for _, owner in ipairs({ "alice", "bob" }) do
			slots[#slots + 1] = ui.col({ grow = true, gap = 6, center = true,
				text(owner == "alice" and "Alice's slot" or "Bob's slot", 11, C.muted),
				text(tostring(m.replicas[who][owner]), 24, C[owner], "counter-slot:" .. who .. ":" .. owner),
			})
		end
	end
	return ui.col({
		grow = true, min_w = 210, pad = 20, gap = 12, radius = 14, fill = C.panel, stroke = { 1, C.line },
		ui.row({ align_center = true, gap = 9,
			ui.col({ w = 26, h = 26, radius = 13, fill = bg, center = true, text(name:sub(1, 1), 12, color) }),
			text(name .. "'s replica", 14, color),
		}),
		ui.col({ center = true, gap = 3,
			text(tostring(model.total(m, who)), 66, color, "counter-total:" .. who),
			text("DISPLAYED TOTAL", 10, C.muted),
		}),
		m.naive and ui.col({ h = 56, center = true, text("Only a scalar is stored", 12, C.muted) })
			or ui.row({ h = 56, stretch = true, slots }),
		ctx.mode ~= "record" and button("inc-" .. who, "+1 for " .. name, function() ctx.act("inc-" .. who) end, color) or false,
	})
end

local wire = gfx.path({ { "move", 8, 16 }, { "line", 96, 16 }, { "move", 96, 30 }, { "line", 8, 30 } })
local arrow = gfx.path({
	{ "move", 96, 16 }, { "line", 88, 11 }, { "line", 88, 21 }, { "close" },
	{ "move", 8, 30 }, { "line", 16, 25 }, { "line", 16, 35 }, { "close" },
})
local connection = gfx.frame({ width = 104, height = 44,
	gfx.stroke({ path = wire, brush = gfx.solid(C.line), width = 2 }),
	gfx.fill({ path = arrow, brush = gfx.solid(C.line) }),
})

function M.view(m, ctx)
	local controls = {}
	if ctx.mode ~= "record" then
		if m.naive then
			controls[#controls + 1] = button("copy-total", "Copy Alice's total → Bob", function() ctx.act("copy-total") end, C.warn)
		else
			for _, action in ipairs({
				{ "send-alice", "Queue Alice → Bob" }, { "send-bob", "Queue Bob → Alice" },
				{ "deliver-first", "Deliver oldest" }, { "deliver-last", "Deliver newest" },
				{ "redeliver", "Repeat last" },
			}) do
				controls[#controls + 1] = button(action[1], action[2], function() ctx.act(action[1]) end)
			end
		end
	end
	local packets = {}
	for _, packet in ipairs(m.queue) do
		packets[#packets + 1] = ui.col({ pad = 10, gap = 4, radius = 7, fill = C.panel, stroke = { 1, C.line },
			text("#" .. packet.id .. "  " .. packet.from .. " → " .. packet.to, 10, C.muted),
			text(string.format("[ %d, %d ]", packet.state.alice, packet.state.bob), 17, C[packet.from]),
		})
	end
	return ui.col({ gap = 14,
		ui.row({ gap = 14, stretch = true,
			replica(m, ctx, "alice"),
			ui.col({ w = 114, center = true, gap = 12,
				text(m.connected and "CONNECTED" or "OFFLINE", 10, m.connected and C.bob or C.warn, "counter-network"),
				ui.col({ w = 104, h = 44,
					ui.frame({ visual = connection }),
					ctx.playing and ctx.last_action and ctx.last_action:find("deliver") and ui.col({
						absolute = true,
						left = m.last and m.last.from == "bob" and 88 - 80*ctx.motion or 8 + 80*ctx.motion,
						top = m.last and m.last.from == "bob" and 25 or 11,
						w = 10, h = 10, radius = 5, fill = C.accent,
					}) or false,
				}),
				ctx.mode ~= "record" and not m.naive and button("network", m.connected and "Disconnect" or "Connect", function() ctx.act("network") end) or false,
			}),
			replica(m, ctx, "bob"),
		}),
		ui.row({ pad = 14, gap = 16, radius = 9, fill = m.naive and C.warn_bg or C.soft,
			text(m.naive and "SCALAR COPY" or "THE MERGE RULE", 10, m.naive and C.warn or C.accent),
			text(m.naive and "replace a total → a contribution is lost" or "slot = max(local, incoming)     ·     total = Alice + Bob", 12),
		}),
		ui.row({ wrap = true, gap = 6, controls }),
		ui.row({ gap = 12, align_center = true,
			text(#m.queue .. (#m.queue == 1 and " snapshot waiting" or " snapshots waiting"), 12, C.ink, "counter-queue"),
			ui.col({ grow = true }),
			text(m.deliveries .. " deliveries", 11, C.muted, "counter-deliveries"),
			text(m.clicks .. " clicks happened", 11, C.muted, "counter-expected"),
		}),
		ui.row({ wrap = true, gap = 7, packets }),
		ui.col({ pad = 14, radius = 9, fill = C.panel, text(m.event, 12, C.muted, "counter-event") }),
	})
end

return M
