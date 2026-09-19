-- Data, plot geometry (frame-local units) and the pre-built visuals.
local M = {}

M.colors = {
    bg = "#0d1117",
    panel = "#161b22",
    grid = "#30363d",
    axis = "#484f58",
    text = "#e6edf3",
    muted = "#8b949e",
    accent = "#58a6ff",
    halo = "#1f3b5c",
}
local C = M.colors

M.data = {
    { label = "Jan", value = 42 },
    { label = "Feb", value = 48 },
    { label = "Mar", value = 45 },
    { label = "Apr", value = 61 },
    { label = "May", value = 58 },
    { label = "Jun", value = 72 },
    { label = "Jul", value = 80 },
    { label = "Aug", value = 76 },
    { label = "Sep", value = 69 },
    { label = "Oct", value = 74 },
    { label = "Nov", value = 88 },
    { label = "Dec", value = 95 },
}

M.W, M.H = 648, 292
M.PAD_X, M.PAD_T, M.PAD_B = 16, 16, 16
M.STEP = (M.W - 2 * M.PAD_X) / (#M.data - 1)
M.MAX, M.TICK = 100, 20
M.HIT = 18

function M.x_of(i) return M.PAD_X + (i - 1) * M.STEP end
function M.y_of(v) return M.PAD_T + (1 - v / M.MAX) * (M.H - M.PAD_T - M.PAD_B) end

-- Nearest point within HIT of (x, y), in frame units; nil when none is near.
function M.nearest(x, y)
    local best, best_d = nil, M.HIT * M.HIT
    for i, p in ipairs(M.data) do
        local dx, dy = x - M.x_of(i), y - M.y_of(p.value)
        local d = dx * dx + dy * dy
        if d <= best_d then best, best_d = i, d end
    end
    return best
end

-- No arc command yet: four cubics approximate a circle.
local function circle(cx, cy, r)
    local k = 0.5522847498 * r
    return gfx.path({
        { "move", cx + r, cy },
        { "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r },
        { "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
        { "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r },
        { "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy },
        { "close" },
    })
end

local grid_cmds = {}
for v = M.TICK, M.MAX, M.TICK do
    local y = M.y_of(v)
    grid_cmds[#grid_cmds + 1] = { "move", 0, y }
    grid_cmds[#grid_cmds + 1] = { "line", M.W, y }
end

local line_cmds, area_cmds = {}, { { "move", M.x_of(1), M.y_of(0) } }
for i, p in ipairs(M.data) do
    local x, y = M.x_of(i), M.y_of(p.value)
    line_cmds[#line_cmds + 1] = { i == 1 and "move" or "line", x, y }
    area_cmds[#area_cmds + 1] = { "line", x, y }
end
area_cmds[#area_cmds + 1] = { "line", M.x_of(#M.data), M.y_of(0) }
area_cmds[#area_cmds + 1] = { "close" }

local accent = gfx.solid(C.accent)
local bg = gfx.solid(C.bg)

local base = {
    width = M.W,
    height = M.H,
    gfx.stroke({ path = gfx.path(grid_cmds), brush = gfx.solid(C.grid), width = 1, dashes = { 3, 4 } }),
    gfx.stroke({
        path = gfx.path({ { "move", 0, M.y_of(0) }, { "line", M.W, M.y_of(0) } }),
        brush = gfx.solid(C.axis),
        width = 1,
    }),
    gfx.fill({
        path = gfx.path(area_cmds),
        brush = gfx.linear_gradient({
            from = { 0, M.PAD_T },
            to = { 0, M.y_of(0) },
            stops = { { 0, "#58a6ff55" }, { 1, "#58a6ff00" } },
            extend = "pad",
        }),
    }),
    gfx.stroke({ path = gfx.path(line_cmds), brush = accent, width = 2.5, cap = "round", join = "round" }),
}
for i, p in ipairs(M.data) do
    local dot = circle(M.x_of(i), M.y_of(p.value), 3.5)
    base[#base + 1] = gfx.fill({ path = dot, brush = bg })
    base[#base + 1] = gfx.stroke({ path = dot, brush = accent, width = 2 })
end
M.base = gfx.frame(base)

-- One visual per highlighted point, built once so the view only picks.
M.highlighted = {}
local guide_brush = gfx.solid(C.axis)
local halo = gfx.solid(C.halo)
for i, p in ipairs(M.data) do
    local x, y = M.x_of(i), M.y_of(p.value)
    local dot = circle(x, y, 5.5)
    M.highlighted[i] = gfx.frame({
        width = M.W,
        height = M.H,
        gfx.stroke({
            path = gfx.path({ { "move", x, 0 }, { "line", x, M.y_of(0) } }),
            brush = guide_brush,
            width = 1,
            dashes = { 2, 3 },
        }),
        gfx.instance({ visual = M.base }),
        gfx.fill({ path = circle(x, y, 11), brush = halo }),
        gfx.fill({ path = dot, brush = accent }),
        gfx.stroke({ path = dot, brush = bg, width = 2 }),
    })
end

return M
