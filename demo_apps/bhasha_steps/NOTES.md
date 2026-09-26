# Bhasha Steps — build notes

## Drag-and-drop upgrade

### Implementation process

- Replaced the five shared sentence-build views with real `on_drag` sources and `on_drop`
  targets. Bank cards and placed cards have token-stable IDs; every current gap has an
  exercise-and-position slot ID. Gesture state remains local and progress storage is unchanged.
- A drag keeps its source dimmed and draws a root-level card ghost at the runtime-provided
  screen origin. Slots widen and brighten on `over`; placed cards can be reordered or dropped
  onto a conditional removal zone. The small-click path is retained only as an explicitly
  labelled fallback below the bank.
- A drop changes only the local attempt. Mastery and attempt counts still change only when the
  learner presses **Check sentence**, so arranging a correct sentence does not complete it.
- Wrong checks retain the constructed sentence, show the existing clue, and now explicitly ask
  the learner to move cards and retry. Navigation, exercise text, answers, and progress keys were
  preserved; only the five build prompts changed from tap wording to drag wording.

### Failures and friction

- The first complete five-class bridge validation exceeded its 120-second command allowance in
  the debug shell. It produced no app error; rerunning the same focused scenario with fewer drag
  interpolation steps and a 300-second allowance completed successfully.
- The drop targets must stay present even when no drag is active: otherwise the pointer has no
  real zone to enter. Keeping narrow dashed slots reserved also avoids sentence layout jumping;
  only the active slot widens.
- The removal target cannot be located before the gesture because it intentionally appears only
  for a placed-card drag. Validation therefore pressed a placed card, moved beyond drag slop,
  queried `Rects` for the newly reachable removal zone, then released over it.
- No Rust, shared documentation, shared tests, or status files were changed. Per instruction, no
  separate review agent was launched; the Lua-app checklist was applied directly before and
  after implementation.

### Focused bridge drag validation

- `python3 scripts/check_lua.py bhasha_steps` reports zero problems.
- An offscreen 1100×900 real shell run used bridge pointer `Drag` plus explicit
  move/press/move/release sequences, never direct click selection for card placement.
- During a held Class 1 drag, `DumpTree` contained both source and ghost text and `Rects` showed
  the hovered slot widened from 18 to 28 points. Release placed the card through `on_drop`.
- The run built and checked all five sentences through real drags. Class 1 deliberately failed
  once, showed retry feedback, reordered a placed card, removed another through the drop zone,
  restored it from the bank, then passed. The resulting progress had all five build exercises
  mastered, with two Class 1 attempts and one for each other class.
- The final console was clean. A 1100×900 screenshot with three placed cards was inspected from
  `/tmp`; no image was added to the app folder.

## Original implementation process

- Read the required architecture, status, Lua author guide, conventions, learning-suite design,
  six-app driver design, and Lua-app expert checklist before authoring.
- Inspected kanban's input/document patterns and the visual/layout patterns in pomodoro,
  dashboard, pie, and the existing education-suite palette stub.
- Authored fifteen deterministic exercises: one choose, sentence-build, and typed short-answer
  activity for each of Classes 1–5. Wording progresses from concrete word recognition to
  inference, evidence, contrast, and cohesion.
- Kept navigation, selections, token order, feedback, and typed drafts in module-local viewer
  state. The sole document contains only `mastered` and `attempts`, keyed by semantic exercise ID.
- Added responsive wrapping at the header, class selector, activity selector, activity body, and
  action rows. Visually inspected bridge captures at 1100×800, 680×900, and 520×900.

## Friction, failed attempts, and workarounds

- No undocumented UI or document API was needed. The strict documented surface was sufficient.
- The first all-exercise bridge script tried to click a Class 2 activity while Class 1 was still
  selected. The bridge correctly returned `no element with id 'open:c2-choose-plural'`: only the
  selected class's three activity buttons exist. The corrected script selects `class:N` before
  driving that class.
- The first privacy assertion searched serialized progress for answer words and found `drink`
  because the original semantic ID was `c1-type-drink`, not because a draft had been stored. The
  ID was changed to `c1-type-action`; a second run verified that no tested typed answer appears in
  document data.
- A 520px capture taken immediately after opening caught the intentional 180ms exercise-card
  fade near its first frame. This is deterministic offscreen-clock behaviour, not a rendering
  failure; later frames/captures show the card at full opacity. No workaround was added to app
  code because a live window naturally advances through the transition.
- The requested fresh `pi -p` expert review was attempted after implementation, but the process
  returned an account-level “out of extra usage” error before reviewing. I therefore reran the
  expert checklist directly: constructors only, stable IDs for handlers/scroll/animation, local
  attempts, one document write per check, guarded seed, strict documented props, and responsive
  bounds/wrapping.
- English is the only authored language. The footer states the honest future path: Hindi lessons
  may be added later, while typed-Hindi grading is not supported. No Hindi capability is implied.

## Bridge validation

Validated against the real offscreen shell and Lua VM using `Session(..., offscreen=(1100,800))`:

- Upload and staged reload produced a clean console.
- DumpTree contained the expected stable class, activity, option, token, input, and action IDs.
- A choice was clicked through `Rects` + the real pointer pipeline, not only direct handler calls.
- Retry then success paths passed for choose, sentence build, and typed answer.
- All fifteen exercises were completed; document data contained exactly fifteen progress entries
  and eighteen attempts (three deliberate retries), with every entry mastered.
- Document inspection showed only the `bhasha_steps_progress` document and only `mastered` /
  `attempts` fields per exercise. A deliberately typed `private learner phrase` never appeared.
- Locking, unlocking, and reopening preserved completed progress. The unfinished private draft did
  not return; checking the reopened empty input produced the local “One small step first” nudge
  without a document write.
- Final console remained clean. Screenshots were written only to `/tmp` for inspection; no image
  artifact was added to the app folder.

A final suite review found that the viewport-positioned drag ghost was still nested inside the root vertical scroller. The root is now fixed, the ordinary content has its own stable-ID growing scroller, and the ghost is its sibling at the app root, so scrolling cannot transform or clip the held word. The Lua type gate remained clean after the restructuring.
