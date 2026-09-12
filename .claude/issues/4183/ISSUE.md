# CONC-D3-01: `submersion_system` samples `TotalTime`/`WindField` underneath the `WaterPlane`/`WaterVolume` storage guards, inverting the documented snapshot-before-storage discipline

Labels: medium,concurrency,water,bug

**Description**: `submersion_system` binds `world.query::<WaterPlane>()` and `world.query::<WaterVolume>()`, then acquires `try_resource::<TotalTime>()` and, inside the closure with that guard still live, calls `weather_wave_adjustment(world, time.0)`, which itself acquires `try_resource::<WindField>()`. Both water storage guards stay live until an explicit `drop` later in the function. This records four lock-order edges nothing else in the tree records: `WaterPlane/WaterVolume -> TotalTime/WindField`. The other three consumers of the identical wave-parameter pair all snapshot the frame-global resources into plain scalars *before* taking any water storage guard — one of them documents this explicitly as "resource-snapshot-before-storage discipline (#3265)". No live cycle exists today (checked every `WindField`/`WaterPlane`/`WaterVolume` acquisition site) — this is latent, not live.

**Evidence**:
`byroredux/src/systems/water.rs` (confirmed): `wq`/`vq` queries bound first, then `wave_adjustment` computed via `try_resource::<TotalTime>()` while those guards are still in scope, guards dropped later in the function. `crates/physics/src/water.rs:637-645` and `byroredux/src/render/water.rs:102-131` both follow the correct resource-first order; `character.rs:1043-1056` documents the discipline explicitly.

**Impact**: (a) The moment any code takes `TotalTime`/`WindField` before a water storage read — the natural spelling, since 3 of 4 existing sites do resource-first — the graph closes a real ABBA cycle and a `BYRO_LOCK_ORDER_CHECK=1` run aborts. (b) `submersion_system` is currently `add_exclusive_with_access`, so today's safety is circumstantial — promoting it to a parallel lane is a one-line change with no compile-time or test-time signal.

**Related**: #3265 (the discipline violated), #2388/#313 (inverted-pair debug aborts), `docs/engine/ecs.md` Sec Canonical acquisition order.

**Suggested Fix**: Hoist the frame-global sample above the storage guards to match `player_water_state` exactly — move the `wave_adjustment` binding before the `query::<WaterPlane>()` call, as a plain `Option` scalar. Three lines moved, no behavioral change.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
