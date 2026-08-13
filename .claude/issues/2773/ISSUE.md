# REN-D1-04: Stale acceleration.rs-monolith comments cite dead symbol tick_pending_destroy_blas and a rotted draw.rs:889 anchor

- **Severity**: LOW
- **Dimension**: 1
- **Labels**: low,renderer,documentation

## Description
Three live comments cite the pre-Session-35 `acceleration.rs` monolith; one names `tick_pending_destroy_blas` (no such symbol — it is `tick_deferred_destroy`), one uses a rotted `draw.rs:889` anchor now pointing at a DOF test.

## Location
`crates/renderer/src/deferred_destroy.rs`, `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/src/vulkan/context/draw.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D1-04).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2773
