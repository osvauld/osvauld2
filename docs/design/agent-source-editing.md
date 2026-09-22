# Agent source editing — precise edits to LoroText

**Status: implemented, 2026-09-22.** `ReadFileVersioned` and `EditFile` provide
revision-checked surgical edits through the bridge for open and closed apps. Source persistence,
staged activation, failure reporting, the Python client and a running-app smoke are wired.
This revises the near-term direction in [code-as-tree.md](code-as-tree.md): semantic Loro
storage, structural edit operations and the [nid channel](nid-channel.md) are deferred.

## Decision

**Source text remains authoritative. Agents edit it directly through the bridge.**
An app's source remains `files: path → LoroText` in its encrypted source document. Paths are
logical module names, not working files on disk. Agents need no checkout or temporary files.

An edit changes only the addressed character ranges. It does not delete and reinsert the whole
file, parse and normalize the whole file, or replace the running VM's closures in place.
The existing staged hot reload builds a replacement VM and swaps it into the running app.
Saving an encrypted Loro snapshot is independent of mutation granularity; snapshots remain
acceptable even when the document mutation is surgical.

## Bridge contract

`ReadFile` remains compatible. `ReadFileVersioned` returns `{ content, revision }`; the revision
is the SHA-256 digest of that file's UTF-8 content. It is an opaque equality precondition, not a
security claim. Changes to another file do not invalidate it.

An edit carries:

```text
item_id
path
expected_revision
edits: [{ old_text, new_text }, ...]
```

The host executes the operation on the shell UI thread, the existing authority for source
mutation. It must:

1. Validate the item/path and require an existing source file.
2. Reject a revision that no longer matches. No automatic rebase or fuzzy matching.
3. Require nonempty `old_text` with exactly one occurrence in the original source for each edit.
   Missing and ambiguous matches are errors. An agent supplies more context to disambiguate.
4. Resolve all edits against that same original source and reject overlapping ranges. Do not
   mutate anything until the complete batch passes these checks.
5. Convert matched byte ranges to Loro's Unicode character offsets, and apply replacements
   from the end backward so earlier offsets remain valid. Unaffected text retains CRDT identity.
6. Commit the batch once, persist through the existing source path, and trigger staged reload.

Insertion includes existing surrounding text in `old_text` and preserves it in `new_text`.
Deletion uses empty `new_text`. Identical replacements should be no-ops. File creation and
wholesale replacement remain `WriteFile` responsibilities; file deletion is outside this slice.

`EditFile` returns `{ revision, persisted, activation }`. `activation` is `"activated"` for a
running VM that matches the source, `"closed"` when no VM is running, or a `failed` value carrying
the reload error. A failed reload leaves the prior VM running while the edited source remains
durable.

Precondition failures must leave source untouched. This is not a claim of transactionality
across Loro mutation, vault persistence and VM activation. Persistence failures and unexpected
mutation failures need explicit handling and tests; a generic success response must not hide
partial outcomes.

## Guarantees and limits

This prevents an agent from silently applying an edit against a changed local file, and makes
its target exact rather than guessed. Strict per-file revision checks may reject independent
concurrent edits; the first version deliberately prefers reread/retry to automatic rebasing.

It does not guarantee program correctness or semantic merging. A text CRDT merges characters,
not intent. Future offline peer edits can still merge into invalid Lua despite local revision
checks. Compilation, runtime diagnostics and exercising the app remain the correctness loop.
Validation-before-persistence is not promised by this design; the existing reload trial is not
an isolated transaction over app data or a complete validation of all handlers and UI props.

## What is deferred

- Semantic nodes as the persistent Loro source representation.
- Structural `set`, `replace`, `insert` and move operations addressed by source node identity.
- Source nids and their propagation into rendered elements.
- Click-to-source highlighting, a useful later feature rather than an editing prerequisite.

`lua_tree` stays intact as isolated parser/printer groundwork, not a production dependency of
this editing path. No source migration or crate deletion is required. A future derived source
index can be reconsidered independently of the storage decision.

**Runtime/UI `id`s are not source nids.** Existing app IDs remain necessary for handlers,
retained state, scrolling and drag identity and are unchanged.

The earlier concern about normalization widening textual edits remains valid, but does not
apply here: this path splices the authored text and never writes a whole normalized projection.

## Proof

Unit tests pin unique/missing/ambiguous matches, stale revisions, overlaps, batched edits,
insertions, deletions, no-ops, Unicode offsets and snapshot reopening. The RPC round-trip suite
pins the wire vocabulary. `scripts/smoke_bridge.py` edits an already-running app, observes the
new UI, rejects a stale edit, persists invalid source while retaining the old VM, repairs it,
and verifies the repaired source after reopening. The bridge bounds a batch at 128 edits and
1 MiB of replacement text. Explicit persistence-failure injection remains unbuilt.
