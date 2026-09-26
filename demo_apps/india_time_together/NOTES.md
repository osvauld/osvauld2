# India: Time & Together — build notes

## Implementation process

- Kept the five levels, 15 authored activities, navigation, source/civics interactions, neutral wording, and existing progress document.
- Replaced the five tap-to-build ordering activities with one reusable evidence/timeline board. Every evidence card has a stable `timeline-card:<exercise>:<step>` drag ID; every numbered destination has a stable `timeline-slot:<exercise>:<number>` drop ID.
- Drag state is module-local viewer state. The source card dims while held, a bordered root-level ghost follows the pointer outside the scrolling content, and the active drop zone changes colour. Releasing over a slot places the card immediately; a placed card can be dragged again, and moving onto an occupied slot returns or swaps the displaced card rather than accepting a tap shortcut.
- A full but incorrect timeline shows the existing careful clue and writes nothing. A correct full timeline performs one `progress:set`, preserving the stable exercise key and the existing `mastered`/`attempts` shape. Further rearrangement of an already mastered timeline does not write another completion.
- Updated only the five ordering prompts to say that cards are dragged onto a timeline. The historical/civic claims and the disclaimer that sources are authored practice scenarios remain unchanged.

## Failures and friction

- The first type-check command passed the folder path to `scripts/check_lua.py`; that tool accepts app names, so it reported “no such app.” Re-running it as `python3 scripts/check_lua.py india_time_together` was clean.
- An initial all-five bridge check assumed every placed card would remain in `Rects`. On the longer Class 5 board, placement changed wrapping and moved the timeline card below the visible scroll clip, so that assertion failed even though the release hit the slot and the card moved. `DumpTree` and a screenshot exposed the layout shift. Removing full-width child claims and keeping the evidence/timeline lanes side by side made source cards and destinations usable together at narrow widths; the final check still combines real release hits with `DumpTree` identity rather than mistaking clipping for deletion.
- No drag API workaround was used: validation exercised the runtime pointer path and real `on_drag`/`on_drop`, not handler-by-ID clicks. The board can become vertically long for Classes 4–5, but it remains inside the existing scroll surface and lets wrapped card text grow rather than clipping it.

## Focused validation

- `luac -p` parsed all three Lua files, and the per-app Lua type gate reported 0 problems.
- In an offscreen 1000×900 shell, used bridge `Rects` for coordinates and `Drag` for releases in all five class ordering activities. Every release hit its numbered `timeline-slot` through the normal hit-test, every placed card remained in `DumpTree`, the app console stayed clean, and partial boards left the progress document empty. Repeated the longest Class 5 card-to-slot drag at 480, 320, and 280 px widths in tall viewports; both lanes remained reachable and the console stayed clean.
- For Class 1, completed a wrong three-card timeline first and confirmed the progress document was unchanged. Reset it, dragged `ask → listen → draw`, and confirmed exactly `{ mastered = true, attempts = 1 }`. Rearranged and restored the mastered board; the document remained byte-for-byte equivalent at the JSON surface, demonstrating no second completion write.
- Drove a press and pointer move separately in an offscreen 900×760 shell, captured the gesture before release, and visually confirmed both the dimmed origin and the bordered root ghost. No temporary capture was kept in the app folder.
