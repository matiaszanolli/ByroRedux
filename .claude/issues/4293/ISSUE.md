# #4293: REN-2026-09-14-D4-01: the ground-cover counter clear issues three overlapping `vkCmdFillBuffer`s with no barrier between them — the only site in the crate that can explain the "10 vkCmdFillBuffer WAW hazards" noted in a commit message

- **Labels**: medium,renderer,sync,vulkan,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4293
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: MEDIUM. Rises to HIGH (the "validation errors in normal operation" floor) if a `BYRO_VALIDATION=1` run attributes those hazards to these calls.
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (`GroundCoverPipeline::record_scatter`, `buffer_barrier`)
- **Status**: NEW
- **Description**:
  - `record_scatter` clears the shared (not per-FIF) `counter_buffer` in three steps: a whole-buffer zero fill, then two 4-byte `EXTREMA_MIN_SEED` fills into ranges the zero fill already wrote. Only after all three does it emit a `TRANSFER → COMPUTE` barrier.
  - Nothing separates the seed fills from the zero fill. Under the sync model this crate follows (#4177/#4179/#4181), their order is undefined, and sync validation reports this pattern as WRITE_AFTER_WRITE.
  - Strict-reading secondary: `counter_buffer`, `blade_buffer` and `indirect_buffer` are single allocations shared by both FIF slots. The next frame's pre-scatter barrier's source stage is TRANSFER only, so it does not cover the previous submission's VERTEX / DRAW_INDIRECT reads or its readback copy.
  - That secondary gap is serialized host-side by the both-slot fence wait, but #4177/#4179 hold that a host wait is not a device edge.
  - The comment in `record_interaction` ("consecutive frames can overlap on the queue") is contradicted by that same both-slot wait.
- **Evidence**:
  - Only three production `cmd_fill_buffer` sites exist:
    - `compute.rs`: one fill of the per-FIF telemetry buffer, followed immediately by a buffer barrier.
    - `restir.rs` `zero_fill`: one fill per distinct buffer in a one-time submit.
    - `groundcover.rs`: the three overlapping fills above.
  - The `6db9eac2` commit message, recorded against a `BYRO_VALIDATION=1` run on a Skyrim SE terrain capture: "the same 10 `vkCmdFillBuffer` WAW hazards, all pre-existing and none of them mine".
  - Two intra-command-buffer WAW pairs per scatter frame fits that count. I did not re-run validation (engine launch is out of scope).
- **Impact**:
  - Rendering is unaffected.
  - Under the strict reading, if the zero fill lands last, the `atomicMin` extrema start at 0, so `GroundCoverStats::d_ground_min` and `view_dist_min` read 0. This is the telemetry-only class of #4181, and EXAL tuning reads it directly.
  - The standing validation noise can also hide a real new hazard.
- **Related**: #4181, #4177, #4179, #4182 (open)
- **Suggested Fix**: Needs `BYRO_VALIDATION=1` verification on an exterior with ground cover first. If these fills are confirmed as the source, a TRANSFER→TRANSFER memory barrier between the zero fill and the seed fills should clear the hazards. Verify by re-running validation, not by `cargo test`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **DROP**: If Vulkan objects change, teardown is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
