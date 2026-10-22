# #6209: ECS-2026-10-21-02: `collect_victim_gpu_handles` takes `ParticleEmitter` under seven storage guards
STATE: OPEN
LABELS: bug,ecs,high,concurrency

**Source**: docs/audits/AUDIT_ECS_2026-10-21.md · **Severity**: HIGH · **Dimension**: 1 Lock Ordering & Deadlock Machinery · **Location**: `byroredux/src/cell_loader/unload.rs:809-816`

- **Location**: `byroredux/src/cell_loader/unload.rs:809-816` (eight read guards held together for the victim walk; `peq` is the eighth). The reverse order is at `byroredux/src/render/particles.rs:84-87` (`ParticleEmitter` then `TextureHandle`).
- **Status**: NEW, introduced by `5f7e01fbe` (Fix #6109, 2026-10-21 09:17). The CI run at `21b9743e0` (the first after it) shows 27 cycle panics through `ParticleEmitter`: `TextureHandle`, `GlobalTransform` and `WaterVolume`, the last two transitive through the new edges. At HEAD the local run shows 7 failures on the `TextureHandle → ParticleEmitter` cycle: `render::particles::quantize_fade_tests::*` (6) and `render::water_wave_params_tests::water_reemit_ignores_particle_draws_that_share_the_plane_id`.
- **Description**: The walk holds `MeshHandle`, `TextureHandle`, `NormalMapHandle`, `WaterNoiseMapHandles`, `MaterialTextureHandles`, `TerrainTileSlot` and `AnimatedTextureFlip` read guards, and #6109 added `ParticleEmitter` as an eighth. That records seven `X → ParticleEmitter` edges. The particle render pass takes `ParticleEmitter → TextureHandle`, and `systems/particle.rs:408`'s TypeId-sorted `query_2_mut::<GlobalTransform, ParticleEmitter>` adds a `ParticleEmitter`/`GlobalTransform` edge, so cycles close directly and transitively. The block comment above the guards ("pure reads have no such risk") is the premise the detector rejects: it records read→read edges too, and std's RwLock queues readers behind a waiting writer.
- **Evidence**: see the location above, and the panic text in `/tmp/audit/ecs/bin_lockcheck.log` (`attempted acquisition of TextureHandle while holding ParticleEmitter … TextureHandle → ParticleEmitter → TextureHandle`).
- **Impact**: The caller holds `&mut World` (`unload.rs:481`), so there is no live deadlock. The lane stays red for a second reason, which hides real regressions (see -01), and the comment misleads the next contributor who adds a ninth guard.
- **Related**: #6109, #883 (the single-walk consolidation the comment belongs to), ECS-2026-10-21-01.
- **Suggested Fix**: Collect the emitters' `greyscale_lut_index` in a separate pass under the `ParticleEmitter` guard alone, before or after the seven-guard walk. Then correct the comment: read guards are lock-order edges.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

