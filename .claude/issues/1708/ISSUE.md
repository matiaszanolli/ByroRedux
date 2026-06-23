# SAVE-D1-01: reproducible-CRC-across-runs holds only at column-key level — sparse columns are insertion-ordered

Labels: bug medium tech-debt 

- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: none (doc/contract mismatch — the save still round-trips correctly)
- **Location**: docstring `crates/save/src/snapshot.rs:40-42`; row source `crates/save/src/registry.rs:85` (`q.iter().collect()`); iteration order `crates/core/src/ecs/sparse_set.rs` ("Iteration is dense but not sorted by EntityId") vs `crates/core/src/ecs/packed.rs` (entity-sorted)

## Description
`Snapshot.components`/`.resources` are `BTreeMap`, so column **keys** are deterministic. But the **rows** within a column come from `World::query::<T>().iter()`. For `PackedStorage` (Transform, Children, GlobalTransform) that is entity-id-sorted (stable). For `SparseSetStorage` (Name, Inventory, EquipmentSlots, LightSource, LightFlicker, AnimationPlayer, AnimationStack, ScriptTimer, FormIdComponent) it is **insertion order**, and swap-remove on despawn reorders the dense array. Two playthroughs reaching the same logical state via different spawn/despawn histories produce different row orders in sparse columns → different JSON byte order → different CRC. The "reproducible CRC across runs at equal state" claim is therefore false for sparse columns; it is a deterministic-per-run-at-equal-history hash, not a content hash.

## Evidence
`sparse_set.rs` iter zips `dense`/`data` (insertion order); `packed.rs insert_bulk` sorts by entity id. The CRC is computed over the serialized payload (`encode`), which serializes rows in iteration order.

## Impact
No data loss — restore is order-independent (`insert_batch` re-sorts/keys by id). The only consequence is that the docstring's "reproducible CRCs across runs at equal state" / "stable diffs" promise overstates the guarantee.

## Suggested Fix
Either sort each saved column by entity id in `save_world` before serialize (cheap; makes the CRC a true content hash and stabilizes diffs), or soften the docstring to "deterministic per run / column-key order is stable; row order follows storage iteration."

## Completeness Checks
- [ ] **TESTS**: If columns are sorted, a test pins a stable CRC across two differing spawn/despawn histories reaching equal state
