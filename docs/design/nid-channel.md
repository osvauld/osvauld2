# The nid channel: how a node id reaches a click

**Status:** proposal, 2026-09-06. Nothing here is built.
**Context:** `code-as-tree.md` §4 (the original claim), §13 (why it failed).
**Decides:** how provenance gets from the tree to `Placed` now that it cannot ride in the source.

---

## 0. The problem in one paragraph

§4 said an id could ride into the VM as an ordinary table field: print `_nid = "k3f9"` into every
table, `walk` carries it onto `El`, a click hands back the exact node. §13 found that this does not
run. The printer stamps every **table constructor**, and only some table constructors are elements
— `model.lua` seeds the board through `doc.list({ … })`, which rejects named keys on purpose, so
the app fails to load at module scope. The conclusion §13 drew:

> Stamping ids into source is a claim about every table in the file. Syntax cannot tell an element
> from a data table; only the runtime can.

This note is about the second half of that sentence. The runtime already does tell them apart, and
has since W2.

---

## 1. The discriminator already exists

`app_host/src/lib.rs:457`, the prelude:

```lua
local function tagger(tag)
    return function(t)
        t.tag = tag
        t.line = debug.info(2, "l")
        return t
    end
end

ui = { col = tagger("col"), row = tagger("row"), text = tagger("text"),
       button = tagger("button"), input = tagger("input"), text_area = tagger("text_area") }
```

Six constructors. Everything that becomes an element passes through one of them, and **nothing
else does**. `doc.list` is not a tagger. `doc.map` is not a tagger. `{ 1, C.line_soft }` as a
`stroke` value is not a tagger.

This is not a convention. `walk` (`lib.rs:610`) opens with `let tag: String = node.get("tag")?` —
a table with no `tag` is not an element and cannot become one. So the tagger is exactly the set of
tables that can reach `Placed`, which is exactly the set that can be clicked.

### 1.1 Why not detect `ui.*` syntactically instead

The parser could recognise `ui.text({ … })` as a call on `Index { base: Name("ui") }` and stamp
only those. For the kanban this would be correct. It is still the wrong mechanism:

```lua
local col = ui.col     -- legal Lua, produces elements, matches no syntactic pattern
```

`ui` is an ordinary global and can be aliased, passed, or wrapped. **The tagger cannot be avoided**
— it is on the only path to `tag`. A syntactic rule is a heuristic that is right until an app is
written slightly differently; the tagger is right by construction.

### 1.2 The three kinds of table

Making the distinction explicit, because §13 showed it is not obvious:

| kind | example | tagged | reaches `Placed` | wants a nid |
|---|---|---|---|---|
| **element** | `ui.col({ … })` | yes | yes | **yes** |
| **splice group** | `local body = {}` … `ui.col({ …, body })` | no | no — flattened | no |
| **data** | `doc.list({ … })`, `stroke = { 1, C.line_soft }` | no | no | **must not** |

The middle row is real and in use: `main.lua:115` builds `body` imperatively and `main.lua:162`
nests it as a positional child. `children` (`lib.rs:548–566`) sees an untagged table and splices
its contents into the parent rather than treating it as an element. It never appears in the layout,
so it never needs to be addressed — its children are tagged and they are what a click lands on.

---

## 2. The channel

The printer knows which line it wrote each table on. The tagger knows which line it was called
from. That is the join.

```
tree ──print_bare──▶ text (no ids)
        │                  │
        └──────▶ [(chunk, line) → nid]        vm.load
                           │                     │
                           └──── install ───▶ tagger stamps t._nid ──▶ walk ──▶ El ──▶ Placed
```

Concretely, three changes:

**Printer.** `print_bare` returns the text *and* a `Vec<(usize, Nid)>` — the line each table's
constructor was emitted on. The host turns that into a per-chunk Lua table.

**Prelude.** The tagger gains one lookup:

```lua
local function tagger(tag)
    return function(t)
        t.tag = tag
        local s, l = debug.info(2, "sl")
        t.line = l
        t._nid = _nids[s] and _nids[s][l]
        return t
    end
end
```

**Props.** `_nid` joins `line` in `STRUCTURAL` (`props.rs:229`), so `props::apply` stops rejecting
it — and this time that is *correct*, because the field now only ever appears on elements.

### 2.1 What `debug.info` actually returns

Verified in this repo, not assumed:

```
debug.info(1, "sl")  ->  [string "u.lua"] @ 2
```

The source comes back **wrapped**, as `[string "<chunkname>"]`, not as the bare path. Every chunk
is named — `main.lua` at `lib.rs:158`, required modules at `modules.rs:58` via `set_name(&key)` —
so the key is well-defined, but the host must build the map with the wrapper form or normalise it
once at install time. Getting this wrong fails silently: every lookup misses and every element
gets a nil nid.

