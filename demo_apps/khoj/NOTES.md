# Khoj build notes

## Build log

- Read the repository architecture/status, Lua authoring contract, conventions, both requested design notes, and the Lua-app expert checklist before writing app code.
- Inspected the kanban reference app plus the pie, dashboard, Pomodoro, Frame-orbits, and in-progress learning-suite palette examples. The pie demo confirmed the documented Frame shape-hit pattern is suitable for the diagram activity.
- Planned a five-class, three-mechanic structure with exactly 15 stable exercise IDs. Learner selections remain module-local; the document stores only `mastered` and `attempts` progress.

## Friction and workarounds

- `docs/lua-apps.md` documents Frame shape hits in detail, but a later “Not exposed to Lua yet” sentence still lists hit-testing individual Frame shapes. The shipped `pie` demo and the offscreen run both confirmed that named `gfx.group` hits do work, so Khoj uses that live behaviour for its five diagram exercises.
- An immediate narrow screenshot taken just after changing activities caught the exercise card partway through its 120 ms `fade_in`, making it look washed out. This was virtual-clock timing rather than a paint or layout error; interaction tests and settled screenshots drive subsequent frames before visual judgement.
- The first fresh-review command selected an unavailable Claude allocation and failed before reading the app. Retrying explicitly with the configured `openai-codex/gpt-5.6-sol` provider worked. That review found no blockers and two SHOULDs: centered overflow could hide a diagram's left edge on very narrow screens, and the water-cycle “round trip” stopped at rain. The diagram now centers inside a start-aligned 520 pt scroll child, and collection completes the water-cycle sequence.

## Validation

- Opened the uploaded folder through a real `shell2 --offscreen 1000x760` bridge session. All five source files loaded with a clean Lua console.
- Captured and inspected 1000×760 classification and diagram screens; controls, typography, wrapping, diagram art, feedback, and completion states rendered as intended.
- Exercised a retry and success path for all three mechanics in Class 1. Durable data contained only each exercise's `attempts` and `mastered` fields.
- Visited every Class 1–5/mechanic combination through stable IDs (15 exercises total) with a clean console.
- Clicked a Class 5 diagram through `Rects` plus real pointer input, confirming the Frame hit path rather than only direct handler dispatch.
- Reloaded the app after an unfinished diagram choice: the temporary choice disappeared while three mastered exercises remained.
- Captured settled 640×720 and 480×720 layouts. Class/activity controls wrap, while the fixed-width diagram starts at a reachable left edge and scrolls horizontally on the narrower view.
- `python3 scripts/check_lua.py khoj` reports zero language-server problems. (An initial invocation passed the folder path instead of the app name and only printed the script's usage error.)
- A final fresh expert review after those fixes reported no BLOCKER or SHOULD findings; it also exercised all 15 success paths in the real offscreen shell with a clean console.

## Live circuit lab upgrade

### Implementation process

- Re-read the repository instructions, Lua app contract, conventions, and Lua-app expert checklist, then inspected every Khoj source file and these notes before editing.
- Kept the five-class navigation, 15 exercise IDs, factual explanation, and existing mastery lookup intact. Class 4's diagram activity is now the signature lab: the learner drags a named switch contact, sees the physical gap and percentage change continuously, and sees the bulb turn on only when the conducting path is actually closed.
- The lab uses module-local gesture state and rebuilds its Frame visual from that state. `circuit-lab:c4-circuit` and the Frame shape `switch-handle` are stable IDs; no drag sample enters the document.
- Tightened progress writes across all activities: an incomplete or incorrect check writes nothing, while a successful check stores only `{ mastered = true }`. Older stores may still contain the initial version's `attempts` field until that exercise is mastered again; the app ignores it.

### Failures and friction

- The first endpoint-only pointer check proved dragging but did not prove intermediate response. It was replaced with a two-stage real-pointer test that asserts the visible `Contact gap: 50%` state before closing the switch.
- An edit-tool call for the circuit's lit-state cleanup had a malformed `newText` key and was rejected without changing a file; the corrected edit then applied normally.
- Bridge `Rects` exposes the interactive Frame bounds, not a separate rectangle for an internal named Frame shape. Validation therefore derived the handle point from the reported Frame origin plus the visual's authored local coordinates, then confirmed the pointer was over the lab before dragging. No screen coordinate was guessed.
- A continuously fading bulb would misrepresent an ordinary switch. The final model keeps the contact motion and measured gap continuous, but changes current and the bulb at contact, and states that distinction in the learner-facing explanation.

### Focused validation and final review

- `python3 scripts/check_lua.py khoj` reports zero problems.
- In a fresh `shell2 --offscreen 1000x760` bridge session, uploaded Khoj, navigated through stable IDs to Class 4's diagram activity, and used runtime pointer drag synthesis (not direct handler dispatch).
- Dragged the handle to halfway in eight pointer steps: the tree reported `Contact gap: 50%`; checking there produced feedback but the progress document remained empty. Dragged from halfway to contact: the tree reported `CLOSED • current flows`, while progress still remained empty until `check:c4-circuit` was invoked. The resulting durable entry was exactly `{ mastered = true }` and the Lua console stayed clean.
- Captured and inspected `/tmp/khoj-circuit-final.png`; the circuit, closed contact, lit bulb, live status, explanation, feedback, and existing navigation fit at 1000×760.
- Per the requested process, no additional review agent was launched. Final self-review against the Lua-app checklist found no blocker: elements use constructors, gesture state is local, the drag has a stable ID, no per-move document writes occur, and the scroll container is identified.
