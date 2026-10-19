"""Sources for the `agent_builds` clip in demo_reel.py: a to-do app an "agent" grows live.

`V0` is the starting `main.lua`; `EDITS` are the exact `edit_file` replacements the agent makes
(each is applied to the source as the previous edit left it, so every `old` must occur once);
`STATS_LUA` is the one new file. The clip runs all of it through the real bridge — nothing here
is mocked — and `check()` replays the edits in plain Python so a drifted anchor fails loudly.
"""

import atexit
import shutil
import tempfile
from pathlib import Path

V0 = '''\
-- Launch checklist. Tasks live in the "todo" document, so every reload keeps them.
local t = doc:open("todo")

if not t.tasks then
    t:set({ "tasks" }, doc.list({
        doc.map({ id = "t1", text = "Write the launch post", done = false, prio = "high" }),
        doc.map({ id = "t2", text = "Record the demo reel", done = true, prio = "high" }),
        doc.map({ id = "t3", text = "Fix the pricing page copy", done = false, prio = "med" }),
        doc.map({ id = "t4", text = "Reply to early testers", done = false, prio = "med" }),
        doc.map({ id = "t5", text = "Rotate the node keys", done = true, prio = "high" }),
        doc.map({ id = "t6", text = "Tidy the changelog", done = false, prio = "low" }),
    }))
end

local C = {
    bg = "#0d1117",
    card = "#1c2128",
    card_hi = "#22282f",
    sunken = "#0d1117",
    line = "#30363d",
    text = "#e6edf3",
    muted = "#8b949e",
    accent = "#2f81f7",
    accent_hi = "#4493f8",
}

local function row(task)
    return ui.button({
        id = "task:" .. task.id,
        w_full = true, h = 38, px = 14, gap = 12, radius = 10, align_center = true,
        fill = C.card, hover_fill = C.card_hi, press_scale = 0.99,
        ui.col({ w = 18, h = 18, radius = 6, fill = task.done and C.accent or C.sunken, stroke = { 1, task.done and C.accent or C.line } }),
        ui.text({ task.text, color = task.done and C.muted or C.text, font_size = 15, no_wrap = true }),
        on_click = function()
            t:set({ "tasks", task.id, "done" }, not task.done)
        end,
    })
end

return function()
    local rows = {}
    local done = 0
    for _, task in ipairs(t.tasks) do
        if task.done then done = done + 1 end
        table.insert(rows, row(task))
    end
    return ui.col({
        id = "todo",
        full = true, align_center = true, fill = C.bg, py = 22,
        ui.col({
            w = 640, gap = 14,
            ui.row({
                align_center = true,
                ui.col({
                    gap = 4,
                    ui.text({ "Launch checklist", color = C.text, font_size = 26, no_wrap = true }),
                    ui.text({ done .. " of " .. #t.tasks .. " done", id = "summary", color = C.muted, font_size = 14, no_wrap = true }),
                }),
            }),
            ui.col({ gap = 6, rows }),
        }),
    })
end
'''

STATS_LUA = '''\
-- stats.lua: tasks by priority, drawn straight from the document.
local C = { card = "#161b22", line = "#30363d", text = "#e6edf3", muted = "#8b949e" }
local COLOR = { high = "#f85149", med = "#d29922", low = "#3fb950" }
local W, H = 150, 12
local M = {}

local bars = {}
local function bar(done, total, peak, color)
    local key = done .. ":" .. total .. ":" .. peak .. color
    if not bars[key] then
        local function rect(w)
            return gfx.path({ { "move", 0, 0 }, { "line", w, 0 }, { "line", w, H }, { "line", 0, H }, { "close" } })
        end
        local items = { gfx.fill({ path = rect(W), brush = gfx.solid(C.line) }) }
        if total > 0 then
            table.insert(items, gfx.fill({ path = rect(W * total / peak), brush = gfx.solid("#30363d") }))
            table.insert(items, gfx.fill({ path = rect(math.max(1, W * done / peak)), brush = gfx.solid(color) }))
        end
        items.width, items.height = W, H
        bars[key] = gfx.frame(items)
    end
    return bars[key]
end

function M.view(tasks)
    local count = { high = { 0, 0 }, med = { 0, 0 }, low = { 0, 0 } } -- { done, total }
    for _, task in ipairs(tasks) do
        local c = count[task.prio]
        if c then
            c[2] = c[2] + 1
            if task.done then c[1] = c[1] + 1 end
        end
    end
    local peak = 1
    for _, c in pairs(count) do peak = math.max(peak, c[2]) end

    local cells = {}
    for _, p in ipairs({ "high", "med", "low" }) do
        local done, total = count[p][1], count[p][2]
        table.insert(cells, ui.col({
            gap = 6,
            ui.row({
                gap = 6, align_center = true,
                ui.text({ p, color = COLOR[p], font_size = 12, no_wrap = true }),
                ui.col({ grow = true }),
                ui.text({ done .. "/" .. total, color = C.muted, font_size = 12, no_wrap = true }),
            }),
            ui.frame({ visual = bar(done, total, peak, COLOR[p]) }),
        }))
    end
    return ui.col({
        gap = 10, pad = 14, radius = 10, fill = C.card, stroke = { 1, C.line },
        ui.text({ "By priority", color = C.text, font_size = 13, no_wrap = true }),
        ui.row({ gap = 28, cells }),
    })
end

return M
'''