Both values come from one `debug.info` call, and the chunk name is a constant per chunk, so the
lookup is two table indexes and no allocation.

---

## 3. What the printer has to change

**Rule: a call whose arguments contain a table constructor starts its own line.**

Without it the map is not a function. `main.lua:55` today:

```lua
ui.col({ grow = true, ui.text({ c.text, color = C.text, font_size = 13 }) })
```

Two tagged tables, one line, one map slot. `multiline()` (`print.rs`) does not catch this because
it only inspects an entry's value one level deep — the entry here is `Expr::Call`, and the table is
*inside* the call, so the outer table prints inline.

The rule is deliberately phrased without mentioning `ui`, for the reason in §1.1: the printer stays
a Lua printer and does not learn the DSL. It is slightly stronger than needed — `doc.list({ … })`
would also get its own line — which costs some vertical space and nothing else.

§6 already gives the printer total authority over formatting, so this is a rule change, not a
design change.

---

## 4. The map is not durable

It is regenerated with the text and is only valid for that text. Insert one statement and every
line below it moves.

This is the failure mode to design against: **text and map must travel together.** A skew does not
error — it silently labels every element below the edit with its neighbour's identity, which is
worse than having no id at all. So they are one artifact with two halves, produced by one call and
stored or passed as a pair, never regenerated independently.

This is tolerable only because the text is *generated*. Under §12 level 3 the tree is the stored
thing and the text is a rendering; producing both from one `print` is the normal path, not a
special case. If the text were ever the durable artifact — someone edits `main.lua` in an external
editor — the map is stale and the correct response is to reparse and reissue, which §3 already
calls the reimport path.

---

## 5. What this does not give you

**Instance identity.** Verified against the real app:

| construct | source | elements on screen |
|---|---|---|
| `card_of` | `main.lua:47`, one `ui.row` | every card on the board |
| `W.guide` | `ui/widgets.lua:14`, one `ui.col` | every gap in every column |

A click on any card resolves to `main.lua:47`. That is the right answer to *which code drew this*
and no answer at all to *which card*. This is §11's two address spaces, unchanged: the nid says
which construct, the data path says which instance. `walk` already builds a path under `dev`
(`lib.rs:614–626`), which is the raw material for the second half.

**Bare string children.** `ui.col{ "hello" }` becomes a text element at `lib.rs:545` with no table
behind it. It has no nid and cannot have one; the nearest addressable node is its parent.

**Hand-rolled elements.** An app that writes `{ tag = "col", … }` directly bypasses the tagger and
gets no nid. `walk` accepts it and `tag` is already in `STRUCTURAL`, so this is legal today. It is
unlikely and worth knowing about rather than defending against.

---

## 6. Costs

| | per | what |
|---|---|---|
| tagger | element, frame | two table indexes; `debug.info` already runs |
| `walk` | element, frame | one `get::<String>("_nid")` — a boundary crossing |
| printer | print | one extra output; slightly taller files |

The second row is the one to watch. `lib.rs:611` already carries a warning that `walk` is ~80% of a
frame's Lua cost and that a `get` plus a `format!` per element is why the dev breadcrumb is
gated. The `cost_curve` test measures exactly this shape, and the earlier estimate — one extra
string prop is +50–130 ns/el against the breadcrumb's +490–820 — bounds it.

If it does not fit, the id does not have to be a string. It has to be unique and cheap to compare;
an interned `u32` handed out at map-install time would keep the boundary crossing but drop the
allocation.

---

## 7. Alternative considered: pass the id as an argument

```lua
ui.col({ … }, "k3f9")     -- tagger takes (t, nid)
```

No line map, no formatting rule, no dependence on `debug.info`. Strictly simpler where it applies.

It fails on the same rock as §13: the printer has to decide *which* calls get the extra argument,
and it cannot. Adding one to every call that takes a table means `doc.list({ … }, "k7a2")` — an
extra argument silently handed to every function in the program, ignored by most and meaningful to
some. That is a worse version of the bug this note exists to fix.

Worth revisiting only if the tree ever records "this node is an element" as data, at which point the
printer would know without guessing.

---

## 8. Open before building

1. **Cost.** Measure `_nid` in `walk` against `cost_curve` before committing to a string.
2. **Where the map lives.** It is per-VM and rebuilt by `reload()` along with everything else
   (`lib.rs:193`), which is free. But `reload` stages a whole second app — the map has to be built
   inside `build`, not beside it, or a failed reload leaves a map pointing at the wrong text.
3. **The `[string "…"]` wrapper.** Decide once, at install, and pin it with a test — a silent
   whole-app miss is the failure mode.
4. **Multi-file.** Every chunk is named, so the key works. Untested end to end.
5. **`print_bare` returning a pair** changes its signature; `round_trip` is its only caller today.
