# Viewport geometry rebuild

**Status — 2026-09-11:** implementation in progress. Paint and the hit resolver now interpret the
same explicit transform/clip boundaries; editor, drag/drop, nested wheel routing, and transformed scrollbars have been
migrated. Euclid-backed screen/viewport/content/node units now type the camera and Geometry
rectangles. Board-height containment now pins growing scrollers to the available main axis;
transformed overlay anchoring is built; overlay gesture occlusion, current-geometry capture, the
final event contract, and motion remain incomplete. This is not a product-ready foundation yet.

This plan revises the implementation direction in `view-and-interaction.md` §§2 and 5 and
`visual-substrate.md` §4.2. Those historical proposals remain intact: scale can use rectangular
screen hits, but coordinate-bearing events still need inverse mapping; the camera belongs to
an app viewport, not the whole window. No compatibility with the experimental internals or
`zoom_x` workaround is required. Existing app behavior remains an acceptance requirement.

## Scope and invariants

Keep Taffy, the immediate El description, and per-id retained state. Replace the geometry seam
between layout, paint, and interaction rather than patching individual consumers.

- Layout determines content rectangles. Camera zoom and press-scale do not cause reflow.
- Screen coordinates mean logical window coordinates; DPI is applied only at rendering/input
  boundaries, not folded into app geometry.
- Coordinate-bearing APIs distinguish screen, viewport, content, and node-local space. Content is
  a viewport-owned layout space; a future canvas may use it as persisted world space.
- Every node has a full content rectangle and an accumulated content-to-screen transform.
- Every clip boundary retains its own coordinate space. Resolve boundaries to screen space
  before intersecting them. Never intersect local rectangles from different spaces.
- Paint and the Geometry hit resolver consume the same placed transforms and explicit ancestor
  clip boundaries; their screen-space results must agree.
- Visible hit regions do not replace full element geometry used for text, drag, or drop math.
- Paint order equals reverse hit-test order. Overlays paint last and hit first.
- Scope is finite positive uniform scale and translation, with rectangular viewport/scroll
  clips. Arbitrary rotation, general path hit-testing, z sorting, and filters are out of scope.
- Pan, zoom, animation, focus, and capture are viewer state, never per-frame CRDT writes.

## Pipeline

`El tree → content layout → optional Frame visual payload → resolved Geometry → paint and interaction`

`Frame` in `visual-substrate.md` is proposed local visual data; Geometry is placement, inverse input
mapping, and clipping. The coordinate chain is `Frame/node local → content → viewport → screen`.
The current renderer still paints `Placed` nodes directly; typed Geometry is being established
without waiting for Frame, and Frame must later consume rather than duplicate this mapping.

Layout retains ancestry and explicit viewport/scroll boundaries. A viewport gets its available
size from its parent; its content lays out against a stable viewport-sized constraint, regardless
of camera scale. Overflow remains reachable. The content root preserves the container's layout
semantics rather than silently replacing every zoomable container with a synthetic column.

The geometry walk accumulates transforms and resolves the clip chain. A node's screen hit region
is its transformed rectangle intersected with its effective screen clip. Input additionally
retains the full local rectangle and inverse transform. Empty visible regions cannot receive hits.
Paint uses explicit, balanced subtree clip scopes, not transform-equality heuristics or fake nodes
filled with unused appearance/behavior fields.

Retained animation is advanced once per frame before resolving geometry. Paint and hits use the
same animation sample. Active motion requests redraw; settled motion returns to on-demand idle.

## Interaction policy

Keep `zoomable` as an opt-in app-content viewport requiring an id; shell/app toolbar stays outside.
Free drag-pan works in both directions. Content bounds may support future framing or scrollbar
policy, but bounded pan is not required to fix clipping and is not part of this repair.

- Ctrl+wheel targets the topmost eligible zoom viewport, subject to overlay occlusion.
- Ordinary wheel goes to the innermost eligible scroller; unconsumed delta can reach viewport pan.
  Shift+wheel supplies horizontal movement. Wheel deltas are converted to the consumer's units.
