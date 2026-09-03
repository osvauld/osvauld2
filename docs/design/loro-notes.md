# Loro — verified mechanics (2026-09-01)

> What Loro actually does, read out of the source rather than the docs. Written while building
> `app_host/src/crdt.rs`; the point is that most of this is expensive to re-derive and none of it
> is in Loro's published docs.
>
> **Versions:** `loro 1.13.9`, `loro-internal 1.13.9`, `loro-common 1.13.1`. Every line reference
> below was checked against those. Treat them as stale on the next bump — the *facts* survive
> version drift, the *line numbers* will not.
>
> Two crates matter and their types differ: `loro::` is the public API, `loro_internal::` is what
> it wraps. §6 is where that bites.

---

## 0. The model, in one page

The stored state is **a set of operations**, each with a unique id `(peer, counter)` and a
lamport stamp. Merge is **set union**:

```
a ⊔ b  =  a ∪ b
```

Union is commutative, associative and idempotent for free, so the semilattice laws cost nothing.
`⊥ = ∅` = the empty document. Idempotence comes from **op identity, not op content** — which is
why every op needs a globally unique id even when two peers write byte-identical values.

All the data-type behaviour lives in the **read function**, not the merge:

```
value = f(op_set)
```

Union is generic and stupid. A map, a list and a counter share the identical `⊔` and differ only
in `f`. **The CRDT is the op log; the value is a derived projection of it.** That asymmetry is the
shape of `crdt.rs`: writes go to the structure, reads come from the projection, and the projection
is disposable.

---

## 1. Three partial orders, not one

Easy to conflate, and conflating them is what makes the rest confusing.

| | order **on** | relation | job |
|---|---|---|---|
| ① causal | individual **ops** | `→` happens-before | defines lamport; constrains which states are legal |
| ② state | **op sets** (a replica) | `⊆` | **this is the one that merges** |
| ③ version | **version vectors** | pointwise `≤` | ② compressed |

② and ③ are the same order — ③ is a cheaper encoding of it. ① is a DAG, not a lattice you merge
on; nothing joins there.

```
②  decides what data moves      (sync)
①  decides what the data means  (merge outcome)
```

A legal op set is **downward-closed under `→`**: hold an op, and you hold every ancestor. That is
① fencing off which elements of ② are reachable.

---

## 2. Naming a version — three tools, three jobs

### Lamport — a number on one op

```
lamport(op) = 1 + max(lamport of everything the writer had seen)
            = the longest causal chain ending at that op
```

Loro computes it from the DAG tips rather than a stored counter:

```rust
let next_lamport = oplog_lock.dag.frontiers_to_next_lamport(&frontiers);   // txn.rs:352
// stamped at txn.rs:589, advanced at txn.rs:625
// frontiers_to_next_lamport: oplog/loro_dag.rs:1300
```

Max over the tips suffices — everything else is beneath a tip and therefore smaller.

One-directional, and the direction matters:

```
X → Y                     ⟹  lamport(X) < lamport(Y)     always
lamport(X) < lamport(Y)   ⟹  X → Y                       FALSE
```

**Equal lamport implies concurrent. Concurrent does not imply equal lamport.** To *detect*
concurrency, compare version vectors, not lamports.

### Version vector — one number per peer, on a replica

```rust
pub struct VersionVector(FxHashMap<PeerID, Counter>);              // version.rs:29
pub struct VersionRange(FxHashMap<PeerID, (Counter, Counter)>);    // version.rs:33
```

**How the compression works:** a peer numbers its own ops `1,2,3,…` with no gaps, so the ops you
hold from any one peer are always a contiguous **prefix**. Only the endpoint is needed.
`O(ops) → O(peers)`, losslessly.

The payoff is that you never decompress — every set operation has a counterpart on the compressed
form:

| on the op set | on the VV |
|---|---|
| `A ∪ B` — merge | pointwise **max** (`merge`, version.rs:255) |
| `A ⊆ B` — am I behind | pointwise `≤` (`partial_cmp`, version.rs:399) |
| `A ∩ B` — common ancestor | pointwise **min** |
| `A \ B` — what to send | per-peer range `(B[p], A[p]]` |

`PartialOrd`, not `Ord`, and it returns `Option` (version.rs:436):

```rust
if eq                 { Some(Ordering::Equal)   }
else if self_greater  { Some(Ordering::Greater) }   // they are behind — I send
else if other_greater { Some(Ordering::Less)    }   // I am behind — they send
else                  { None }                      // CONCURRENT — both send
```

That `None` is the formal definition of a conflict, in the type system.

**`VersionRange` exists because pending ops are not a prefix.** The type difference *is* the
compression story: a VV can only describe gap-free sets.

### Frontiers — the tips

