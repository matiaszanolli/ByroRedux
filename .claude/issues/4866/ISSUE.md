# #4866: REN-D12-2026-09-24-03: the ground-cover model tier's ray-query compute runs in no GPU timer bracket

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D12-2026-09-24-03**._

- **Severity**: LOW (telemetry blind spot; same grade as closed #4315 and open #4618).
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/groundcover_models.rs` — `GroundCoverModels::record`; call site `context/dispatch_skin_and_cluster.rs` `record_groundcover_models`, invoked from `draw.rs` `draw_frame`.
- **Status**: NEW (`aabd99a05` / #4413)
- **Description**: `record` issues three phase dispatches (PLACE / LAYOUT / EMIT), barriers and the stats copy, and `groundcover_models.comp` traces `rayQueryEXT` per candidate. It is recorded after the scatter bracket's END and before `cmd_main_render_start`. All 19 issued brackets (skin dispatch, skin palette, BLAS refit, TAA, main render, TLAS build, cluster cull, SVGF, composite, SSAO, bloom, caustic splat, volumetrics, upscale, presentation, depth-history copy, ground-cover bench, sky cube, ground-cover scatter) miss it.
- **Impact**: Its GPU cost is invisible to `bench:` / the debug-UI grid / `gpu_breakdown`, and its drain is absorbed into `main_render_ms` (the `TOP_OF_PIPE` START is written right after it), inflating the adaptive ray-budget controller input `max(main_render_ms, volumetrics_ms)` on exterior frames with authored cover. Same mechanism as open #4808.
- **Suggested Fix**: Add a 20th bracket (`QUERIES_PER_FRAME` 38 → 40, a `BIT_`, `_ms` / `_active`, `SkinCoverageStats`, `fill_skin_coverage_stats`, `gpu_breakdown`, `metrics_sample_system`, the bench line, `BENCH_GPU_KEYS`, `bench_gpu_inactive_token`, a doc-table row), together with #4618 so the wiring is done once. Every consumer is pinned by an existing test, so a partial wiring fails loudly.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