# (caption, [(old, new), ...]) — what the agent says, then its exact replacements.
EDITS = {
    "chips": (
        "priority chips",
        [
            (
                "local function row(task)\n",
                '''local PRIO = {
    high = { "#f85149", "#3d1d20" },
    med = { "#d29922", "#3a2f14" },
    low = { "#3fb950", "#16301d" },
}

local function chip(p)
    local c = PRIO[p] or PRIO.low
    return ui.row({
        px = 10, h = 22, radius = 11, fill = c[2], center = true,
        ui.text({ p, color = c[1], font_size = 11, no_wrap = true }),
    })
end

local function row(task)
''',
            ),
            (
                '''font_size = 15, no_wrap = true }),
        on_click = function()''',
                '''font_size = 15, no_wrap = true }),
        ui.col({ grow = true }),
        chip(task.prio),
        on_click = function()''',
            ),
        ],
    ),
    "filters": (
        "filter chips with live counts",
        [
            (
                "return function()\n    local rows = {}\n    local done = 0\n    for _, task in ipairs(t.tasks) do\n"
                "        if task.done then done = done + 1 end\n        table.insert(rows, row(task))\n    end\n",
                '''local function filters(ux, total, done)
    local chips = {}
    for _, f in ipairs({ { "all", total }, { "open", total - done }, { "done", done } }) do
        local on = ux.filter == f[1]
        table.insert(chips, ui.button({
            id = "filter:" .. f[1],
            h = 30, px = 14, gap = 6, radius = 15, align_center = true,
            fill = on and C.accent or C.card, hover_fill = on and C.accent_hi or C.card_hi,
            ui.text({ f[1], color = on and "#ffffff" or C.muted, font_size = 13, no_wrap = true }),
            ui.text({ tostring(f[2]), color = on and "#cfe3ff" or C.muted, font_size = 13, no_wrap = true }),
            on_click = function() ux.filter = f[1] end,
        }))
    end
    return ui.row({ gap = 8, chips })
end

return function()
    local ux = ui.state("todo-ui", { filter = "all" })
    local rows = {}
    local done = 0
    for _, task in ipairs(t.tasks) do
        if task.done then done = done + 1 end
        if ux.filter == "all" or (ux.filter == "done") == task.done then
            table.insert(rows, row(task))
        end
    end
''',
            ),
            (
                "            ui.col({ gap = 6, rows }),",
                "            filters(ux, #t.tasks, done),\n            ui.col({ gap = 6, rows }),",
            ),
        ],
    ),
    "ring": (
        "progress ring drawn with gfx",
        [
            (
                "return function()\n    local ux =",
                '''local rings = {}
local function ring(steps) -- 0..48 segments of a 48-segment circle
    if not rings[steps] then
        local R, cx = 24, 28
        local function at(i)
            local a = -math.pi / 2 + i / 48 * 2 * math.pi
            return cx + R * math.cos(a), cx + R * math.sin(a)
        end
        local track, arc = {}, {}
        for i = 0, 48 do
            local x, y = at(i)
            table.insert(track, { i == 0 and "move" or "line", x, y })
        end
        for i = 0, math.max(1, steps) do
            local x, y = at(i)
            table.insert(arc, { i == 0 and "move" or "line", x, y })
        end
        rings[steps] = gfx.frame({
            width = 56, height = 56,
            gfx.stroke({ path = gfx.path(track), brush = gfx.solid(C.line), width = 7 }),
            gfx.stroke({ path = gfx.path(arc), brush = gfx.solid(C.accent), width = 7, cap = "round" }),
        })
    end
    return rings[steps]
end

return function()
    local ux =''',
            ),
            (
                '    return ui.col({\n        id = "todo",',
                '    local pct = #t.tasks > 0 and math.floor(done * 100 / #t.tasks + 0.5) or 0\n    return ui.col({\n        id = "todo",',
            ),
            (
                '''no_wrap = true }),
                }),
            }),''',
                '''no_wrap = true }),
                }),
                ui.col({ grow = true }),
                ui.text({ pct .. "%", id = "pct", color = C.text, font_size = 20, no_wrap = true }),
                ui.frame({ visual = ring(math.floor(pct * 48 / 100 + 0.5)) }),
            }),''',
            ),
        ],
    ),
    "stats": (
        "stats.lua: bars by priority, required by main.lua",
        [
            ('local t = doc:open("todo")\n', 'local stats = require("stats")\nlocal t = doc:open("todo")\n'),
            (
                "            ui.col({ gap = 6, rows }),",
                "            ui.col({ gap = 6, rows }),\n            stats.view(t.tasks),",
            ),
        ],
    ),
    "composer": (
        "add-task composer (Enter submits)",
        [
            (
                "return function()\n    local ux =",
                '''local function composer()
    local d = ui.state("todo-draft", { text = "" })
    local function add()
        local text = d.text:match("^%s*(.-)%s*$")
        if text == "" then return end
        t:insert({ "tasks" }, doc.map({ id = uuid(), text = text, done = false, prio = "med" }))
        d.text = ""
    end
    return ui.row({
        gap = 8,
        ui.input({
            id = "draft", value = d.text, grow = true, h = 38, px = 12, radius = 8,
            fill = C.card, stroke = { 1, C.line }, color = C.text, font_size = 15,
            on_input = function(v) d.text = v end, on_enter = add,
        }),
        ui.button({
            id = "add", h = 38, px = 18, radius = 8, center = true,
            fill = C.accent, hover_fill = C.accent_hi, press_scale = 0.97,
            ui.text({ "Add", color = "#ffffff", font_size = 14, no_wrap = true }),
            on_click = add,
        }),
    })
end

return function()
    local ux =''',
            ),
            (
                "            filters(ux, #t.tasks, done),\n",
                "            filters(ux, #t.tasks, done),\n            composer(),\n",
            ),
        ],
    ),
    "typo": (
        "restyle the rows (contains a typo)",
        [("w_full = true, h = 38, px = 14, gap = 12, radius = 10,", "w_full = true, h = 38, px = 14, gap = 12, raduis = 10,")],
    ),
    "fix": (
        "fix the typo the host reported",
        [("gap = 12, raduis = 10,", "gap = 12, radius = 10,")],
    ),
}


def check() -> str:
    """Replay every edit on V0 in plain Python: each `old` must occur exactly once."""
    src = V0
    for key, (_caption, pairs) in EDITS.items():
        for old, new in pairs:
            n = src.count(old)
            assert n == 1, f"edit {key!r}: anchor occurs {n} times: {old[:60]!r}"
        # edit_file resolves every pair against the same original, so apply them from that.
        spans = sorted((src.index(old), old, new) for old, new in pairs)
        for at, old, new in reversed(spans):
            src = src[:at] + new + src[at + len(old):]
    return src


_seed: Path | None = None


def seed_dir() -> Path:
    """A folder holding V0 as `main.lua`, for `upload_folder` (removed at exit)."""
    global _seed
    if _seed is None:
        _seed = Path(tempfile.mkdtemp(prefix="agent-builds-"))
        (_seed / "main.lua").write_text(V0)
        atexit.register(shutil.rmtree, _seed, ignore_errors=True)
    return _seed
