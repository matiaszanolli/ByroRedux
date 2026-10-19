# NIF-SD-04: `NifImportRegistry::clear()` leaks clip-registry slots, and a debug-load archive change leaves the path-keyed `SceneImportCache` stale

Labels: low,nif-parser,nif,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6117

**Severity**: LOW · **Dimension**: Allocation Hygiene (cache lifecycle) · **Games**: all; debug `cell.load` across different archive sets only

## Location
`byroredux/src/cell_loader/nif_import_registry.rs` (`NifImportRegistry::clear`); `byroredux/src/debug_load.rs` (`invalidate_nif_cache_on_archive_change`); `byroredux/src/scene_import_cache.rs` (no `clear`)

## Description
`clear()` empties `clip_handles` but returns nothing, whereas every other removal path hands the freed handles back so the caller can `AnimationClipRegistry::release` them (#863). The registry's only caller is `invalidate_nif_cache_on_archive_change`, which then does not release them either. The same function clears the `NifImportRegistry` "to avoid stale cross-load model reuse" (#2078) but not `SceneImportCache`, which keys scenes by lowercased path alone and holds every NPC skeleton/body/head/armour import. `studio_host.rs` already knows two titles ship different files under one path and qualifies its label with the game.

## Evidence
`NifImportRegistry::clear` body (resets `core`, `access_tick`, `next_tick`, `evictions`, `clip_handles`; returns `()`); `grep -rn "SceneImportCache" byroredux/src` shows no clear or invalidate call.

## Impact
After a debug load that switches archive sets (e.g. FNV then Skyrim in one session), NPC spawns reuse the previous game's skeleton/body scenes, and each registry clear strands the keyframe arrays of every animated cached model. Debug path only; no production caller.

## Related
#2078, #863, #3760.

## Suggested Fix
Make `clear()` return the freed handles (like `insert`) and release them in `invalidate_nif_cache_on_archive_change`, which should also reset `SceneImportCache`.

Source: docs/audits/AUDIT_NIF_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