Same information as a VV given the DAG, different shape and different cost:

```
VV         {A:3, B:2}         one entry per peer that ever wrote — never shrinks
Frontiers  [(A,3), (B,2)]     one id per live concurrent branch — usually 1
```

A doc edited by 500 people carries 500 VV entries forever, including everyone who left, while its
frontiers stays `[(X,9931)]`.

```rust
frontiers_to_vv(&f) -> Option<VersionVector>   // loro/src/lib.rs:840 — Option: tips must be in your DAG
vv_to_frontiers(&vv) -> Frontiers              // loro/src/lib.rs:861
oplog_frontiers()                              // loro/src/lib.rs:947 — tips of everything you HOLD
state_frontiers()                              // loro/src/lib.rs:967 — tips of what is MATERIALIZED
```

Use a **VV to subtract** (sync: what do I lack) and **Frontiers to name a point** (`checkout`,
`fork_at`). The two accessors diverge only in detached mode — the doc comment on `state_frontiers`
says so explicitly. Number of frontiers = number of unmerged branches live in your history.

---

## 3. Causal closure and pending changes

Causal delivery is not a nicety: **ops are written in terms of ids that earlier ops created.** An
op that says "insert into container `c`" is unresolvable if the op creating `c` has not arrived,
and a list insert saying "place after element `(A,7)`" cannot compute a position without `(A,7)`.
Neither is a merge conflict; both are dangling references.

```rust
fn push_pending_change(&mut self, missing_dep: ID, change: PendingChange)   // pending_changes.rs:122
pub(crate) fn try_apply_pending(...)                                        // pending_changes.rs:159
```

Pending changes are **filed under the id they are waiting for**. When that id lands,
`try_apply_pending` unblocks everything keyed to it and cascades. Three fates for an arriving op:

```
deps satisfied     → applied, VV advances
deps missing       → parked under the missing id, VV unchanged
missing dep lands  → auto-applied, cascade
```

Surfaced as `ImportStatus { success: VersionRange, pending: Option<VersionRange> }`
(`encoding.rs:228`).

**Pending changes exist to protect the VV's prefix invariant.** Held outside the op log, the VV
keeps describing a genuinely gap-free set. Loro therefore does not require a causally-ordered
transport — it enforces causal order internally.

---

## 4. The read function, per container

### Map — LWW on the whole key

```rust
pub struct MapValue { pub value: Option<LoroValue>, pub lamp: Lamport, pub peer: PeerID }
// delta/map_delta.rs:20

impl Ord for MapValue {                                          // map_delta.rs:26
    self.lamp.cmp(&other.lamp).then_with(|| self.peer.cmp(&other.peer))
}
```

**Values are never compared** — `"abe"` vs `"bob"` is irrelevant. Only `(lamport, peer)` decides.
`apply_local_op` is at `state/map_state.rs:299`. Deletion is an op in the same race, not a removal.

The peer tiebreak is arbitrary but **deterministic**, which is the actual requirement: every
replica computes the same winner. And because `X → Y ⟹ lamport(X) < lamport(Y)`, LWW follows the
causal order wherever ① has an opinion and only invents an answer where it is silent.

### Text and List — Fugue

`container/richtext/fugue_span.rs`, `container/richtext/tracker/crdt_rope.rs`.

Elements carry immutable ids and the ids they were inserted *between*; `f` topologically orders by
those anchors, tie-breaks deterministically, and skips tombstones. Tombstones stay in the
structure and vanish from the projection — they are still anchors for anyone who inserted beside
them. Fugue specifically is the low-interleaving choice.

### MovableList — two independent LWW registers

The one worth internalising, because the kanban runs on it:

```rust
pub struct ListItem  { pointed_by: Option<CompactIdLp>, id: IdFull }   // movable_list_state.rs:36
pub struct Element   { value: LoroValue, value_id: IdLp, pos: IdLp }   // movable_list_state.rs:42
```

ListItems are the Fugue-ordered **slots**; Elements are the **values**, each pointing at a slot.
Each element carries an LWW stamp for *what it holds* (`value_id`) and a separate one for *where
it sits* (`pos`):

| concurrent actions | register | outcome |
|---|---|---|
| two peers `set` the same element | `value_id` | LWW |
| two peers `move` the same element | `pos` | LWW |
| one peer **sets** while another **moves** | different registers | **both apply, no conflict** |

That third row is the entire reason `MovableList` exists. Under a plain list a move is
delete + reinsert, so a concurrent edit lands on a tombstone and is lost, and two concurrent moves
produce duplicates. Concretely: **dragging a card and renaming it concurrently both survive.**

`IndexType::ForOp` vs `ForUser` (`:54`) is the tombstone seam — vacated slots stay addressable by
ops, invisible to users.

### Counter, Tree

