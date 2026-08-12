# CONC-D1-NEW-02: build_tlas commits BUILD-vs-UPDATE bookkeeping before the build is recorded

**Issue**: #2674
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: HIGH
- **Dimension**: 1 — Vulkan Queue & AS Sync
- **Location**: [tlas.rs](crates/renderer/src/vulkan/acceleration/tlas.rs)`:138-165` (commit) vs
  `tlas.rs:200` (`instance_buffer.write_mapped(..)?`) and `tlas.rs:320-363`
  (`built_primitive_count` + `cmd_build_acceleration_structures`), all in
  `AccelerationManager::build_tlas`
- **Status**: NEW
- **Description**: The three pieces of state that `decide_use_update` consults next frame are
  all committed **before** the build is recorded and before the one fallible call in between:
  1. `std::mem::swap(&mut tlas.last_blas_addresses, &mut current_addresses_scratch)` (line 138) —
     promotes this frame's BLAS address list to "the addresses the last BUILD used";
  2. `tlas.needs_full_rebuild = false` (line 164);
  3. `tlas.last_blas_map_gen = map_gen` (line 165).

  `tlas.instance_buffer.write_mapped(device, &instances)?` at line 200 can return `Err`, and
  `built_primitive_count` is only assigned at line 352, inside the BUILD arm that follows. So a
  failure at line 200 leaves the manager asserting "a BUILD landed at generation `map_gen` with
  address list X" when no build was recorded at all.

  Next frame `decide_use_update` sees `needs_full_rebuild == false`,
  `tlas_last_gen == current_gen`, and a zip-compare against the *promoted* cache that now
  matches — so it returns `use_update = true`. The single remaining guard is
  `if use_update && instance_count != tlas.built_primitive_count { use_update = false; }`
  (line 127), which only catches a **count** change. A frame in which the BLAS map generation
  bumped (cell load / eviction / skinned-BLAS rebuild — all four `blas_map_generation` bump
  sites) while the instance *count* stayed constant slips straight through, and the UPDATE is
  submitted with `acceleration_structure_reference` values that differ from those of the last
  real BUILD.
- **Evidence**:
  ```rust
  // tlas.rs:138-141 — cache promoted …
  std::mem::swap(&mut tlas.last_blas_addresses, &mut current_addresses_scratch);
  // tlas.rs:164-165 — … dirty flags cleared …
  tlas.needs_full_rebuild = false;
  tlas.last_blas_map_gen = map_gen;
  // tlas.rs:200 — … and only now the first fallible step
  tlas.instance_buffer.write_mapped(device, &instances)?;
  // tlas.rs:352 / 359 — count recorded and build actually emitted, far later
  tlas.built_primitive_count = instance_count;
  self.accel_loader.cmd_build_acceleration_structures(cmd, &[build_info], &[..]);
  ```
  The function's own doc (`tlas.rs:78-82`) states the invariant this breaks: "only the
  `acceleration_structure_reference` field is off-limits" across an UPDATE.
- **Impact**: A spec-violating TLAS UPDATE
  (`VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03707` class). The refit BVH keeps device
  addresses of BLAS entries that were replaced or evicted; evicted entries are freed
  `DEFAULT_COUNTDOWN` frames later by `tick_deferred_destroy`, at which point every shadow /
  reflection / GI ray traverses freed device memory. This is the "AS built at wrong address"
  severity row. Rated HIGH rather than CRITICAL only because reaching it requires the line-200
  host write to fail first (OOM / flush failure / near-device-lost); the *consequence* is
  CRITICAL-class.
- **Trigger Conditions**: Frame N takes the BUILD path because `blas_map_generation` changed
  (cell load, `evict_unused_blas`, `drop_blas`, `drop_skinned_blas`, or a skinned-BLAS rebuild)
  but `instances.len()` is unchanged from the previous successful BUILD; `write_mapped`'s flush
  fails on that frame. Frame N+1 then selects UPDATE. Deterministically reproducible by
  fault-injecting the `write_mapped` return.
- **Verification Path**: The ordering itself is checkable **in `cargo test`** with the repo's
  existing source-position pinning idiom (cf. `skin_built_this_frame_skip_tests` in
  [skinned_blas_refit.rs](crates/renderer/src/vulkan/context/skinned_blas_refit.rs)) — assert
  the three commit sites appear *after* `cmd_build_acceleration_structures`. The runtime
  consequence is **validation-layer-only** (`VUID-…-pInfos-03707` on the UPDATE call), not
  visible to `cargo test`.
- **Related**: CONC-D1-NEW-01 (same commit-before-success root cause); the #917 /
  REN-D10-NEW-03 fix in `draw.rs:3198-3216` (SVGF / TAA / volumetrics history counters advanced
  only after `queue_submit` returns `Ok`) is the established house pattern this site does not
  follow.
- **Suggested Fix**: Move the `mem::swap`, `needs_full_rebuild = false` and
  `last_blas_map_gen = map_gen` commits to immediately after `cmd_build_acceleration_structures`
  returns, alongside the existing `built_primitive_count` assignment — mirroring the post-submit
  history advance in `draw_frame`.

---

### MEDIUM


---
*Filed from [`docs/audits/AUDIT_CONCURRENCY_2026-08-12.md`](docs/audits/AUDIT_CONCURRENCY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `CONC-D1-NEW-02`.*

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other pipelines, other AS paths)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
