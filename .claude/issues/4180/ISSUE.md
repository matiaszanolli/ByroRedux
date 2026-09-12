# CONC-D1-03: Two blocking one-time submits (build + compaction) run inside the per-frame path via `restore_missing_static_blas_for_draws`

Labels: medium,sync,renderer,performance,bug

**Description**: `restore_missing_static_blas_for_draws` is called every frame from the render driver (`byroredux/src/app_frame.rs:263`); when a TLAS-eligible rigid handle lacks a BLAS it runs the full `build_blas_batched` pipeline: a `submit_one_time` (builds + compaction-size queries) with a host fence-wait, then `get_query_pool_results` with `vk::QueryResultFlags::WAIT` (a second host stall), then a second `submit_one_time` for compaction copies with another fence-wait — up to `MAX_STATIC_BLAS_RESTORES_PER_FRAME = 256` meshes per frame, repeated every frame until the visible set is restored. Synchronization-correct, but load-time-shaped work executing in the frame loop; the fence wait lands on the same graphics queue as the still-in-flight previous frame.

**Evidence**:
`app_frame.rs:263` calls this per-frame, before `draw_frame`; `resources.rs:485-491` invokes `build_blas_batched`; `blas_static.rs` does two `submit_one_time` calls with a `WAIT` query readback between them; `constants.rs:166` sets the 256-mesh cap.

**Impact**: Multi-millisecond-to-second CPU+GPU stalls whenever LRU eviction has removed BLAS still visible — sustained hitching on over-budget cells, not a one-off load cost. Also the mechanism that makes CONC-D1-01's cross-submission window realistic on ordinary frames. Reproduce with `cargo run --release -- ... --bench-frames 300 --bench-hold` on a large exterior; correlate frame-time spikes with the "Restored {count} missing static shadow BLAS before TLAS build" debug log.

**Related**: #3540 (the per-frame cap), #1449 (why eviction stays deferred), CONC-D1-01.

**Suggested Fix**: Record the restore builds into the frame command buffer (as #911 did for skinned first-sight BUILDs), or move the restore into the streaming step in `about_to_wait` with an explicit per-frame budget, so no host fence-wait sits in the render driver.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
