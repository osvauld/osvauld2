# Game branch integration

## End-to-end acceptance

- Run `python3 scripts/smoke.py` through LoadTable on the merged shell. Keep the existing
  host, multiplayer, Jetpack, Hillclimb, SVG, sound, binary-file, GLB and rover checks registered.
- Cover all built `scripts/e2e_mp.py` scenarios: M1/M4/M6 in `smoke_mp.py`, and
  M2/M3/M5/M8/M9/M10 explicitly. Run the stock-Lua Jetpack and Hillclimb checks.
- Replace a binary asset with invalid bytes, reject activation, then load that same path
  from a handler in the fallback VM: it must still read the original asset and identity.
- A fresh checkout must generate the ignored moon surface before the rover surface smoke;
  imported E-2000 geometry must remain tracked. No personal root-level downloads are committed.
- Run workspace check and tests through LoadTable, including the Lua round-trip suite.
- Review production Rust against the expert checklists: reusable physics, rendering, audio,
  assets and multiplayer infrastructure only. Vehicle assembly, input policy, missions,
  scoring and game content stay in Lua. Preserve the independent mechanics bench.

Status: integration authorized by the user on 2026-11-10; completed and validated. Jetpack Arena and
laptop-host are already on dev. Previous branch-specific verification is historical;
the combined checks and corrective reruns are recorded below.

## Steps

1. Preserve Hillclimb's uncommitted SVG, maps and audio foundations in a source-branch commit.
2. Merge jetpack-mp, then hillclimb, then moon-rover into dev, retaining both sides of shared
   host/physics changes and all smoke registrations.
3. Resolve integration failures and fresh-checkout asset setup. Update current contracts/status
   without deleting dated design history. Sound loops, game sounds, file decoding, signed match
   results and device multiplayer acceptance remain unfinished.
4. Run the acceptance checks and a fresh `pi -p` expert review. Commit the reviewed integration;
   prepare dev for pushing. Do not include credits or sharded-datasets in this integration.

## Review revision — 2026-11-10

The fresh expert review found that binary loader closures followed the mutable source doc and
that replacing a file deleted the fallback VM's original blob. Pin each VM's path bindings;
before replacing a bound path, retain its original bytes for that VM. This keeps sealed blob
cleanup unchanged and retains at most that revision's bounded item payload until successful
reload or close. The GLB smoke gains a lazy-load handler after a deliberately failed asset
reload, also checking `me()` survives the combined file/identity opening path.

The native folder picker currently imports Lua/OSV/SVG only; agents' `upload_folder` also imports
binary files. Extending the native picker is follow-up work, not a silent promise of this merge.

The complete 54-script sweep exposed a second integration issue: the shared RPC reader's
16 MiB cap rejected base64 binary uploads before the 64 MiB file validator could answer.
The private shell automation socket now opts into a bound for one base64 64 MiB file plus
a 64 KiB JSON envelope. Default RPC/node reads retain their 16 MiB bound. Framing tests pin
header-first rejection, and the binary smoke covers a 13 MiB round trip plus a semantic
oversize refusal.

## Verification — 2026-11-10

- Workspace check: 2716. Workspace tests: 2694 (1,376 passed, 26 ignored); app-host
  re-run after the asset correction: 2707 (381 passed, 8 ignored); RPC framing: 2713
  (7 passed). Release shell and kunki: 2714. Existing compiler warnings remain.
- Full 54-script sweep: 2709 (53 scripts successful, including two optional Android
  skips). The binary-file failure is corrected; 2715 passes bridge, binary files, GLB,
  host and multiplayer smokes. Every registered smoke has passing coverage across these runs.
- Multiplayer: 2711 passes M2/M3/M5/M8/M9/M10; 2715 passes M1/M4/M6. The three M8
  peer screenshots and GLB pixels were inspected. Physical-device acceptance remains pending.
- Pure Lua: Jetpack 2690; Hillclimb vehicle/run/maps and lunar terrain/course 2691.
- Fresh reviews identified the binary revision bug, which is corrected and covered; the
  user explicitly waived further review. The native picker limitation remains documented.
