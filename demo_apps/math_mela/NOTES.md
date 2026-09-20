# Math Mela — build notes

Started 2026-09-20. This log records the app-authoring process as it happens; it is not a claim that the shared documentation changed.

## Preparation and plan

Read the required architecture, status, Lua author guide, conventions, primary-learning-suite and six-apps design notes, plus the expert Lua-app checklist. Inspected the current kanban drag/state patterns, Linkage manipulation demo, Pomodoro, Dashboard, Pie, the bridge client/session, and existing manifests/themes.

Planned one authored activity of each required mechanic for every Class 1–5 (15 stable exercise IDs total): choose, numeric solve, and tap-to-order. Durable state will be only a `mastered` progress map; class, activity, choice, typed draft, feedback, and in-progress order stay in module-local or `ui.state` viewer state. Numeric drafts are never written to the document.

The ordering interaction will use large tap targets in two live zones: tap a loose tile to append it to the answer tray, and tap an answer tile to return it. This follows the suite's specified tap-to-order mechanic and remains straightforward to validate through bridge IDs.

## Friction and failed attempts

None yet. The current guide explicitly documents the constructors, strict props, named handler event tables, state lifetimes, and offscreen bridge flow needed for this app.

## Validation

Not run yet.
