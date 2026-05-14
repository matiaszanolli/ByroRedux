# Issue #1030

**Title**: REN-D10-NEW-09/10: Composite + SVGF descriptor pool sizes lag layout binding counts

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D10-NEW-09 / REN-D10-NEW-10
**Severity**: LOW (latent / startup-only assert risk)
**Files**: `crates/renderer/src/vulkan/composite.rs:573-579` ; `crates/renderer/src/vulkan/svgf.rs:490-497`

## Issue

Pool size declarations for `combined_image_sampler` (composite, 7 samplers) and SVGF (8 samplers + 2 storage + 1 UBO) currently match the layouts on inspection — but the count is hand-derived from layout bindings rather than computed from the layout itself. Adding a binding to the layout without bumping the pool size produces a startup pool-allocation failure.

## Fix

Either: compute pool sizes by summing layout-binding descriptor counts, OR add an assert that pool sizes match layout-binding sums at startup. Self-derived sizes prevent silent drift.

## Completeness Checks
- [ ] **SIBLING**: Apply same pattern to all subsystem pool declarations (caustic, taa, bloom, volumetrics, etc.)
- [ ] **TESTS**: Compile-time assert that pool sizes are derived from layout binding counts

