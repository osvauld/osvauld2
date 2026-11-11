-- The fight's HUD (docs/design/brawl-fighting.md decision 14): percent and stocks per fighter along
-- the bottom, the training line (the dummy's last hit and the combo), the mode button, and the line
-- that ends a match. View only: it reads match state and never writes it.
local C = require("theme")
local H = {}

local NAMES = { human = "You", dummy = "Dummy", bot = "CPU" }
H.MODES = { none = "Solo", dummy = "Dummy", bot = "CPU", bots = "CPU vs CPU" }

local function num(v) return string.format("%g", v) end

local function stocks_of(f, T)
	if f.stocks < 0 then return "∞ · outs " .. f.outs end
	return string.rep("● ", f.stocks) .. string.rep("○ ", math.max(0, T.stocks - f.stocks))
end

-- `s` the match state (or nil before the first step), `mode` "none" | "dummy" | "bot", `next_mode` the click.
function H.view(s, mode, T, next_mode)
	local out = {}
	out[#out + 1] = ui.button({ id = "mode", absolute = true, left = 16, top = 12, px = 10, py = 4, radius = 6,
		fill = "#00000055", hover_fill = "#00000088", on_click = next_mode,
		ui.text({ H.MODES[mode], id = "mode:label", color = C.text, font_size = 13, no_wrap = true }) })
	if not s then return out end
	local f1 = s.fighters[1]
	local cards = {}
	for i, f in ipairs(s.fighters) do
		cards[#cards + 1] = ui.col({ align_center = true, gap = 2, px = 12,
			ui.text({ NAMES[f.ctl] or f.ctl, color = C.muted, font_size = 12, no_wrap = true }),
			ui.text({ num(f.dmg) .. "%", id = "hud:dmg:" .. i, color = f.dmg >= 100 and C.danger or C.text, font_size = 24, no_wrap = true }),
			ui.text({ stocks_of(f, T), id = "hud:stocks:" .. i, color = C.muted, font_size = 11, no_wrap = true }),
		})
	end
	out[#out + 1] = ui.row({ absolute = true, bottom = 30, w_full = true, center = true, gap = 24,
		ui.row({ gap = 24, px = 8, py = 4, radius = 8, fill = "#00000066", cards }) })
	for i, f in ipairs(s.fighters) do
		if f.ctl == "dummy" then
			local line = f.last_dmg > 0 and string.format("last hit %s dmg · kb %s · %s° · stun %s · combo %s", num(f.last_dmg),
				num(f.last_kb), num(f.last_angle), num(f.last_stun), num(f.combo)) or "hit the dummy: J light, K heavy"
			out[#out + 1] = ui.row({ absolute = true, right = 16, top = 66, px = 10, py = 4, radius = 6, fill = "#000000aa",
				ui.text({ line, id = "hud:train:" .. i, color = C.text, font_size = 12, no_wrap = true }) })
		end
	end
	local over
	if s.winner ~= 0 then
		local w = s.fighters[s.winner]
		over = (w.ctl == "human" and "Player " .. s.winner or NAMES[w.ctl]) .. " wins — press Enter"
	elseif f1.act == "ko" then
		over = "KO — press Enter"
	end
	if over then
		out[#out + 1] = ui.col({ absolute = true, top = 80, w_full = true, center = true,
			ui.text({ over, id = "hud:ko", color = C.danger, font_size = 22, no_wrap = true }) })
	end
	out[#out + 1] = ui.col({ absolute = true, bottom = 10, w_full = true, center = true,
		ui.text({ "A/D or ←/→ move · Space jump · S/↓ drop, fast fall · Shift dash · J light · K heavy (+ direction; in the air, held: jet) · L dodge · F2 hitboxes · Enter restart",
			color = C.muted, font_size = 11, no_wrap = true }) })
	return out
end

return H
