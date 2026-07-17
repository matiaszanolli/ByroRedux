# MEM-D3-01: Bindless texture slots leak on every cell revisit

**Labels**: medium, performance, memory, bug

**Severity**: MEDIUM (HIGH-adjacent on GPUs with a smaller `maxPerStageDescriptorUpdateAfterBindSampledImages` limit, or very long streaming sessions)
**Dimension**: GPU Memory Pressure
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/renderer/src/texture_registry.rs:452-459,1024-1054,1063-1083`; `check_slot_available` `:221-229`; drop call site `byroredux/src/cell_loader/unload.rs:170`

## Description
`TextureRegistry` is strictly grow-only — every registration takes `self.textures.len()` as a fresh index. On cell unload, `drop_texture` purges the `path_map` entry once `ref_count` hits 0, so re-entering a previously-unloaded cell re-registers its textures as **new** slots rather than hitting the dedup cache. GPU image memory *is* reclaimed via deferred destroy — this is not a VRAM leak — but the finite bindless-array slot index (ceiling: `min(maxPerStageDescriptorUpdateAfterBindSampledImages, 65535)`) is never reclaimed. `check_slot_available` gates on cumulative `textures.len()`, including dead slots.

Verified current: `check_slot_available` (`crates/renderer/src/texture_registry.rs:221-229`) bails once `self.textures.len() as u32 >= self.max_textures`; `set_fallback`/registration paths still take fresh `self.textures.len()` indices with no free-list.

## Impact
Slow-motion, session-length exhaustion. At ~150 unique textures/cell, ~430 cell transitions exhausts the 65535 ceiling on the dev card (fewer on constrained devices). Degrades gracefully to a checkerboard fallback — no crash/corruption — but the degradation is total and permanent until process restart, with no telemetry surfacing the slot high-water mark.

## Related
#372 (handle-stability rationale for the grow-only design); mesh-registry analog carries the same shape but a practically-unreachable 16M-slot ceiling (see the companion `MeshRegistry` doc-rot finding).

## Suggested Fix
Add a generational free-list (recycle a dropped index after a deferred-destroy fence proves no live `GpuInstance.texture_index` references it), or track `live_count` separately and gate `check_slot_available` on it plus a periodic compaction pass at cell-unload boundaries. At minimum, surface the slot high-water in `ctx.scratch` / telemetry and document the ceiling in `docs/engine/memory-budget.md`.

## Completeness Checks
- [ ] **SIBLING**: `MeshRegistry` carries the same grow-only shape (practically-unreachable 16M ceiling) — confirm no fix is needed there yet, but keep the two in sync if one changes
- [ ] **DROP**: If slot reclamation requires new deferred-destroy plumbing, the Drop impl ordering must remain reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (e.g. asserting slot reuse after N cell-unload cycles)
