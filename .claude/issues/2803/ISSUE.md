# REN-D9-2026-08-12-04: _skin_chain_ns is dead telemetry, now load-bearing only as a test source-position anchor

- **Severity**: LOW
- **Dimension**: 9
- **Labels**: low,renderer,tech-debt,bug

## Description
`_skin_chain_ns` is dead telemetry — measured every frame since M29 (`1ae235b9`), never consumed anywhere. Now load-bearing as a **source-position anchor** for the #2494 regression test, so naive deletion breaks `skin_eviction_runs_without_global_vertex_buffer_tests`. The CPU-side skin-chain wall time is genuinely unmeasured.

## Location
`crates/renderer/src/vulkan/context/skinned_blas_refit.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D9-2026-08-12-04).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2803
