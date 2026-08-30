# #3760 — SAFE-2026-08-30-D3-01: `SceneImportCache` is an uncapped, never-evicted process-lifetime hold of every NPC skeleton / body / hand / armor `ImportedScene`

**Labels**: bug, medium, memory, safety

---

- **Severity**: MEDIUM
- **Dimension**: 3 — Memory & Resource Leaks (D3 item 5 — CPU-side unbounded growth)
- **Location**: `byroredux/src/scene_import_cache.rs` (`SceneImportCache`), backed by `byroredux/src/parsed_nif_cache.rs` (`ParsedNifCache::insert`); inserted at `byroredux/src/scene/nif_loader.rs`; registered at `byroredux/src/boot.rs`
- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (`SAFE-D3-01`), HEAD `64f64480`

## Description

`SceneImportCache` wraps `ParsedNifCache<ImportedScene>` and is the process-lifetime cache
for the *hierarchical* NIF import path (`load_nif_bytes_with_skeleton`). Its sibling
wrapper `NifImportRegistry` grew an LRU cap under **#635** (default 2048 entries,
`BYRO_NIF_CACHE_MAX` override) because the shared core deliberately does no eviction of its
own. **`SceneImportCache` never got the equivalent**: it has no `max_entries`, no
`access_tick`, no `remove` call site, and no `clear` anywhere in the tree (re-verified at
HEAD: 0 hits for `max_entries` / `access_tick` in the file; the struct is
`{ core: ParsedNifCache<ImportedScene>, bypass_parses: u64 }`).

Entries are only ever added, and each entry is a full `Arc<ImportedScene>` (positions
`[f32;3]`, colors `[f32;4]`, normals `[f32;3]`, tangents `[f32;4]`, uvs `[f32;2]`,
`indices: Vec<u32>`, plus skin data per mesh) — **roughly 60+ bytes per vertex retained on
the CPU heap in addition to the GPU copy.** That is a substantially heavier per-entry
payload than any of the three caches that were already capped.

## Evidence

The shared core states the contract explicitly — `byroredux/src/parsed_nif_cache.rs`:

```rust
/// Insert (or overwrite) an entry. […]
/// Does NOT do LRU eviction — that's
/// the wrapper's responsibility (only `NifImportRegistry`
/// supports LRU today).
pub(crate) fn insert(&mut self, key: String, value: Option<Arc<T>>) {
```

The wrapper adds nothing but a counter, and its `insert` is a straight pass-through to
`self.core.insert`.

Contrast the capped sibling, `byroredux/src/cell_loader/nif_import_registry.rs`:

```rust
let max_entries = std::env::var("BYRO_NIF_CACHE_MAX")
    .ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(2048);
```

Every `SceneImportCache` use outside its own file is exactly: `boot.rs` (`insert_resource`),
`scene/nif_loader.rs` (`record_bypass_parse`, `get`, `insert`). **There is no eviction, no
`remove`, no `clear` call site anywhere.**

The population is not just skeletons/bodies: the **armor/outfit** spawn phase routes
through the same cached call when `hidden_biped_mask == 0`
(`byroredux/src/npc_spawn/resumable.rs`), joining skeleton, body, head, and the generic
part loader. On Skyrim SE / FO4 the distinct-armor-mesh population reachable across a long
exterior-streaming session is in the **hundreds-to-thousands**, so the key set is not
naturally small the way `csg_cache`'s (load-order-keyed) is.

## Impact

Monotonic CPU-heap growth across a long session — roughly one full mesh's worth of host
geometry per distinct NPC-part NIF path ever seen, retained until process exit. Not
per-frame, so no frame-time cliff, but it is the same unbounded-by-construction shape that
#951 / #3054 / #635 each closed on the smaller caches, **on the largest payload of the
four**. It also defeats the "bounded arena" posture the rest of the streaming path
maintains: `NifImportRegistry` evicts at 2048 while its structurally identical sibling, fed
by the same cell-streaming traffic, does not.

## Related

#635 (LRU cap for `NifImportRegistry` — the exact fix shape), #951 (SAFE-26, `bgem_cache`
+ `failed_paths` unbounded), #3054 (SF-D3-01, `sf_cdb_cache` uncapped), #1430 (MEM-04,
clear-whole-map vs LRU), #863 / #1854 (clip-handle release must be ordered with respect to
eviction — the precedent to follow if this cache ever grows side-state).

No open or closed issue mentions `SceneImportCache` / `scene_import_cache`; this is the one
wrapper of the shared core that was never given a cap.

## Suggested Fix

Give `SceneImportCache` the same bounded shape as its sibling — either reuse
`NifImportRegistry`'s `access_tick` + `max_entries` LRU (lifting that machinery into
`ParsedNifCache` so both wrappers share it, which also removes the duplication the module
doc already flags), or, minimally, the half-eviction-on-overflow pattern `MaterialProvider`
uses in `asset_provider/material.rs`. Default the cap to a few hundred entries given the
per-entry payload, and honour the existing `BYRO_NIF_CACHE_MAX=0` unlimited escape hatch
for parity.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — any other `ParsedNifCache` wrapper added since
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix — insert past the cap and assert the entry count stops growing
