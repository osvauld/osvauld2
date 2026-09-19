local chart = require("chart")
local C = chart.colors

-- `absolute` is viewport coordinates, so the plot sits at a fixed spot and
-- labels/tooltip are placed relative to it.
local LEFT, TOP = 72, 88
local TIP_W = 104

local S = { hover = nil } -- index of the highlighted point

local function on_plot_hover(e)
	local phase, x, y = e.phase, e.x, e.y
    if phase == "leave" then
        S.hover = nil
    else
        S.hover = chart.nearest(x, y)
    end
end

local function y_labels()
    local out = {}
    for v = 0, chart.MAX, chart.TICK do
        out[#out + 1] = ui.row({
            absolute = true,
            top = TOP + chart.y_of(v) - 8,
            left = LEFT - 44,
            w = 36,
            h = 16,
            align_center = true,
            ui.col({ grow = true }),
            ui.text({ tostring(v), color = C.muted, font_size = 11, no_wrap = true }),
        })
    end
    return out
end

local function x_labels()
    local out = {}
    for i, p in ipairs(chart.data) do
        out[#out + 1] = ui.row({
            absolute = true,
            top = TOP + chart.H + 4,
            left = LEFT + chart.x_of(i) - 20,
            w = 40,
            h = 16,
            center = true,
            ui.text({ p.label, color = C.muted, font_size = 11, no_wrap = true }),
        })
    end
    return out
end

local function tooltip(i)
    local p = chart.data[i]
    local px, py = chart.x_of(i), chart.y_of(p.value)
    local left = LEFT + px + 14
    if px + 14 + TIP_W > chart.W then left = LEFT + px - 14 - TIP_W end
    local top = TOP + py - 56
    if py < 60 then top = TOP + py + 14 end
    return ui.col({
        id = "tooltip",
        absolute = true,
        top = top,
        left = left,
        w = TIP_W,
        pad = 8,
        gap = 2,
        radius = 6,
        fill = C.panel,
        stroke = { 1, C.axis },
        fade_in = 120,
        ui.text({ p.label .. " 2026", color = C.muted, font_size = 11, no_wrap = true }),
        ui.text({ tostring(p.value) .. "k users", color = C.text, font_size = 15, no_wrap = true }),
    })
end

return function()
    local h = S.hover
    return ui.col({
        full = true,
        fill = C.bg,
        ui.text({ "Monthly active users", absolute = true, top = 24, left = LEFT, color = C.text, font_size = 20, no_wrap = true }),
        ui.text({ "Thousands, 2026. Hover a point.", absolute = true, top = 54, left = LEFT, color = C.muted, font_size = 12, no_wrap = true }),
        ui.frame({
            id = "plot",
            visual = h and chart.highlighted[h] or chart.base,
            absolute = true,
            top = TOP,
            left = LEFT,
            on_hover = on_plot_hover,
        }),
        y_labels(),
        x_labels(),
        h and tooltip(h) or false,
    })
end
