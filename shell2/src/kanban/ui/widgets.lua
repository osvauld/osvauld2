-- Small pieces that draw and nothing else.
--
-- The boundary here is not "small" but *pure*: every function below is a function of its
-- arguments and the palette. None of them touch the board, the pointer state or `update`, which
-- is what lets them sit in another file without anything being passed down to reach them.
local C = require("theme")

local M = {}

-- Guides are rendered on *every* gap, always, so the space is permanently reserved and nothing
-- shifts when a drop target appears — only the line's alpha changes. `fade` is keyed by id:
-- that's where the runtime parks the tween, so retargeting mid-flight reverses smoothly.
function M.guide(id, on)
	return ui.col({
		py = 3,
		w_full = true,
		ui.col({ id = id, h = 2, radius = 1, fill = C.accent, fade = { on and 1 or 0, 140 } }),
	})
end

-- Vertical twin, for column reordering. No height: the board row is `stretch`, so it fills.
function M.guide_v(id, on)
	return ui.col({
		px = 7,
		ui.col({
			id = id,
			w = 2,
			grow = true,
			radius = 1,
			fill = C.accent,
			fade = { on and 1 or 0, 140 },
		}),
	})
end

-- The resize grip: a hairline in the gutter to the right of a column. Same shape as `guide_v` on
-- purpose — it stands in the same kind of gap and must not push the columns around by being there.
--
-- Faint but never invisible. `guide_v` can fade to nothing because a drop target announces itself
-- when it becomes relevant; a grip has to be found before it is useful, and a control you can only
-- hit by guessing where it is is not a control. Hover brightens it, dragging holds it bright.
--
-- Pure like everything else here: it is handed the handler rather than reaching for `update`.
function M.grip(id, active, on_drag)
	return ui.col({
		px = 4,
		ui.col({
			id = id,
			w = 3,
			grow = true,
			radius = 2,
			fill = active and C.accent or C.line,
			hover_fill = C.accent,
			on_drag = on_drag,
		}),
	})
end

-- Both of these sit on the *main* axis of a row next to text that can be any length — the badge
-- beside a column name, the button beside a card's body. A flex item shrinks to its min-content
-- width before its neighbour gives way, so without `no_shrink` a long card title squashes the
-- delete button until its glyph wraps, and the control the user is reaching for is the one that
-- disappears. Grows nothing, costs nothing, and only matters when space runs out.
function M.badge(n)
	return ui.row({
		px = 7,
		py = 1,
		radius = 10,
		fill = C.line_soft,
		center = true,
		no_shrink = true,
		ui.text({ tostring(n), no_wrap = true, color = C.muted, font_size = 11 }),
	})
end

function M.icon_button(glyph, on_press, hover)
	return ui.button({
		w = 22,
		h = 22,
		radius = 6,
		center = true,
		no_shrink = true,
		hover_fill = hover or C.line,
		ui.text({ glyph, no_wrap = true, color = C.muted, font_size = 13 }),
		on_click = on_press,
	})
end

function M.empty_slot(on)
	return ui.col({
		h = 64,
		center = true,
		radius = 8,
		stroke = { 1, on and C.accent or C.line_soft },
		ui.text({ "drop a card here", no_wrap = true, color = on and C.accent or C.muted, font_size = 12 }),
	})
end

return M
