# #7383 — REN-D9-2026-11-23-01: the #7150 `skin.coverage` counters (`dispatch_vertices`, `refit_triangles`) have no test, and `debug-cli.md` still shows the old output and points at the wrong file

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D9-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (test gap + doc rot; counters themselves are correct)
- **Dimension**: Skinning / Debug-Telemetry
- **Location**: `SkinChain` dispatch loop and `record_skinned_refits` (`context/skinned_blas_refit.rs`), `SkinCoverageFrame` (`skin_compute.rs`), `fill_skin_coverage_stats` (`context/telemetry.rs`), `SkinCoverageStats` (`crates/core/src/ecs/resources/mod.rs`), `SkinCoverageCommand` (`byroredux/src/commands/assets.rs`); doc `docs/engine/debug-cli.md` "### `skin.coverage`".
- **Status**: NEW
- **Description**: ce79a1213 added `dispatch_vertices += push.vertex_count` after `skin_pipeline.dispatch(...)` (skipped entities `continue` before it, so only recorded dispatches count) and `refit_triangles += idx_count / 3` in the `Ok(())` arm of `refit_skinned_blas` (first-sight builds are skipped by `built_this_frame`, so it is refit-only, as the field doc says). The counters reset with the rest of `SkinCoverageFrame::default()` at the top of `draw_frame`. All three behaviours are right, but nothing asserts them: `rg "dispatch_vertices|refit_triangles"` finds no test, and the 20 `coverage` tests cover only the pre-existing fields. The debug-cli.md sample output omits both new lines, and the same section still says the per-frame increments live in `context/draw.rs` (they moved to `skinned_blas_refit.rs` in the #1857 split).
- **Evidence**: skinned_blas_refit.rs dispatch loop (`self.last_skin_coverage_frame.dispatch_vertices += u64::from(push.vertex_count)` after `skin_pipeline.dispatch`); refit arm (`refit_triangles += u64::from(idx_count / 3)`); telemetry.rs `stats.dispatch_vertices = f.dispatch_vertices`.
- **Impact**: low. The two numbers are the denominators the TSD-4.0 per-vertex / per-triangle rates in `terrain-displacement.md` section 8.1 were computed from; a future refactor of the dispatch loop could drop or double an increment unnoticed. Docs mislead anyone following the pointer.
- **Related**: #7150 (TSD-4 spike), M29.3 closure doc.
- **Suggested Fix**: add one pure test that mirrors the skip/dispatch/refit counter updates (or lift the three increments into a small `SkinCoverageFrame` method and test it), extend the `fill_skin_coverage_stats` mapping assertions, and update the debug-cli.md sample and file pointer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
