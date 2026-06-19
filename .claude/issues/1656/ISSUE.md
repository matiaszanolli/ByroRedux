**Severity**: LOW · **Dimension**: Cell Loading (test coverage)
**Location**: `byroredux/src/cell_loader/unload_greyscale_lut_tests.rs:65-87` (`unload_walk_collects_all_texture_handle_components`)
**Status**: NEW — CONFIRMED this sweep

## Description
The test named for sweeping *all* texture-handle components on unload covers `TextureHandle` / `NormalMapHandle` / `DarkMapHandle` / `GreyscaleLutHandle` but never constructs `ExtraTextureMaps` — the largest texture-bearing component (6 slots: glow / detail / gloss / parallax / env / env_mask), each acquired via `resolve_texture` at spawn time (`byroredux/src/cell_loader/spawn.rs:916-921`) and swept on unload (`byroredux/src/cell_loader/unload.rs`, `collect_victim_gpu_handles` querying `ExtraTextureMaps`).

## Evidence
- The test inserts only the four single-handle components (`unload_greyscale_lut_tests.rs:71-75`); `ExtraTextureMaps` is absent from the fixture.
- Production code IS correct — `collect_victim_gpu_handles` queries `ExtraTextureMaps` (`unload.rs:259`) and releases all six slots — so this is a coverage gap, not a runtime leak.
- A future edit that dropped the env / parallax / env_mask arms from the unload walk (leaking up to six texture refcounts per env-mapped FNV mesh per cell-unload cycle) would still pass this "all components" test.

## Impact
Diagnostic / coverage only. No live defect on FNV. Blast radius is the loss of a regression guard on a per-cell-cycle texture-refcount leak.

## Related
Mirrors the per-cell texture-refcount sweep guards (#1338 / #1341 / #627).

## Suggested Fix
Add an `ExtraTextureMaps { glow, detail, gloss, parallax, env, env_mask, .. }` entity to the fixture and assert all six non-zero handles appear in the collected `texture_drops`.

## Completeness Checks
- [ ] **SIBLING**: Same six-slot coverage check applied to any parallel unload/eviction walk that consumes `ExtraTextureMaps`
- [ ] **TESTS**: The augmented fixture asserts all six `ExtraTextureMaps` handles (glow/detail/gloss/parallax/env/env_mask) appear in `texture_drops`, and handle 0 (placeholder) never does
