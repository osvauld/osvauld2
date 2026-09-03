# Merge referee — validated merge on the node (2026-09-01)

> The detail behind "kunki as the merge referee", which `w3.md` §3 lists as deferred with no
> content. Records **what we take from Mergeable Replicated Data Types (MRDTs) and what we
> don't**, and why the node's validation hook has to change shape. Prior art: `osvauld/kunki`
> (`validation_service.rs`, `node_runtime.rs`), which already has the plumbing but the wrong
> contract.
>
> Status: **design only, nothing built.** Deferred behind the runtime rebuild. Not W3 scope.

---

## 0. The one-line conclusion

Keep Loro as the substrate. Steal exactly one idea from MRDTs — **merge against a common
ancestor** — and spend it on the node's policy hook: give validation `base`, `proposed` and
`diff` instead of ops alone. That single change moves policy from *"may this role touch this
path?"* to *"is the resulting document legal?"*, which is the class of rule we actually want.

---

## 1. What kunki does today, and the gap

`kunki/src/validation_service.rs:143` builds the whole contract:

```rust
let ctx = ValidationContext {
    layer_name, ops, from_did, role, page_id,
};
```

`shared/validation.lua` sees **the operations and who sent them. Nothing else.** No before-state,
no after-state. So the expressible policy set is exactly the authorization checks:

| policy | needs | works today |
|---|---|---|
| "viewers can't edit `status`" | the ops | ✅ |
| "a column may not exceed 50 cards" | the **result** | ❌ |
| "you may only edit rows you own" | the **base** — the op says `set [cards,k1,owner]`; ownership lives in the pre-state | ❌ |
| "the budget total must stay under X" | the **result** | ❌ |
| "don't delete a card that has comments" | **base + result** | ❌ |

The gap is **invariants over state, not over operations**. Everything below exists to close it.

---

## 2. What we take from MRDTs

An MRDT replaces `merge(a, b)` with `merge(lca, a, b)` — three-way, against the lowest common
ancestor. Git's model, generalized past text.

That third argument restores the information a two-way state merge destroys. `{x}` vs `{}` is
ambiguous — add or remove? With an ancestor it is not: the ancestor had `x`, one side dropped it,
that is a remove. Consequence in the papers: the *type* stops needing tags and tombstones, because
the ancestor supplies what they were carrying.

We do not need that consequence — Loro's containers already merge correctly. We want the
**ancestor-aware policy check**, and one property CRDTs structurally cannot offer:

> Merge in a semilattice is **total** — it always yields an answer. MRDTs make merge a **partial
> function**. A referee that can say *no* is a partial merge, and that is the shape we are building.

The correspondence, which is why this reads as an MRDT merge rather than a validation hook:

| referee step | MRDT |
|---|---|
| receive the diff | the incoming version `b` |
| apply it on a branch | fork at the LCA, apply |
| check against policy | the merge function's guard |
| publish to the canonical doc | commit the merge |
| **reject** | **merge returns failure** |

---

## 3. The contract change

```rust
struct ValidationContext {
    // unchanged
    page_id, layer_name, from_did, role,
    // the diff — what was asked for
    ops: Vec<Parivarta>,
    // NEW: the two states
    base:     LoroValue,   // the forked doc before the ops
    proposed: LoroValue,   // after importing them
}
```

Three inputs, three questions a policy can now ask:

```
base      what was true before      → ownership, prior values, what existed
proposed  what would be true after  → totals, counts, cardinality, referential integrity
diff      who asked, and for what   → authorization, rate, blast radius
```

One struct, and the expressible policy set changes category. **This is the whole steal.** Every
other section is mechanism or caveat.

Keep the merge function *supplied*, not baked in — per layer, per role, in
`shared/validation.lua`. That is MRDT's other structural idea (merge belongs to the type, not the
store) and kunki already has the shape right.

---

## 4. Mechanism on Loro

Loro gives us the LCA for free. Remember that a version vector is a lattice point: **join is
pointwise max (merge), meet is pointwise min (common ancestor)**.

