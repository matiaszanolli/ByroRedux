# PERF-D3-2026-09-11-01: The mid-batch BLAS eviction check and #3979's admission gate are unreachable on the production static-BLAS path

Labels: medium,performance,renderer,memory,bug

**Description**: The mid-batch eviction trigger and the #3979 admission gate are both reachable only through `idx % BATCH_EVICTION_CHECK_INTERVAL (64) == 0` inside `build_blas_batched`'s Phase-1 loop, and the admission gate additionally needs `eviction_can_still_reclaim`, which is written *only* inside that same interval-gated block. The dominant production caller, `spawn_placed_instances`, submits one batch per placed reference (a single NIF's sub-meshes — single digits to low tens), never approaching `idx == 64`. So on interior and exterior loads alike, only the pre-batch eviction (evaluated against the accounting figure, not the resident one) ever runs. Worst case: the synchronous interior load path (`FrameTimeBudget::unlimited()`) spawns every REFR with no intervening `draw_frame`, so once a cell crosses budget, each subsequent per-REFR batch pre-evicts the paper figure back down while the evicted BLAS stay resident — true residency grows monotonically for the rest of the load. #3979 closed today added this admission gate; this finding is a residual reachability gap in that same fix, not a regression of it.

**Evidence**:
`crates/renderer/src/vulkan/acceleration/blas_static.rs:434-508` (`eviction_can_still_reclaim = true` init at :425, interval gate at :434-436, sole writer inside the gated block at :470); `constants.rs:81` (`BATCH_EVICTION_CHECK_INTERVAL = 64`); `byroredux/src/cell_loader/spawn.rs:708,748-751` ("Per REFR placement the loader calls `spawn_placed_instances`"); `cell_loader/references/mod.rs:284-303`; `cell_loader/load.rs:588`.

**Impact**: Unbounded real BLAS residency growth during an over-budget synchronous cell load, on exactly the small-VRAM (6 GB RT-minimum) configuration the budget machinery exists for. Not reachable on the 12 GB dev card. Failure mode is a logged "Batched BLAS build failed" (missing RT geometry for the cell), not corruption.

**Related**: Closed #3979 (the gate this neuters), #3840 (`pending_destroy_static_bytes`), #510 (the interval), #1792/#1793 (distinct, documented-not-fixed siblings), #3540.

**Suggested Fix**: Decouple `blas_admission_exhausted` from the loop index (it's three integer ops — cheap to evaluate every iteration); arm `eviction_can_still_reclaim` from the pre-batch eviction's paper-delta too. Keep the 64-interval on the `evict_unused_blas` *call* itself only.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