- Text selection, scrollbar thumbs, and app controls win over empty-area viewport pan. Resolve
  eligibility using shared ordering/ancestry, not independent searches that click through overlays.
- Capture stays tied to stable identity and a defined coordinate frame. Resolve current geometry
  by id during a gesture; cancel safely if its owner disappears. App events distinguish screen
  position for ghosts/overlays from content-space delta and node-local positions. Do not silently
  reinterpret one Lua x/y pair for both uses; settle the event contract in the input slice.
- Drop normalization uses the full target rectangle, never its clipped visible fragment.
- Overlay element anchors use the stable authored/camera transform before portal placement; they
  ignore transient press feedback. Point anchors are already screen-space. Overlays deliberately
  escape content clips. Scrollbar paint, hits, and thumb movement share resolved geometry; the
  first version follows its owning scroller's transform rather than inventing fixed-pixel chrome.

## Implementation slices and gates

Each numbered stage is broken into writes of at most roughly 100 changed code lines; tests ride
along. Explain each write before implementation, run the relevant expert checklist, and obtain
user judgment after a fresh diff review. Do not add motion before the geometry gates pass.

1. **Pin failures.** Add a minimal overflowing board fixture and geometry tests: an initially
   offscreen composer is panned into view; nested clips remain attached to their own boundaries;
   content outside the viewport cannot receive clicks. Record current failures before replacing code.
2. **Local layout and shared geometry.** Introduce explicit boundaries and one geometry resolver;
   preserve stable viewport layout, padding, row/column semantics, and overflow. Test nested pan,
   zoom, press-scale, and window resize. Replace mixed-space clip intersection.
3. **Paint and basic hits.** Migrate both consumers to the same frame geometry and clip scopes.
   Remove dummy clip nodes and inherited-clip heuristics. Test visible button centers, clipped
   buttons, overlapping controls, and overlays. Keep paint order and hit order identical.
4. **Coordinate-bearing input.** Migrate editor selection/caret, drag, resize, drop, context menus,
   scrollbars, and overlay anchors. Settle Lua drag screen/local fields and adapt kanban together;
   no compatibility shim required. Test partially clipped targets and capture-owner removal.
5. **Gesture arbitration.** Restore nested wheel scrolling and scroll chaining into free x/y pan.
   Test Ctrl+wheel, Shift+wheel, overlay occlusion, and controls taking precedence over pan.
6. **Motion.** Replace the button-specific clamped spring with a tested scalar spring supporting
   configurable parameters and arbitrary finite targets. Preserve velocity on retargeting, handle
   long frames safely, and settle to idle. Use spring output directly rather than easing it again.
   Then spring zoom while preserving the world point under its anchor on every animated frame.
   Keep drag following direct; add damped release inertia separately from target-seeking springs.
   App-object spring settling is a subsequent slice, not an implicit change to drag/drop commits.
7. **Cleanup and acceptance.** Remove abandoned APIs/state, correct current-status claims, and
   document the final Lua contract. Preserve unrelated working-tree docs and design changes.

## Acceptance

At normal, reduced, and enlarged zoom: add a fourth column, pan to it in both axes, type a todo,
click Add, edit/select text, resize the column, and drag/drop cards across columns. Confirm the
header never receives leaked paint or hidden hits, vertical card scrolling works, scrollbar thumbs
track their visuals, and a modal blocks underlying gestures. Repeat after resizing the window.

Automated tests must exercise resolved geometry/event routing, not only Lua view construction or
RPC Click-by-id (which bypasses pointer hit-testing). Add long-dt, rapid-retarget, finite-output,
anchor-preservation, and eventual-idle motion tests. The unbuilt Runner-owned inspection and
pointer/wheel bridge contract is specified in `app-discovery-and-invocation.md` §7; it follows the
geometry unit tests and then automates these live acceptance scenarios.

Final gate: `cargo check --workspace`, `cargo test --workspace`, matching expert reviews, and
user acceptance of the live scenarios. Report warnings and unverified behavior explicitly.
