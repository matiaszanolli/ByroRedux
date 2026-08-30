# #3629 — REN-2026-08-30-D20-04: the `bench:` line reports 12 of the 14 GPU brackets, and its `tlas_ms=` is the host-side number

**Labels**: `low,renderer,performance,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3629 --json state`.

---

- **Severity**: Low
- **Dimension**: Debug/Telemetry
- **Location**: `byroredux/src/app_events.rs:888-926` (bench GPU array), `byroredux/src/main.rs:91-104` (`BENCH_GPU_KEYS`)
- **Status**: Open
- **Description**: `gpu_timers.rs` owns 14 brackets (`QUERIES_PER_FRAME == 28`, `BIT_SKIN_DISPATCH … BIT_PRESENTATION`), and every other consumer surfaces all 14 — `systems/metrics.rs:143-195` (debug-UI grid), `systems/debug.rs::gpu_breakdown`, `context/mod.rs:2234-2266` (the `SkinCoverageStats` fill). The bench summary copies only 12: `gpu_tlas_build_ms` and `gpu_caustic_splat_ms` are absent from both the value array and `BENCH_GPU_KEYS`, despite the comment above the array claiming "Full per-pass GPU breakdown."
- **Evidence**:
  - `app_events.rs:894-908` — the 12-element array: `skin_dispatch, skin_blas_refit, taa, upscale, main_render, svgf, composite, ssao, bloom, volumetrics, cluster_cull, presentation`. No `gpu_tlas_build_ms`, no `gpu_caustic_splat_ms`.
  - `main.rs:91` `const BENCH_GPU_KEYS: [&str; 12]` and `main.rs:116 fn bench_gpu_inactive_token(active: [bool; 12])` — so the `gpu_inactive=` token cannot name either missing bracket either.
  - `app_events.rs:850`: `let tlas_ms = ft.tlas_build_ns as f64 / n / 1e6;` — `FrameTimings`, i.e. the **host** TLAS build cost. It is printed inside the CPU group `[fence= … tlas_ms= … submit=]`, but `gpu_tlas_build_ms` never appears anywhere on the line, so `tlas_ms=` is the only `tlas` token a sweep harness can extract.
  - `main.rs:205-211` (`bench_gpu_keys_match_the_reported_bracket_order`) asserts `len() == 12` and spot-checks indices 4 and 11 — it pins the current 12 rather than catching the gap.
- **Impact**: The FSR benchmark matrix and the four sweep harnesses that parse this line have no device-side TLAS-build or caustic-splat number. `gpu_timers.rs`'s own doc calls out first-cell-load TLAS spikes as the thing the bracket exists to catch, and the caustic bracket is one of the five Phase-7 brackets added specifically to close the "438 ms unaccounted" gap — neither is measurable from a bench run. The `tlas_ms=`/`gpu_tlas_build_ms` name collision invites reading the host number as the device one.
- **Suggested Fix**: Widen the array, `BENCH_GPU_KEYS`, and `bench_gpu_inactive_token`'s parameter to 14, appending `tlas_build` and `caustic_splat` as `gpu_tlas_build=` / `gpu_caustic_splat=` (append, so existing `key=<float>` extractors keep matching), and raise the `len()` assertion to 14. Consider renaming the CPU token to `cpu_tlas_ms=` in the same pass, or leave it and note the distinction in the comment.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D20-04

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
