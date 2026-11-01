# #6531 tracking: Fire, smoke and explosions quality/performance pass
Sub-issues #6532 (P0) .. #6538 (P6). Plan: docs/engine/fire-volumetrics-plan.md; survey: docs/engine/fire-volumetrics-phase0.md.
Decisions (user, 2026-10-31): waive Phase 0 gate (loose --mesh explosion NIF acceptable); start Phase 1 (#6533);
new world-space grid is TRANSIENT (not saved, no FORMAT_MAJOR bump).
Ground rules: commits `Refs #N` (no closing keyword); combustion rates in crates/core/src/combustion.rs; run scripts/check-shader-artifacts.sh
before shader commits; report per-pass GPU ms vs baseline; sampling/jitter/denoise changes -> screenshots + ask Matias; no GpuMaterial/GpuInstance layout change without asking.