Counter: `f` = sum; no conflict possible. Tree: parent pointers plus a fractional index for
sibling order, and it needs cycle prevention (two peers can concurrently reparent A under B and B
under A). **We do not use Tree** — nothing in our write API creates one, so import should error
explicitly if one appears.

---

## 5. `get_deep_value()` — six container types collapse to two

| container | becomes |
|---|---|
| Map | `LoroValue::Map` |
| List / MovableList | `LoroValue::List` |
| Text | `LoroValue::String` |
| Counter | `LoroValue::Double` |
| Tree | `LoroValue::List` of Maps |

No `LoroValue::Container` survives — `state.rs:1561` has `LoroValue::Container(_) => unreachable!()`,
and tree node `meta` containers are resolved by `get_meta_value` (`tree_state.rs:1393`), which
recurses into `children`. Tree nodes come out shaped as a Map with `id`, `parent`, `index`,
`fractional_index`, `meta`, `children` (`tree_state.rs:1423`).

**This is why `patch_into` is 40 lines and not 400.** Only Map and List are recursive cases;
everything else is a leaf. By the time it runs, every CRDT decision has already been made — no
lamports, no peer ids, no tombstones, no Fugue anchors, no `pos` registers. They were all consumed
by `f`. The mirror code can afford to be dumb.

---

## 6. Snapshots and lazy state

```rust
pub enum ExportMode<'a> {          // encoding.rs:53
    Snapshot,                      // full history AND current state
    Updates { from: Cow<VersionVector> },
    UpdatesInRange { spans },
    ShallowSnapshot(Cow<Frontiers>),      // full state, history only since the frontier
    StateOnly(Option<Cow<Frontiers>>),    // state + a few ops
    SnapshotAt { version },
}
```

A `Snapshot` carries **history and the materialized state side by side**, which is what makes a
cold open fast — you get the answer and the ops that justify it. `Updates` carries history only.

**Import does not decode the state.** State is stored per container as raw bytes and decoded on
first touch:

```rust
pub encoded_state_bytes: Bytes,                              // state/container_store.rs:60
pub(crate) fn ensure_container(&mut self, id: &ContainerID)  // state/container_store.rs:312
pub(super) fn get_or_create_imm(&mut self, idx) -> &State    // state/container_store.rs:324
```

Their own test names state the guarantee: `first_lazy_read_caches_value` (:464),
`deep_value_read_keeps_imported_state_lazy` (:700), `snapshot_export_keeps_imported_state_lazy`
(:546). Open a doc with 10,000 containers and touch three — three get decoded. Import a snapshot
and re-export it and the bytes never decode at all.

**Cold open is `O(what you actually read)`**, not `O(ops)` and not `O(state)`.

The split that follows:

```
persistence  →  Snapshot   (opened cold, wants to be fast)
sync         →  Updates    (sent often, wants to be small)
```

Shallow modes discard history before a point — cheap, but you can no longer merge with a peer
behind that point, nor `checkout` earlier than it. Fine for an archive, wrong for a live doc.

---

## 7. Events and subscriptions

### The public types are not the internal types

This is the trap. `loro::event` defines its own borrowed pair:

```rust
pub struct DiffEvent<'a> {                    // loro/src/event.rs:30
    pub triggered_by: EventTriggerKind,
    pub origin: &'a str,
    pub current_target: Option<ContainerID>,
    pub events: Vec<ContainerDiff<'a>>,
}
pub struct ContainerDiff<'a> {                // loro/src/event.rs:43
    pub target: &'a ContainerID,
    pub path: &'a [(ContainerID, Index)],
    pub is_unknown: bool,
    pub diff: Diff<'a>,
}
```

`loro_internal` instead has an **owned** `ContainerDiff { id, path, idx, is_unknown, diff }`
(`event.rs:25`) inside a `DocDiff { from, to, origin, by, diff }` (`event.rs:86`). Through the
public API you see the borrowed pair, so **nothing from the event can be stashed for later** —
extract owned data inside the callback or lose it. `triggered_by` and `origin` are flat on
`DiffEvent`, not nested.

### Trigger kinds — there is no Export

```rust
pub enum EventTriggerKind { Local, Import, Checkout }    // loro-internal/src/event.rs:35
```

Three, and that is all. **`export()` does not fire an export event.** What it does:
`export` → `with_barrier` (`loro.rs:89`) → `implicit_commit_then_stop()`. If ops were pending that
commit fires a normal `Local` event; if you already `commit()` after each write, nothing is
pending and nothing fires.

**Use `triggered_by` for the wake decision.** Only `Import` needs an external repaint poke —
a `Local` change already has an input event behind it.

### What powers dispatch

```rust
queue: Arc<Mutex<VecDeque<DocDiff>>>          // subscription.rs:33
```

