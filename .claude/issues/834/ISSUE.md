# Issue #834 (OPEN): NIF-PERF-05: Arc::from(type_name) per NiUnknown placeholder allocates instead of refcount-bump

URL: https://github.com/matiaszanolli/ByroRedux/issues/834

---

## Description

Each block-recovery / animation-skip path constructs an `NiUnknown { type_name: Arc::from(type_name), data: Vec::new() }`. Per #248 (cited at `blocks/mod.rs:103-105`), the entire reason `NiUnknown.type_name` is `Arc<str>` instead of `String` was to avoid per-block-name allocation when many blocks share the same name.

But these 4 construction sites all `Arc::from(&str)`, which **always allocates** a fresh `Arc<str>` storage with the bytes copied in. The fix #248 made NiUnknown's *field* an `Arc<str>` so callers *could* share storage — but the only caller (lib.rs) doesn't share. The header already stores type names in `header.block_types: Vec<String>` (one entry per **distinct** type in the file, then `block_type_indices` maps block→type_index).

Promoting that storage to `Vec<Arc<str>>` would let lib.rs `Arc::clone` the existing storage (atomic increment, no allocation) per recovery. This is the same change that closes NIF-PERF-01 — both findings should be fixed together.

## Location

`crates/nif/src/lib.rs:279, 406, 444, 469`

## Evidence

```rust
// lib.rs:279 — animation skip path
blocks.push(Box::new(blocks::NiUnknown {
    type_name: Arc::from(type_name),  // fresh alloc every time
    data: Vec::new(),
}));
// Repeated identically at lines 406, 444, 469.
```

```rust
// header.rs:24
pub block_types: Vec<String>,                // one entry per distinct type name
pub block_type_indices: Vec<u32>,            // one entry per block, indexes block_types
```

## Impact

On Skyrim Meshes0 archive walks (per #565 commentary), recovery paths fire thousands of times — each one paying a fresh ~24-40 byte allocation (Arc header + str payload). Per cell load with mid-volume recovery (~50-200 recoveries on SE Whiterun), ~5-10 KB of throwaway allocations. Combined with NIF-PERF-01 (counter-map keys), a typical Oblivion cell load sees ~150 KB of unneeded short-string allocs from the parse loop.

Mid-impact: visible in mass-archive tests, near-invisible per single cell load.

## Related

- NIF-PERF-01 (companion — same fix closes both)
- #248 (closed; this is a partial regression in the call sites that didn't get the storage benefit)

## Suggested Fix

Promote `NifHeader.block_types` from `Vec<String>` to `Vec<Arc<str>>` and `block_type_name` to return `Option<&Arc<str>>` (or just the `Arc<str>` cloned; either is cheap). Then in lib.rs the dispatch loop holds an `Arc<str>` and the 4 NiUnknown construction sites become `type_name: Arc::clone(&type_name_arc)`.

Same change unblocks NIF-PERF-01's `HashMap<Arc<str>, _>` migration.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit all callers of `header.block_type_name(i)` for sites that would benefit from the promoted return type
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing parse tests cover correctness; add an alloc-counter test on a high-recovery archive (e.g. SE Meshes0)

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM5.md` — NIF-PERF-05
