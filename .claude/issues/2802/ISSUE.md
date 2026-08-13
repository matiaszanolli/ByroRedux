# REN-D9-2026-08-12-03: record_skinned_blas_refit logs two WARNs per entity per frame and retries a build that cannot succeed

- **Severity**: LOW
- **Dimension**: 9
- **Labels**: low,renderer,performance,bug

## Description
`failed_skin_slots` (#900) gates slot *allocation* only; a failed `build_skinned_blas_batched_on_cmd` records nothing, so the entity re-runs the size query + allocation every frame and logs **two WARNs per entity per frame** (build-failure + the refit that then cannot find the BLAS), while `refits_attempted` counts attempts that could never succeed. Fires precisely under the VRAM pressure that caused it.

## Location
`crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (`record_skinned_blas_refit`)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D9-2026-08-12-03).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2802