```
                                                       API                          verified
1. ancestor  = meet(canonical_vv, incoming_vv)          pointwise min
2. frontiers = doc.vv_to_frontiers(&ancestor)           loro/src/lib.rs:861
3. branch    = doc.fork_at(&frontiers)?                 loro/src/lib.rs:158
4. branch.import(&diff_bytes)?                          loro/src/lib.rs:710
5. proposed  = branch.get_deep_value()
6. base      = doc.fork_at(&frontiers)?.get_deep_value()
7. policy(base, proposed, diff) -> bool
8. on pass:  canonical.import(&diff_bytes)?             import is idempotent + commutative,
   on fail:  drop the branch                            so applying the same bytes twice is safe
```

Notes that matter:

- **`fork_at` takes `Frontiers`, not a `VersionVector`.** The meet gives you a VV; step 2 is not
  optional. `frontiers_to_vv` (`lib.rs:840`) is the inverse and returns `Option` — the tips must
  be in your DAG.
- **`fork()` is documented O(n) in time and space** (`lib.rs:145`). Per-request forking on a hot
  page will show up in a profile. If it does, keep one warm scratch doc per page and `checkout`
  instead of forking.
- **`LoroDoc::clone` is a reference clone**, not a fork. Never use it here.
- Steps 3 and 6 fork the same point twice; collapse them once the shape is real.

---

## 5. The rollback trap, and the split that avoids it

**A peer that already applied the op locally is ahead of a history that then rejected it.** CRDT
state is monotone — the op set only grows — so there is no clean undo, and any local ops built on
top of the rejected one have to go with it.

Do not solve this with rollback. Split the policy in two:

| tier | runs | checks | on failure |
|---|---|---|---|
| **local** | on every peer, before the op is created | deterministic rules over local state | the op never exists — nothing to undo |
| **node** | on the referee, authoritative | anything needing global state, other users' data, or secrets | rare, and a surprise by then |

Same `validation.lua`, two call sites. The node stays the authority; the local run makes rejection
almost never surprise anyone.

**Loro has a first-class hook for the local tier:**

```rust
pub type PreCommitCallback =
    Box<dyn Fn(&PreCommitCallbackPayload) -> bool + Send + Sync + 'static>;
    // loro-internal/src/pre_commit.rs:14 — exposed as LoroDoc::subscribe_pre_commit
```

It returns `bool` and its payload carries a `ChangeModifier`. Reject there and nothing enters
history at all.

---

## 6. Determinism is a requirement, not a style note

Two nodes evaluating the same diff must reach the same verdict, or the referee becomes a source of
divergence rather than a cure for it. So the policy function must be:

- **deterministic** — no clocks, no RNG, no network reads
- **side-effect free** — no writes back into the doc from inside the check

kunki already spawns the validation runtime with `ui_enabled: false` and `user_role: "node"`
(`validation_service.rs:127-140`), which points the right way. Make it a stated rule rather than an
accident of construction.

**Rejection does not break convergence.** The referee decides which ops *exist*; every peer that
received the same accepted set still converges by the same algebra. We are gatekeeping the CRDT's
input, not weakening the CRDT.

---

## 7. What we are not taking

- **Irmin's content-addressed store.** Loro's frontiers already name branch points, and no mature
  Rust equivalent exists. Building the store, the LCA computation, and derived merges is a research
  project, not a week.
- **Merge functions derived from a relational specification.** We want a Lua function a person can
  read at 2am.
- **MRDTs as the substrate.** Their strength is "two people diverged for three days." Ours is an
  agent and a human editing a live app seconds apart — which is what Loro's containers are for.

---

## 8. Before this gets built

- [ ] Decide whether `base`/`proposed` cross into Lua as full `LoroValue` trees or as a lazy
      handle. Full trees are simplest and match the mirror; a large page makes that a real cost.
- [ ] Confirm the `fork_at` cost on a realistic page before committing to fork-per-request.
- [ ] Decide what the peer is told on rejection — an error, or a corrective update that returns it
      to the canonical state. The second is friendlier and strictly more work.
- [ ] Related and separate: **source code as CRDT** wants this same three-way shape (see
      `future-directions` in memory). The conclusion there was that granularity beats merge
      theory — one container per function/block makes most conflicts never form. Do not let the
      referee design get pulled into solving code merge; they share a mechanism, not a schedule.
