# Bharat Compass build notes

## Upgrade scope and process

- Changed only `demo_apps/bharat_compass/`; unrelated working-tree changes were left untouched.
- Read the repository instructions, Lua author guide, conventions, and Lua-app expert checklist before editing, then reviewed every app file and these notes.
- Preserved the five-class navigation, all 15 exercise IDs, the choose/match content, and mastery lookup. The five former sequence activities now form one signature `Navigate` mechanic while retaining their school, library, river, water-study, and cyclone-readiness themes.
- Kept every map fictional/local and terrain-based. No national, state, or disputed political boundary is drawn.

## Navigation mechanic

- Each class has a visibly different grid: dimensions, start/goal, landmarks, blocked terrain, and route constraints progress from a direct neighbourhood trip to waypoint, move-budget, and hazard-aware routes.
- `gfx.frame` draws the terrain cells, blocked areas, landmarks, destination, travelled line, and current traveller. The rendered visual is cached by exercise and trail so ordinary view rebuilds do not repeatedly compile an unchanged Frame.
- North/east/south/west controls move one square at a time. Blocked and out-of-bounds moves explain the failure without changing the trail. Undo removes one move; Reset returns to the start.
- Route state, feedback, selections, and current navigation remain viewer-local. A failed check performs no document write. A first successful check writes only `{ mastered = true }`; existing mastery remains compatible and activity IDs did not change.

## Failures and friction

- The first type-check command used the folder path, but `scripts/check_lua.py` expects an app name; it reported `no such app`. Rerunning as `python3 scripts/check_lua.py bharat_compass` completed with 0 problems.
- Frame has no text, so landmark meanings and the traveller/destination key are ordinary UI text beside the visual rather than labels embedded in the drawing.
- The bridge uploader accepts only Lua/manifest files, so `NOTES.md` is intentionally absent from the uploaded seven-file app bundle.
- No separate review agent was launched. Final review was a direct pass against the loaded Lua-app checklist: local interaction state, stable handler IDs, strict props, no per-move document writes, and one success-only mastery write.

## Focused bridge validation

Validated in a fresh real shell session with `Session(offscreen=(1100, 900))`:

- Uploaded and opened `compass.lua`, `content.lua`, `main.lua`, `manifest.osv`, `model.lua`, `terrain.lua`, and `theme.lua`; the app console stayed empty.
- Opened the Class 1 route, located its east control with `rects()`, and activated it through `click_at()` to cover real layout/hit testing. Then exercised Undo and Reset.
- Checked both an unfinished route and a moved-but-wrong route; `bharat_compass_progress.exercises` remained empty.
- Completed all five routes through N/E/S/W controls, including the Class 3/4/5 waypoint routes and the Class 4 move budget. Each success produced exactly one `{ mastered = true }` entry under the preserved exercise ID.
- Confirmed the final durable document contained only the five mastery entries—no route, attempt count, feedback, or selection data.
- Captured and visually inspected `/tmp/bharat-compass-route.png`: the Class 5 terrain, hazards, landmark dots, route trail, traveller, destination, directional pad, Undo/Reset, and completion feedback were all visible at 1100×900. The final bridge console remained empty.

A final suite review caught two gaps. Class 1 promised both the neem tree and bridge but accepted any route to school; route completion now supports multiple required checkpoints and Class 1 names both. Its direct tree-to-goal shortcut was rejected in a fresh bridge run, while the authored route mastered cleanly. Route maps also now sit in stable-ID horizontal scrollers, and the surrounding narrow-width floors were removed so fixed intrinsic Frame dimensions stay reachable instead of clipping the viewport.
