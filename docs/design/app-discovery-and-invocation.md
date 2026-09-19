# App discovery and invocation

> **Status — 2026-09-11: design baseline; partly built.** The bridge can list/read source,
> dump the live pre-layout UI tree, read app data/errors, invoke id-addressed handlers, and
> capture live frames. Resolved geometry/layout senses, pointer gesture synthesis, callable action
> references, explicit app commands, manifest metadata beyond its built display name, and Markdown
> folder import are proposals here, not built. MCP is absent.

## 1. Decision

Automation gets no privileged interpreter and apps get no trusted agent-instruction channel.
The general surfaces are app metadata, source/document observation, current UI actions, and
explicit app commands. Python, accessibility tooling, a command palette, and a future MCP
adapter should all be able to use the same surfaces.

The default remains **drive the UI, not the document**. A visible handler owns validation,
id generation, transient-state changes, and side effects. Commands are an optional public API
chosen by the app author, not a way to reflect over arbitrary Lua functions.

## 2. Trust boundary

Everything supplied by an app is untrusted application content: Lua, comments, labels, data,
manifest fields, and Markdown alike. A file named `SKILL.md` does not gain the authority of a
host-installed skill and must never be automatically promoted into system instructions.

The runtime sandbox governs what Lua can do. The automation client separately governs what an
agent may do with its own filesystem, network, secrets, and confirmation policy. RPC/MCP results
must retain app-content provenance; wording in an app cannot enlarge either capability set.

For that reason the conventional prose file should be `README.md` or `INTERACTION.md`, not a
special trusted `SKILL.md`. The folder importer currently excludes Markdown even though the
source document and file RPCs can store it; allowing `.md` is a future import-contract change.

## 3. How a client learns an app

A capable client can progressively inspect:

1. manifest metadata and the source-file list;
2. optional app documentation and relevant Lua modules;
3. `DumpTree`, the truth about controls rendered now;
4. resolved element/layout inspection or a screen-space hit stack when geometry matters;
5. `AppDataGet` and `ReadConsole`, the truth about live state and failures;
6. `ListCommands`, if the app deliberately exports commands;
7. the tree/geometry/data/console/frame again after acting.

The model or human interprets those facts. The bridge transports them; it does not claim to
understand source code.

## 4. Callable UI actions

`DumpTree` should describe every handler exposed by the current element, including elements
without runtime ids. Each action receives an opaque, short-lived locator suitable for an
`InvokeAction` RPC. Invocation rebuilds the current view, resolves the locator, verifies an
expected fingerprint such as kind/text/handler-name presence, and calls that exact UI handler. A
stale or ambiguous locator fails rather than calling a neighbour; closure identity is never reused.

The wire representation of the locator is intentionally undecided: a validated tree path is
preferable to publishing raw Lua handler indexes, which are rebuilt every view. It is not a
persistent app API. Stable element ids remain the better address for scripts, retained input,
scroll, and multi-phase pointer gestures.

This makes kanban's un-id'd `+ Column`, delete, Create, and Cancel controls callable while still
running their real closures. Opening `+ Column`, typing `col_name`, and invoking Enter remains a
sequence of UI actions; no direct board write substitutes for it.

## 5. Explicit application commands

An app may later publish named commands with a description, typed argument schema, result schema,
and effect metadata such as read-only/mutating/destructive. `ListCommands` exposes descriptors;
`CallCommand` validates arguments and invokes only a function the app explicitly registered.
The registration shape is open: a backward-compatible table returned by `main.lua` is one option.

Commands are generic application capabilities, not agent hooks. A command palette or Python client
can call them too. Apps should factor shared domain logic so a command and its UI handler enforce
the same invariants. Commands may intentionally offer semantic or bulk operations that are not a
simulation of pointer input, so clients must distinguish them from `InvokeAction`.

Arbitrary Lua evaluation, calling a local function by source name, and exposing the VM's function
table remain forbidden. A source name says nothing reliable about closure identity, upvalues,
argument shape, visibility, or safety.

## 6. What `lua_tree` contributes

`lua_tree` preserves comments as leading/trailing trivia on statements, entries, final statements,
and block tails; its round-trip suite pins that property. Local comments can therefore explain a
handler and survive structural edits. They are still untyped strings, not command declarations or
trusted instructions.

The tree is static source structure, not the live VM. It can show that function syntax exists but
cannot prove which closure is current or safe to call. Its `_nid` is source-edit identity, not a
runtime element id or authorization token. Runtime invocation must come from the current UI action
registry or an explicit command registry.

## 7. Resolved UI senses and gestures (designed 2026-09-11; unbuilt)

`DumpTree` is intentionally pre-layout: it describes the current `El` vocabulary and handlers but
has no window, Taffy result, camera, clip, or hit order. Do not overload it. Resolved inspection is
a Runner-owned sense, because Runner alone owns the actual viewport, layout, retained scroll/camera
state, Geometry, and final paint/hit ordering.

Like `Screenshot`, an inspection request is deferred until a freshly resolved frame and returns one
snapshot token. Coordinates name their spaces. A bounded `InspectElement`/`InspectSubtree` reports
specified and computed layout, content/screen/visible rectangles, effective clips and transform,
plus relevant scroll viewport/content/offset/range/thumb and camera scale/pan. `HitTest(screen)`
returns the ordered eligible stack and clip rejections. Stable ids are preferred; an un-id'd target
uses the same short-lived, fresh-view-validated locator policy as actions. Never publish Taffy node
ids as protocol identity, and do not pretend computed values explain Taffy's causal reasoning.

Queries carry include masks, depth/node limits, and explicit logical viewport dimensions when an
offscreen layout is requested. Logical geometry is DPI-independent; a custom-sized query must not
replace live hit state. Optional screenshot annotations may highlight returned ids, clips, or hit
rectangles, but pixels are evidence alongside—not a substitute for—the structured snapshot.

Gesture synthesis adds `PointerDown`/`PointerMove`/`PointerUp` and `Wheel`, with buttons and
modifiers, through the normal Runner path. This is required to test camera zoom/pan, scroll thumbs,
and drag/drop; id-addressed `Click` remains useful but deliberately bypasses pointer eligibility.
Requests remain bounded plain data executed by the UI thread, not arbitrary event-loop access.

## 8. Future MCP boundary

If MCP returns, it is a thin client of the live RPC bridge: describe/read the app, inspect the
pre-layout tree or resolved frame, invoke actions/gestures, list/call commands, read data/errors,
and capture a frame. It neither parses Lua for authority nor silently executes prose. Keeping
intelligence out of the adapter preserves the UI thread as the single execution authority and gives
direct RPC clients identical semantics.

## 9. Acceptance slices

1. Add fresh-view-validated action locators and invoke an un-id'd kanban button.
2. Add bounded, fresh-frame `InspectElement` and `HitTest`; diagnose an overflowing zoomed kanban
   column from layout, scroll, transform, clip, and ordered-hit data without reading a screenshot.
3. Add wheel and pointer-sequence synthesis; automate camera zoom/pan, thumb drag, and card drop.
4. Permit ordinary Markdown app files and extend the name-only `manifest.osv` with optional
   documentation metadata, with untrusted provenance explicit in the client contract.
5. Design the smallest explicit command registration against two real apps before changing the
   `main.lua` return contract.
6. Rebuild MCP only if a real consumer needs it; map it one-for-one onto tested RPC operations.
