# #7386 — REN-D12-2026-11-23-01: `far_sun` is absent from the metrics/overlay GPU rows, and `gpu_breakdown` omits five brackets; no guard ties those readers to the bracket set

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D12-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `metrics_sample_system` (`byroredux/src/systems/metrics.rs`, the `gpu_pass_ms.insert` block); `gpu_breakdown` (`byroredux/src/systems/debug.rs`); contrast `bench_gpu_samples` / `BENCH_GPU_KEYS` (`byroredux/src/main.rs`)
- **Status**: NEW (same class as closed #2183, #2821, #3629, #6754 — a bracket added without every reader)
- **Description**: The checklist says a new bracket updates "the constant, the `GpuTimerSnapshot` `_ms` + `_active` fields and every reader together (`BENCH_GPU_KEYS`, `gpu_breakdown`, `metrics_sample_system`, ...)". At HEAD there are 30 `SkinCoverageStats` bracket pairs and 30 `BENCH_GPU_KEYS`. `metrics_sample_system` inserts 29 rows: `far_sun` is the missing one (`git log -S far_sun -- byroredux/src/systems crates/debug-ui` is empty: it was never wired). `gpu_breakdown` (the slow-frame / once-a-second `engine::stats` hitch line, whose doc calls `SkinCoverageStats` "the canonical landing pad for every bracket") prints 25 and omits `far_sun`, `cache_update`, `cache_resolve`, `terrain_patch` and `terrain_patch_blas`. `telemetry.rs::fill_skin_coverage_stats` (both the `Some` and the not-timer arm) and the bench rows are complete.
- **Evidence**: `rg far_sun byroredux/src/systems crates/debug-ui` returns nothing; `gpu_breakdown`'s format string ends `... volumetrics_local_sim={} shadow_filter={} cloud_shadow={} rt_tier=...`. The `bench:` line is the only reader with a parity test (`bench_gpu_keys_match_the_reported_bracket_order`, `every_bench_gpu_key_is_printed_on_the_bench_line`); `metrics.rs` has two unrelated tests and `debug.rs` none for this.
- **Impact**: The Studio/overlay "GPU passes" grid and its Σ (`gpu_pass_upper_bound_ms`) and the byro-dbg `gpu_pass_ms` wire never show the far-field sun pass (0.018 ms median per the Phase 3 evidence, so the Σ error is small today but the pass is what RLT-PERF-001 asks to be reported). A hitch on an exterior frame is not attributable to the terrain patch / patch BLAS / far-sun brackets from the log line (they are on the `terrain.patches` command and the bench line only). No rendering effect.
- **Related**: #2183, #2821, #3629, #6754 (closed, same class); `NESTED_GPU_BRACKETS` needs no change for any of the five (all are siblings, see Invariants).
- **Suggested Fix**: add the `far_sun` row (and the cache / terrain-patch rows to `gpu_breakdown`), then pin it once: a bin-crate test that every `BENCH_GPU_KEYS` entry appears as a `gpu_pass_ms` key in `metrics_sample_system` (production-text scan) or, better, drive both from one `[(key, ms, active); N]` table like `bench_gpu_samples`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
