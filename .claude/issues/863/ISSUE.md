## FNV-D3-NEW-04: AnimationClipRegistry is grow-only — leaks under LRU eviction cycles

## Source Audit
`docs/audits/AUDIT_FNV_2026-05-05.md` — Dim 3 MEDIUM

## Severity / Dimension
MEDIUM / Animation × M40 doorwalking — bounded memory leak

## Location
`crates/core/src/animation/registry.rs:8-89` (registry — no `release()` API)
`byroredux/src/cell_loader.rs:1212-1217, 1591-1599` (cell-loader inserts clip handles)
`byroredux/src/cell_loader_nif_import_registry.rs:262-265` (LRU eviction site)

## Description
`AnimationClipRegistry` is documented as grow-only by design (registry.rs:11-14). The cell loader registers each unique embedded NIF clip via `clip_reg.add(clip)` and stashes the returned `u32` handle in `NifImportRegistry.clip_handles` keyed by lowercased model path.

When `BYRO_NIF_CACHE_MAX` is set (M40 doorwalking — #635), `NifImportRegistry::insert` evicts LRU victims and removes the corresponding entry from `clip_handles` (`cell_loader_nif_import_registry.rs:265`). However, **the underlying `AnimationClipRegistry.clips[handle]` slot is not freed** — the registry has no `remove()` API. After enough LRU eviction cycles, re-parsing the evicted NIF allocates a **new** clip handle, growing `AnimationClipRegistry.clips` unboundedly.

## Evidence
`cell_loader_nif_import_registry.rs:262-265`:
```rust
// #544 — drop the memoised clip handle in lockstep so
// a future re-parse of the same key registers a fresh
// clip rather than reaching into the
// `AnimationClipRegistry` for a stale handle pointing
// at a clip that was logically discarded.
self.clip_handles.remove(&victim_key);
```

The comment correctly identifies the issue but the upstream `AnimationClipRegistry` slot stays live.

`registry.rs:9-14`:
```rust
/// The registry is grow-only by design: handles never alias stale data
/// after a cell unload, so any held `clip_handle: u32` (in
/// `AnimationStack` layers, `AnimationController` catalogs, etc.) is
/// guaranteed to point at the same clip for the process lifetime.
```

The `add()` API at registry.rs:44 has no counterpart for removal.

## Impact
Bounded under the default `BYRO_NIF_CACHE_MAX=0` (unlimited cache → no LRU eviction → no re-parses). With a finite cap and long doorwalking sessions, every evict-then-revisit cycle adds one keyframe-array's worth of memory to `AnimationClipRegistry`.

For Whiterun + interior shopping district + walk-to-Riverwood typical sessions on Skyrim SE this could accumulate hundreds of MB over hours of play. FNV is less affected (smaller animation count, smaller cells), but the leak class is real.

**M40 Phase 2 doorwalking is exactly the workload that triggers it.**

## Suggested Fix
Add `AnimationClipRegistry::release(handle: u32)` that clears the slot's keyframes and marks the slot reusable (or use a generational handle scheme so handles tagged with the wrong generation hit the `None`/identity fallback in `compute_palette_into`). Then, at the LRU eviction site in `cell_loader_nif_import_registry.rs:262`, also call `release()` on the freed handle. Update the registry comment to reflect "freeable but generation-tagged".

## Related
- `FNV-D3-NEW-03` — streaming worker bypasses cache (compounds the leak)
- `FNV-D3-NEW-05` — `finish_partial_import` re-runs `convert_nif_clip` (compounds further)
- `FNV-D6-NEW-07` — registry `get_or_insert_by_path` doesn't lowercase keys (foot-gun for M42)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check whether any `AnimationStack` or `AnimationController` holds `clip_handle: u32` that could become stale post-`release()` — if so, generational tagging is the safer approach
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A — registry is `&mut self` already
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic LRU cycle (`BYRO_NIF_CACHE_MAX=2`, insert 3 clip-bearing NIFs, re-insert evicted one); assert `AnimationClipRegistry.len() <= 3` rather than 4
