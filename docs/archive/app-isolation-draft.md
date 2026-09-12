# App isolation — rough draft

Status: rough. Fold into `runtime-rebuild-plan.md` once shaped.

## Why this is now a hard requirement
Product = OS-shaped host for **many untrusted, agent-authored apps**, apps can have **many concurrent instances**, **no network egress** by design.

The runtime's *only* advantage over a Tauri/webview host is that per-app isolation is **buildable here** (we own the loop) vs. not portably available there (see `memory/tauri-webview-isolation-limits.md`). Today the runtime is single-threaded `ControlFlow::Wait` — one app's long Lua handler blocks every other app, same failure as same-origin iframes. **Unbuilt = no advantage over Tauri.** So this must land.

## Requirements
- **Per-instance Luau VM.** VM is KB-scale, so many instances is cheap (the property that kills iframe/process-per-instance). Instance ≠ app: each open instance owns its VM + state.
- **Instruction-count interrupts.** Preempt a runaway loop mid-execution via Luau's interrupt callback. Browser/iframe gives *no* equivalent — this is the core thing we get that they can't.
- **Per-VM memory cap.** Bound each instance's allocation; a ballooning app can't starve the node.
- **Error containment at the app boundary.** A Lua error is a caught `Result`; one app crashing doesn't take the host or siblings down.
- **Non-blocking execution.** One app's long task must not freeze others. Options TBD (below).
- **No `fetch`/network capability injected.** Apps reach only `doc:*` / `ui:*` / `data:*` + node. Shrinks threat model from "app vs. the internet" to "app vs. other apps / node data" — the tractable one.

## Open questions
- **Threading model:** OS thread per instance / pooled / cooperative single-thread with interrupt-driven yielding? Isolation vs. latency vs. complexity.
- **Shared vs. per-instance VM** for multiple instances of the same app — share code, separate state?
- **Memory-cap mechanism** in mlua/Luau — what's actually enforceable.
- **Placement:** M0 plumbing or M1 Lua layer? Interrupts + threading feel M0; capability surface feels M1.
