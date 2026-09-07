# Archive

Docs that are no longer current truth. Kept readable — this project's design history is
worth more than its diff log — but nothing here should be followed without checking the
live docs first.

**The era break is 2026-06:** everything before it describes the sthalam/egui world, which
was abandoned (see `../blog/why-we-built-our-own-runtime.md`); everything after is the
`runtime`/`shell2` rebuild. `../architecture.md` is the current map.

| doc | era | why it's here |
|---|---|---|
| `app-host-handoff.md` | May 2026 | the old egui app_host handoff; its job is done |
| `osv.md` | May 2026 | the `.osv` manifest + workspaces/pages/layers access model. **Not dead — future:** it is the direction for permits and cross-app access (deferred deliberately in w3 §3). Archived because nothing implements it and the grammar will be redesigned at the auth rewrite. |
| `spaces-pages-apps-nav-brief.md` | May 2026 | nav model brief for the old shell |
| `document-editor-brief.md`, `doc-crate-architecture.md` | Jun 2026 | written against `block_doc`/`doc_editor`, now reference-only crates. M3 gets fresh docs against parley, not egui. |
| `app-isolation-draft.md` | Jul 2026 | rough draft that never got shaped; superseded by the shipped sandbox (`app_host/src/lib.rs`, `sandboxed_vm`) |
| `w1.md`, `w2.md`, `w3.md` | Jul–Sep 2026 | weekly execution notes — the densest decision records of the rebuild. Done/to-do extracted into `../status.md`; still-true design lives in `../architecture.md`, `../lua-apps.md`, and the live design notes. |
| `runtime-3month-plan.md` | Jul 2026 | the 13-week sequencing. Superseded by `../status.md`. |
