# Interactive sync explainer

> Status — 2026-11-05: implementation in progress; awaiting user review.

## 0. End-to-end proof

`scripts/smoke_sync_explainer.py` uploads the app through the real shell bridge into a
throwaway vault and drives actual pointer hits at rectangles reported by the runtime.
Write it before the app and observe its missing-app failure. It must prove:

1. The initial success diagram renders with six selectable components and a stage inspector.
   Clicking a component changes the inspector without moving the walkthrough.
2. Next/Previous/Restart traverse the success path and clamp at its ends. At completion the
   receiver has applied the update. A sampled connector pixel changes when its stage is active.
3. Offline reaches a local-save/waiting stage and stops playback there. Reconnect continues
   through permission checking, merge and delivery. It never implies that being offline loses
   the local edit or skips authorization.
4. Permission denied reaches refusal and writer rollback, without a node merge or peer update.
   The example assumes read access is retained and write access is denied.
5. Playback advances with driven virtual time, Pause holds, and inspection/capture with no
   time advance does not move the walkthrough. Scenario switches reset playback and selection.
6. Revision-checked source reload keeps viewer selection; lock/unlock can reopen the persisted
   source and starts a fresh viewer. The app creates no state documents for viewing progress.
7. At 900 and 1280 logical points wide the scenario controls and diagram remain reachable;
   screenshots are inspected at both widths and each scenario ends with an empty app console.

## 1. Scope and contract

An ordinary Lua app in `demo_apps/sync_explainer`, authorized in the conversation as the first
interactive-understanding artifact. No runtime feature or agent service is needed. Static
story data, palette, diagram composition and controls live in separate modules. Selection,
playback and progress are viewer scratch; source is the durable artifact.

The UI clearly calls itself an illustrative walkthrough, not live telemetry or an actual
network exercise. It assumes an already-published item and an authorized, subscribed receiver.
Today's desktop/node transport is a local Unix socket; remote transport is not built. The
trusted node reads and merges document content; sealed local storage is not a claim that the
node sees only ciphertext.

The story follows `shell2/src/node.rs`, `courier/src/sync.rs`, `courier/src/access.rs`, and
`kunki/src/push.rs`: local save, signed request and scoped permissions, CRDT merge, ack/version
vector and authorized subscriber push. A lost push is recovered by the receiver's later Sync.
The offline path illustrates a disconnected sender reconnecting; the denied path is specifically
a refused document write with retained read access and rollback, not every authentication error.

## 2. Implementation and review

1. Plan and failing real-shell smoke.
2. Build the Lua artifact with clickable components, path highlighting, moving packets,
   stage controls, a component inspector and three scenarios. Keep source references in the
   inspector so explanatory claims can be checked.
3. Run the focused smoke at both sizes, inspect screenshots, register it in `SMOKES`, and
   run the standard smokes and the app-host suite through LoadTable. Resolve any failures
   attributable to this change; report unrelated failures precisely.
4. Obtain a fresh `pi -p` review using the Lua-app and architect checklists; prepare the
   result for the user's judgment. Do not commit or merge without their nod.

The checklist self-check is that all elements use `ui.*`, all interactive IDs are unique,
view builds descriptions only, playback uses the monotonic frame clock, and no transient
viewing state enters Loro. Diagram composition stays Lua-side; no removed reference crate
is recreated. A reusable general diagram library is deferred until this example establishes
which primitives it needs.
