# REN-2026-09-11-D5-01: TextureRegistry::pending_destroy_count has zero callers, unlike its BLAS-side sibling fixed by #3999

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4117
**Labels**: bug, renderer, low, memory, tech-debt

**Severity**: LOW
**Dimension**: Memory/Lifecycle
**Location**: `crates/renderer/src/texture_registry/release.rs` (`pending_destroy_count` at line 163, `drain_pending_destroys`, `tick_deferred_destroy`)
**Status**: NEW (from `docs/audits/AUDIT_RENDERER_2026-09-11.md`)

## Description
The doc comment on `pending_destroy_count` claims it is "surfaced for the regression test and shutdown telemetry" — neither exists. This is the identical shape `REN-2026-09-06-D5-04` found and fixed (`#3999`) for `AccelerationManager`'s three deferred-destroy accessors (`blas_pending_destroy_count`, `scratch_pending_destroy_count`, etc., now wired into `RtIntegrityStats`) — that fix did not extend to this sibling subsystem (the texture registry).

## Evidence
Confirmed by direct read: `grep -rn "\.pending_destroy_count("` across the workspace returns zero call sites outside the accessor's own definition at `release.rs:163`. `RtIntegrityStats` (`crates/core/src/ecs/resources/mod.rs`) has `blas_pending_destroy_count`/`scratch_pending_destroy_count` fields (the BLAS-side sibling, already wired per `#3999`) but no texture-registry pending-destroy field, and nothing in `crates/renderer/src/vulkan/context/mod.rs`'s telemetry fill path calls the texture-registry accessor. The deferred-destroy bookkeeping itself (`drop_texture`/`drop_textures` → `tick_deferred_destroy` → `drain_pending_destroys` at shutdown) is sound; only the telemetry accessor is unread.

## Impact
No leak or correctness risk — pure observability gap. If `tick_deferred_destroy` ever stalled (e.g. a mis-set `current_frame_id`), there is no console/log surface to notice the texture-side deferred-destroy queue growing unbounded before it eventually shows up as VRAM pressure.

## Related
`#3999` / `REN-2026-09-06-D5-04` (the BLAS-side fix this subsystem was never brought in line with)

## Suggested Fix
Add a `textures_pending_destroy` row to the renderer's telemetry stats struct, populated from `TextureRegistry::pending_destroy_count` in the same fill path that already populates the BLAS-side rows, plus a shutdown-drain test in `texture_registry_tests.rs` mirroring the BLAS-side coverage — or delete the accessor and correct its doc comment. Given `#3999` already established the pattern, extending it is the smaller diff.

## Completeness Checks
- [ ] **SIBLING**: Wire identically to the already-fixed BLAS-side pattern (`#3999`) rather than inventing a new telemetry shape
- [ ] **TESTS**: A shutdown-drain regression test mirroring the BLAS-side coverage
