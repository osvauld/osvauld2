# Hidden window — the shell keeps answering while the compositor shows nothing

Status: **planned 2026-10-26**, built in the jetpack-arena worktree.

The shell stopped answering the bridge (even `Ping`) whenever its window was hidden while an app
animated: the main thread sat in `poll`, the event loop never woke for the bridge's message.
Cause: presents use `AutoVsync` (FIFO). On Wayland a FIFO present waits for the compositor's
frame callback, which a hidden window never gets, and that wait is on the event-loop thread. So
bridge requests, input and peer sync messages (all `EventLoopProxy` events) stall until the
window shows. Not focus — visibility.

## Checks

1. **Manual, real compositor** (`scripts/check_hidden_window.py SOCKET ITEM`): with an animating
   app open and the shell window hidden (another workspace or minimised), `Ping`, `ReadData`,
   `ReadConsole` and an app `Screenshot` each answer in under 2 s. Run against the user's shell;
   agents never launch it. Offscreen cannot reproduce this — there is no compositor.
2. Existing smokes stay green (offscreen frames are unchanged): `smoke_bridge.py`,
   `smoke_offscreen.py`, `smoke_jetpack.py`, `smoke_jetpack_editors.py`; `cargo test -p runtime`.

## Change

- Before each present, `window.pre_present_notify()`. On Wayland, winit 0.30 then holds the next
  `RedrawRequested` until the compositor's frame callback arrives
  (`wayland/event_loop/mod.rs`, `FrameCallbackState::Requested`). Hidden → no callback → no
  frame → no present to block on, and the loop stays free. Visible, callbacks pace frames as
  vsync did. Other platforms: a no-op.
- A windowed screenshot no longer waits for `RedrawRequested` (which a hidden window never
  gets): `deliver` takes it and runs the frame at once, capturing from the scene without
  presenting — the path a lost surface already used. A normal frame is still requested after.
- Not changed: present mode (FIFO stays), offscreen frames, driver ops (offscreen only).
