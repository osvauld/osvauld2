# Interactive lessons and presenter scripts

> Status — 2026-11-05: implementation in progress; no platform API change; awaiting user review.

## 0. End-to-end tests

Write `scripts/smoke_crdt_lesson.py` first and observe the missing-app failure. Drive the
real shell, Lua VM, pointer hit geometry and captures in a throwaway vault. Prove:

1. Increment both disconnected replicas: each total becomes one. Delivering while disconnected
   leaves the queue and replica unchanged. A queued update is a snapshot, not a reference to
   its sender's later state.
2. Connect and deliver both ways: slot-wise maxima yield total two at both replicas. Deliver
   an identical or older snapshot again: totals do not increase or decrease.
3. Unequal contributions and different delivery orders converge when all queued state has
   arrived. The displayed totals are computed by the model, not scripted labels.
4. Six scenes share stable IDs with narration and demonstration cues. Presenter mode shows
   those notes; Record mode hides notes and controls and can be exited with Escape. Scene
   selection resets its own model reproducibly; Explore allows unscripted actions.
5. Playback executes the scene's declared cue actions once, pauses exactly, and inspection
   without virtual time advance does not advance it. Script export and recording read the
   same narration/cues as the app. A short silent MP4 has the expected frame count.
6. A second tiny lesson uses the same presenter module with a different model/view and no CRDT
   vocabulary. Validation names unknown cue actions and duplicate IDs. Failed reload keeps
   the working app. Viewer experiment state is absent from app data docs.
7. Capture and inspect the app at 1280x900 and 900x700; counters and scene controls remain
   reachable. Run the focused smoke and existing sync explainer smoke. Preserve the prior
   full-suite result; repeat broader checks only for changes that justify them.

## 1. Product boundary

The user approved a general interactive lesson plus a synchronized presenter script, using a
grow-only CRDT counter as the first subject. The earlier sync diagram remains a separate
example. A lesson has scenes with stable IDs, a title, a learning point, narration, an
operator cue, a duration and timed action references. Each scene's model is prepared from
known inputs; actions compute state and views display it. Explore, Presenter and Record use
that same scene/model. Record is a clean audience view; recorded MP4s are silent, with the
presenter script supplied separately. No audio synthesis, upload or publishing is implied.

The app's `lib/presenter.lua` is a versioned, bundled Lua module. Its public factory accepts
scene data, `prepare(scene_id)`, an action table and `view(model, context)` returning an
ordinary `ui.*` description. It knows no CRDT semantics and reads only its app's modules.
`context.act(id)` dispatches a declared action; cue actions use the same functions. No new
global module loader, host capability or dynamic evaluation is introduced. An agent can
replace the subject's model and visual while retaining navigation, notes and playback.

The presenter validates scene IDs, action references and cue times before the app runs.
Interaction and recording state live in `ui.state`, not shared CRDT docs. Monotonic elapsed
time drives cue delivery; observations at the same time do not repeat actions. Scene or mode
changes stop playback. SVG support can later replace a scene visual without changing the
presenter/model contract. The current native vector primitives are enough for this proof.

## 2. Counter and story contract

This teaches a state-based grow-only counter with fixed replica IDs `alice` and `bob`.
Each replica increments only its own component, value is the sum, merge is per-component
maximum, and messages carry copied snapshots. Disconnection prevents delivery. Explicitly
queuing and delivering snapshots makes delayed, stale and duplicate delivery inspectable.
The scalar-copy scene demonstrates a failing approach separately, without pretending it is
the CRDT. This is an algorithm simulation, not a Loro implementation or network service.

Reference: Shapiro et al., *A comprehensive study of Convergent and Commutative Replicated
Data Types* (2011), §3.1.2, https://www.lri.fr/~mbl/ENS/CSCW/2021/papers/CRDT-study11.pdf.
Text editing, deletion and decrement algorithms are outside this lesson.

## 3. Review units

1. Plan and failing real-shell smoke.
2. General presenter, computed counter model, six-scene storyboard and subject visuals.
3. Export/record driver, narrator script, focused verification and visual inspection.
4. Fresh `pi -p` review using expert-lua-app and expert-architect checklists; user judgment
   before committing or merging. Record actual verification in this plan and the status ledger.

Self-check: pure views, explicit `ui.*`, unique stable IDs, no per-frame doc writes, no
Rust/native changes or removed crate recreation. Components carry a subject-neutral boundary;
the storyboard and presenter script derive from one source rather than independently drifting.
