# Personal Lua launcher

Status: bundled launcher, account persistence, revision-checked safe customization, Lua source
editor and Rust reset are built. Focused real-socket acceptance loops are
`scripts/check_launcher_customization.py` and `scripts/check_launcher_editor.py`.

The launcher is an account-owned source document, not a workspace item. The sealed vault entry
`shell/launcher/v1` stores its Loro snapshot (`files`: path → LoroText). `theme.lua` is the first
configuration surface. Workspace inventory is injected by Rust; credentials, vault access and
app instances are never injected. The bundle is the default when no override exists.

Edits use content revisions and unique, non-overlapping replacements against the original file.
A candidate source/VM is isolated from the working source/VM. Load, view and element-tree
validation must succeed before Rust writes the entry; activation follows that durable write.
World commands are blocked throughout transactional trials (including module load and state
sweep); trial walk geometry is isolated from retained worlds. This stronger gate is opt-in:
ordinary app open/reload behavior is unchanged.
Bad edits and persistence failures preserve the current source and UI. Startup failures in a
saved override show the bundled launcher with a warning, without deleting the saved record.
Rust-owned reset remains available even when the customized launcher fails.

Acceptance loop:

- exact edit changes title/palette in the real launcher;
- restart and unlock restore the change; another account retains the default;
- syntax errors, thrown views, invalid props and load/view commands are rejected;
- stale revisions and ambiguous/overlapping replacements change nothing;
- persistence failure preserves the running VM; successful edits carry `ui.state`;
- reset restores the bundle and survives restart;
- pointer drawing uses shell viewport coordinates and does not steal card/header clicks.

Revision 2026-10-03: the first pointer mount is launcher-local, not shell-wide. The runtime
positions absolute children relative to their containing parent. An offset-root test verifies
the reused cursor's local coordinates; native cursor visibility remains scoped to Rust chrome.
Shell-wide pointer ownership stays deferred until the Lua shell owns that viewport.

The bridge exposes launcher-specific senses/actions; ordinary app APIs do not gain shell access.
Source reads/edits operate on the selected workspace launcher while Home is focused. Reset is
account-wide and works from the workspace listing too (Rust's strip uses a confirmation click).
`SelectWorkspace` preserves app instances, and `ScreenshotShell` captures Home/chrome using
the existing deferred-frame pipeline without requiring a focused app item. No plaintext source files are written beside the account db.

## Lua editor — revision 2026-10-03

The bundled Customize panel edits `theme.lua`, `main.lua` and `cursor.lua` through launcher-only
source read/command bindings. It uses `ui.text_area` and temporary `ui.state`, not document writes
per keystroke. Save runs the existing transactional Rust edit path; rejection retains the draft,
while successful activation/reset advances a source generation and dismisses the panel. Source
reads describe committed source even during candidate validation. Cancel/file switches discard
unfinished edits; drafts do not survive launcher recreation, lock or restart.

Tab display expands to four spaces rather than exposing missing control-character glyphs.
Input is mapped back to raw source with unchanged prefixes/suffixes preserved; partial-tab edits
replace that tab's edited span with spaces. This is a plain text editor, not syntax highlighting
or a new code-editor runtime island.

Queued launcher messages carry an instance token as well as workspace ID. A fresh launcher
rejects old inputs and saves before dispatch, including when a shared workspace has the same ID.
Bridge `ClickLauncher`/`TypeLauncher` call current handlers only on Home; real pointer operations
remain the hit-eligibility check. `Keyboard` drives game-key events, so the focused editor test
uses input actions, not native keyboard/IME. Both 320/900pt layouts retain reachable Save/Cancel
controls after a rejected save. Full reviews are deferred to the final combined pass.

Revision 2026-10-04: successful edits still advance source generation; Reset now builds a fresh
bundled launcher with a new instance token and empty scratch, rather than exporting old-VM
state. This supersedes the reset-generation mechanism above: a poisoned state-export function
must never block Rust recovery. Validation precedes deletion of the saved override.

## Workspace header — revision 2026-10-04

The default title/Back/Add item header is Lua-described. `shell.back()` and
`shell.import_item()` queue plain-data workspace actions only from handlers; Rust revalidates
unlocked-account membership, current workspace and Home before executing existing navigation or
native folder-picker/import logic. Ordinary apps receive neither capability. Moving presentation
does not lend Lua filesystem access or app instances.

`launcher:workspace-header` is a reserved opt-in ID in the rendered description. When absent,
Rust retains its header, including for older overrides and failed views. Detection is a read-only,
allocation-free Rust-tree query, not handler invocation or VM inspection. Recovery/reset stays
Rust-owned regardless of this marker. Focused acceptance: `scripts/check_launcher_navigation.py`
(pointer Back, narrow title layout, legacy fallback and runtime-error recovery); native folder
picker acceptance remains manual.

## Thread integration — revision 2026-10-04

The `lua-shell` worktree integrates `dev`'s thread-owned apps while keeping the launcher on the
chrome thread. Launcher commands remain copied metadata and queued plain data; Rust opens or
focuses an app thread. Launcher senses remain Home-only, separate from threaded app requests.

The launcher configures the shared RAII budget instead of installing a second interrupt hook:
32 MiB heap, 20,000 interrupts, 100 ms load and 10 ms execution CPU limits. On Unix these use
thread CPU time; other platforms currently fall back to wall time. State export/import and
trial view/sweep are armed, independently of handler permission. No native-call preemption or
hard wall watchdog is implied. Source candidates also isolate the 3D host from the live solver.

`check_launcher_threads.py` proves split/Home routing, retained app instances during launcher
edits, and an activated launcher view looping after its successful trial: Rust navigation/import
fallback and pointer reset remain available. Full smokes and combined reviews remain deferred.

Lua-owned auth UI, tab layout/tiling and persisted session layouts are later work. The account-owned source
is shared across platforms; Android-specific input/lifecycle remains with the Android workstream.
