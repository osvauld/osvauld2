# Primary learning suite

**Status (2026-09-20): approved for implementation; no app has landed yet.**

A local-first educational demo suite for Indian primary Classes 1–5 (roughly ages 6–11).
It is curriculum-informed, not represented as official NCERT, CBSE, or state-board material.
This file is also the coordination contract for implementation and review agents.

## Agent protocol

Every agent working on this suite must read this file, `docs/lua-apps.md`,
`docs/CONVENTIONS.md`, and `.agents/skills/expert-lua-app/SKILL.md` first.

- Each implementation agent owns exactly one app folder under `demo_apps/`.
- Agents must not edit another app, shared Rust code, or project documentation.
- No shared educational framework is introduced during this first suite.
- Each write is at most approximately 100 changed code lines; work proceeds in slices.
- Apps use only documented `ui.*`, `gfx.*`, and document APIs.
- Durable state contains progress only—never names, typed answers, or learner profiles.
- Content is bundled, deterministic, offline, and uses stable semantic exercise IDs.
- Every handler and scroll container has a stable unique ID.
- Exercise attempts stay viewer-local; document writes happen on checks/completion only.
- Implementation agents run the headless opener on their own app before handing off.
- Fresh review agents inspect each completed folder using the expert Lua checklist.
- The coordinating agent owns final integration, screenshots, workspace tests, and status.

## Assigned apps

| Folder | App | Subject | Three mechanics |
|---|---|---|---|
| `math_mela` | Math Mela | Mathematics | choose, numeric solve, tap-to-order |
| `khoj` | Khoj | Science / EVS | classify, sequence, quiz |
| `india_time_together` | India: Time & Together | History / civics | source choice, order, multi-select |
| `bharat_compass` | Bharat Compass | Geography / EVS | choose, match, sequence |
| `bhasha_steps` | Bhasha Steps | English | choose, sentence build, typed answer |

Each app supplies three exercises per class: fifteen authored exercises total. A visible
Class 1–5 selector changes level without deleting completed progress. Exercise wording grows
from concrete recognition in Class 1 to explanation and application in Class 5.

## Shared product rules

- Friendly feedback; no punishment, leaderboard, countdown, or public comparison.
- Large controls, short prompts, visible class and activity selection.
- A correct answer explains why. An incorrect answer gives a clue and allows another try.
- Completed activities remain replayable, but completion is never lost.
- English is the initial instructional language. Small Hindi display text is allowed only
  after screenshot verification; typed Hindi grading is out of scope.
- Geography avoids disputed boundaries and detailed political maps.
- History emphasizes evidence and viewpoints, avoiding disputed narratives and party politics.
- Civics uses broad constitutional values without presenting legal conclusions.
- Science and mathematics use familiar Indian contexts without claiming syllabus certification.

## Local progress shape

Each app owns one named document with a guarded module-scope seed. Progress is a map keyed by
stable exercise ID. An entry may record `mastered`, `attempts`, or `best`, as appropriate.
Current class, current exercise, selections, drafts, temporary order, hints, and mistakes are
viewer state. Content remains source data and is never copied into the document.

## Delivery gates

1. All five folders open with a clean console.
2. Every class and mechanic is reachable through stable IDs.
3. At least one success and one retry path per mechanic are exercised headlessly where practical.
4. Reopening preserves progress but not unfinished answers.
5. Fresh expert reviews report no blockers.
6. `cargo test --workspace` passes.
7. The coordinating agent updates this status line and `docs/status.md` only after verification.
