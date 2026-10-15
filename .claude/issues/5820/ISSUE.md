# TD2-2026-10-15-19: Four streaming-ring reconcilers each re-spell `desired_set` -> `retain` + unload-non-empty -> budgeted spawn; the Skyrim tree-LOD ring (2026-10-14) is the newest copy

Labels: low,terrain-exterior,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-19`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/cell_loader/object_lod.rs:261-290`, `placement_lod.rs:354-375`, `terrain_lod.rs:543-560`, `tree_lod.rs:405-420`; shared module `lod_support.rs:47` (`LodReconcileInput`), `:227` (`LodWorkBudget`), `:308`
- **Status**: NEW
- **Age**: `a9384f9c0` (2026-10-14) added the fourth
- **Effort**: small
- **Description**: `lod_support` already owns the input, budget and ordering; the "drop what left the ring, count unloads, skip empty sentinels" step is still inline in each (`blocks.retain(|coord, blk| if desired_set.contains(coord) { true } else { if !blk.entities.is_empty() { unload_*_block(world, ctx, blk); unloaded += 1; } false })`).
- **Evidence**: clone scan 7 identical windows `object_lod.rs:261` / `placement_lod.rs:354`.
- **Impact**: band-switch hysteresis and the empty-sentinel rule are per-ring policy today; a leak fix (the #863/#4986 class for LOD block GPU release) must be applied four times.
- **Related**: `docs/engine/exal.md`.
- **Suggested Fix**: `lod_support::retain_resident<K, B>(blocks: &mut HashMap<K,B>, desired: &HashSet<K>, mut unload: impl FnMut(&mut B)) -> usize`, one body returning the unload count.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
