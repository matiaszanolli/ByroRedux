# #1868: SAFE-2026-07-03-01: Residual ~222 renderer unsafe blocks lack a SAFETY comment (continuation of #1644)

- **Severity**: MEDIUM
- **Labels**: `medium`, `safety`, `renderer`, `bug`
- **Source**: `docs/audits/AUDIT_SAFETY_2026-07-03.md` (SAFE-2026-07-03-01)
- **Dimension**: 4 — Unsafe-Block Discipline

## Location
`crates/renderer/src/` — worst files: `vulkan/composite.rs` (17), `vulkan/context/mod.rs` (16), `vulkan/context/helpers.rs` (16), `vulkan/texture.rs` (15), `vulkan/device.rs` (14), `vulkan/svgf.rs` (13), `vulkan/context/resize.rs` (13), plus 7 more.

## Description
545 non-test `unsafe {` block openers across `crates/`, 222 without a SAFETY comment. Same rolling gap as the closed #1644 (fixed 124 of ~327 originally) — no open issue tracks the remainder.

## Impact
Defense-in-depth/maintainability gap, not live UB — spot-checked blocks are all sound today.

## Suggested Fix
Resume the #1644 sweep: small fully-uncommented files first (`texture_registry.rs`, `context/screenshot.rs`, `egui_pass.rs`, `compute.rs`, `skin_compute.rs`), then the 4 large partially-commented files. Batch one SAFETY note per FFI cluster.
