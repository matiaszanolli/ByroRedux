# #790 — E-N1: AnimationClipRegistry is grow-only — leaks one clip copy per cell load

**Severity:** HIGH
**Audit:** `docs/audits/AUDIT_ECS_2026-05-03.md` (Dim 7)
**URL:** https://github.com/matiaszanolli/ByroRedux/issues/790

## Locations
- `crates/core/src/animation/registry.rs:19-23` — registry definition
- `byroredux/src/npc_spawn.rs:175-181` — unconditional `registry.add(clip)` in `load_idle_clip`
- `byroredux/src/cell_loader.rs:848-857` — `load_idle_clip` invocation per cell load
- `byroredux/src/cell_loader.rs:837-841` — misleading "doesn't grow per-NPC" doc to update

## Description

`AnimationClipRegistry::add` is monotonic — handle = `self.clips.len() as u32`, no removal API, no path-keyed dedup. `cell_loader::load_references` calls `npc_spawn::load_idle_clip` once per cell load with NPCs; that helper unconditionally `.push`-converts the parsed `idle.kf` and calls `registry.add(clip)`.

With M40 cell streaming (3×3 to 7×7 grids), every cell-crossing into a fresh NPC-bearing cell appends another full copy of the same `meshes\characters\_male\idle.kf`. `unload_cell` despawns NPC entities but never touches the registry — clips persist for the process lifetime.

Handles never alias stale data (registry is grow-only). The leak is purely allocator footprint.

## Impact

Memory growth proportional to `cells visited × NPC-bearing cells`. Long FNV exterior session leaks tens of MB of duplicated keyframe data.

## Fix Strategy

Either:
1. **Cache `idle_clip_handle` on a process-lifetime ECS resource** keyed by `(GameKind, Gender, kf_path)` and reuse across cell loads. (Smaller change. Matches `NifImportRegistry` pattern, #381.)
2. **Add `register_or_get_by_path` to `AnimationClipRegistry`** that dedups by path symbol before pushing.

Option (1) recommended. Update `cell_loader.rs:837-841` doc as part of the fix.

## Completeness Checks
- [ ] SIBLING: Check that other clip-loading paths (KF for non-NPCs, embedded NIF clips via `import_embedded_animations`) don't have the same leak
- [ ] LOCK_ORDER: If a new resource is introduced for the cache, verify it doesn't deadlock with `StringPool` or `AnimationClipRegistry` writes
- [ ] TESTS: Regression: load → unload → load same cell, assert `AnimationClipRegistry::len()` unchanged after the second load

## Next step
```
/fix-issue 790
```
