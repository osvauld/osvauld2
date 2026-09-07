# Storage

A lite embedded key-value store. One ordered keyspace over **redb**, raw bytes in and
out. Nothing domain-specific, no serialization, no crypto — it is the dumb byte layer
that everything else is persisted *through*.

## What it is (and isn't)

```rust
Store::open(path)          // file-backed (create or open)
Store::open_in_memory()    // redb in-memory backend — used by tests, no temp files
Store::open_readonly(path) // open an existing db without creating or initializing it

store.get(key)    -> Option<Vec<u8>>   // key = a sortable path string
store.put(key, &[u8])                  // one atomic, durable commit
store.delete(key)
store.list_prefixed(prefix) -> Vec<String>   // ordered key listing for a prefix
```

- **Keys** are `&str` — the full hierarchical path, e.g. `"space/page/layer/shard"`.
  There is no separate "namespace" argument: the namespace is just the leading part of
  the key. One address → one blob.
- **Values** are raw bytes. The store never deserializes, decrypts, or inspects them.
- **`Store` is `Clone`** (`Arc<Database>` inside), so it's shared cheaply across callers.

Everything else lives **above** this layer:

| Concern | Where it lives |
|---|---|
| Serialization (bincode) | the domain layer that owns the type |
| Encryption at rest | the service layer — `vault`'s `seal`/`unseal` over `identity::encrypt_for` is the live example |
| Dirty-tracking ("only write actual changes") | the scribe/cache layer |
| Time-sharding of documents | the document layer (encodes the shard into the key) |
| Namespacing | a prefix convention inside the key |

## Why redb (not sled / LSM / C libraries)

The workload is: load-on-open + periodic, modest, **per-layer atomic** writes of CRDT
snapshots (cached in memory, flushed only when changed), with values kept bounded by
time-sharding. That's a **B-tree, read-favoring** profile, not a high-throughput
streaming-write one.

- **redb** — pure Rust, ACID, ordered B-tree, mmap, light footprint, maintained, frozen
  on-disk format. Each `put` is its own atomic + durable commit, which is exactly the
  "a layer write is atomic" requirement. Ordered keys mean prefix/range scans drop in
  later with no schema change.
- **sled** — bytes-native and has dynamic trees + `watch_prefix`, but 1.0 has been a
  long-running alpha and 0.34 is effectively maintenance-only; heavier memory.
- **LSM stores (RocksDB, fjall)** — optimized for write throughput we don't have; we'd
  pay compaction/read-amplification for nothing. RocksDB also drags a C++ dependency.
- **LMDB / SQLite** — fast and rock-solid but C dependencies, which complicate the
  cross-platform desktop build; redb keeps the toolchain pure Rust.

Because we store only raw bytes, redb's typed-table feature goes unused — we use one
`TableDefinition<&str, &[u8]>` and ignore the typing. The choice rests purely on
durability, footprint, maintenance, and ordered-key range scans.

## Deferred (add when a consumer needs them)

- `scan_range(start, end)` — a time window of shards (needs lexicographically sortable
  shard keys: ISO dates, zero-padded counters, or big-endian epoch).
- reverse / `last(prefix)` — "newest shard" for open-doc loads.

These are a few lines each on top of the existing ordered keyspace — intentionally left
out of v1 since no consumer exists yet in the rewrite.

## Module layout

```
storage/src/
  lib.rs      re-exports (Store, StorageError)
  error.rs    StorageError (transparent wrappers over redb's error types)
  store.rs    Store: open / open_in_memory / get / put / delete   (+ store/tests.rs)
```