A subscriber set with a queue, plus a reentrancy guard (`subscription.rs:107`):

```rust
if inner.subscriber_set.is_recursive_calling(&None) || … {
    inner.queue.lock().push_back(doc_diff);
    return false;              // queue instead of dispatch
}
```

`emit` drains the queue after the current callback returns (`subscription.rs:79`). So a subscriber
that writes back to the doc will not recurse or deadlock — **Loro already solved reentrancy for
its own callbacks.** Events are also coalesced: a `DocDiff` may cover several transactions and
imports.

### The subscribe surface

| API | callback | for |
|---|---|---|
| `subscribe_root` | `Fn(DiffEvent)` | everything — what `crdt.rs` uses |
| `subscribe(container_id, …)` | `Fn(DiffEvent)` | one container's subtree |
| `subscribe_local_update` | `Fn(&Vec<u8>) -> bool` | **the update bytes, handed to you** — the sync hook |
| `subscribe_jsonpath` | path-scoped | "something under this path may have changed" |
| `subscribe_pre_commit` | `Fn(&payload) -> bool` + `ChangeModifier` | inspect / modify / **reject** before commit |
| `subscribe_peer_id_change` | | identity changes |
| `subscribe_first_commit_from_peer` | | new peer seen |

Declared at `loro/src/lib.rs:1035, 1056, 1099, 1134, 1372, 1593, 1651`. Callback types:

```rust
pub type Subscriber = Arc<dyn for<'a> Fn(DiffEvent<'a>) + Send + Sync>;   // subscription.rs:20
pub type LocalUpdateCallback = Box<dyn Fn(&Vec<u8>) -> bool + Send + Sync + 'static>;  // :16
pub type PreCommitCallback = Box<dyn Fn(&PreCommitCallbackPayload) -> bool + …>;  // pre_commit.rs:14
```

`subscribe_jsonpath`'s own doc comment is unusually candid and tells you how it is meant to be
used: *"may fire **false positives** (never false negatives) to stay lightweight; does **not**
include the query result so the caller can debounce/throttle."*

`Subscriber` being `Arc<dyn Fn + Send + Sync>` is why `crdt.rs` uses `Arc<AtomicU64>` rather than
`Rc<Cell<_>>`, and why the subscriber **physically cannot** capture the Lua VM (`mlua::Lua` is not
`Sync`). Any Lua-facing change notification has to be deferred to a frame boundary — the same
shape Loro uses internally.

---

## 8. The write side

Going the other way you are not producing a value, you are producing an **op** — which means
choosing which `f` will interpret it, forever:

```rust
map.insert(k, value)                          // ONE LWW register over the whole subtree
map.insert_container(k, LoroMap::new())       // a Map container — field-by-field merge
map.insert_container(k, LoroMovableList::new())  // Fugue ordering + move registers
```

`insert` with a `LoroValue::Map` puts the entire subtree in one op with one lamport, so a
concurrent edit anywhere inside it loses the whole subtree. **`insert` vs `insert_container` is
not a storage detail — it picks the merge function.** Always containers for app structure.

Consequence worth stating on its own: **container granularity is conflict granularity.**
Concurrent edits to `cards.k1.title` and `cards.k2.title` are ops on different containers and
never enter the same comparison. Schema shape decides how many conflicts can exist at all.

`doc.get_by_path(&[Index])` (`loro/src/lib.rs:1150`) resolves paths, where `Index` is
`Key | Seq | Node`. It cannot resolve a list element by an `id` *field* — that is app convention,
not CRDT structure, which is the whole reason `crdt.rs` needs its own id→position scan.

---

## 9. Corrections to earlier working assumptions

Recorded because each was believed and acted on at some point:

- **`export()` fires change events.** Half true and misleading. There is no `Export` trigger kind;
  the event is a `Local` commit of pending ops via `with_barrier`. With a `commit()` after every
  write there is nothing pending and no event. The feared "vault writes every frame" loop does not
  exist.
- **`ContainerDiff` is owned.** True in `loro_internal`, false in the public `loro` crate, which is
  what we consume. Both `DiffEvent<'a>` and `ContainerDiff<'a>` borrow — extract inside the
  callback.
- **`DiffEvent` carries an `event_meta`.** That is the internal type. Publicly, `triggered_by` and
  `origin` sit directly on `DiffEvent`.
- **`LoroDoc::clone` forks the document.** It does not — it is a reference clone sharing the inner
  doc. `fork()` / `fork_at()` are the real forks, and `fork()` is documented O(n).
- **Equal lamport is *the* signature of concurrency.** Only one direction: equal lamport implies
  concurrent, concurrent does not imply equal lamport. Detect concurrency with `partial_cmp` on
  version vectors.
