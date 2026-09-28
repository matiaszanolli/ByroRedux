# #4977: REN-D12-2026-09-27-01: Debug-UI "GPU passes Σ" double-counts volumetrics — the new inject/integrate brackets are nested inside the outer volumetrics bracket

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4977
- **Labels**: low,renderer,tech-debt,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D12-2026-09-27-01**._

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/debug-ui/src/panels.rs:1263` (`gpu_total`); nesting at `crates/renderer/src/vulkan/context/post_passes.rs:829-839` (`record_volumetrics_pass`: `cmd_volumetrics_start` → `vol.dispatch(.., self.gpu_timers.as_mut(), ..)` → `cmd_volumetrics_end`) and `crates/renderer/src/vulkan/volumetrics.rs:1724-1802` (`cmd_volumetrics_inject_*`, `cmd_volumetrics_integrate_*` inside `dispatch`)
- **Status**: NEW (`88c23887b`)
- **Description**:
  - `88c23887b` added `volumetrics_inject` and `volumetrics_integrate` as child brackets inside the pre-existing `volumetrics` bracket.
  - `metrics_sample_system` publishes all three as sibling `gpu_pass_ms` rows.
  - The overlay sums every `Some` row into "GPU passes — Σ upper bound".
- **Evidence**: `let gpu_total: f32 = m.gpu_pass_ms.iter().filter_map(|(_, v)| *v).sum();`. The `volumetrics` row is ≥ `volumetrics_inject + volumetrics_integrate`, so the Σ adds the volumetrics cost roughly twice. `docs/engine/renderer.md` already states the rule for the geometry-phase children ("Do not add the children to their inclusive main-render parent"), and the geometry phases are correctly kept out of `gpu_pass_ms`. The volumetrics children were not.
- **Impact**:
  - The Σ is inflated by one full volumetrics cost on every frame with fog or volumetrics.
  - The hover caveat covers queue-drain overlap, not structural double-counting.
  - It misleads the CPU-Σ vs GPU-Σ comparison the panel exists for.
- **Related**: #2513 (Σ excludes inactive rows); `scripts/fsr_bench_report.py` `render_sum` uses a fixed key list and is unaffected.
- **Suggested Fix**: Exclude child brackets from the Σ. Either mark the children as sub-rows (`volumetrics.inject`), or skip names in a `CHILD_BRACKETS` list when summing. Add a unit test that the Σ over a synthetic snapshot with parent and children equals the parent-only sum.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
