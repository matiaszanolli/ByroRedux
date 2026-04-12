# Issue #247 — PERF-04-11-L1

**Title**: TLAS always uses BUILD mode, REFIT never used for static frames
**Severity**: LOW
**Dimension**: GPU Pipeline / Ray Tracing
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/renderer/src/vulkan/acceleration.rs:375-505`

## Summary
TLAS rebuilt every frame with `BUILD` mode. Interior cells (~80% static) could use `REFIT` for 30-50% accel-structure speedup. Requires per-instance transform dirty tracking + `ALLOW_UPDATE` flag on creation.

## Fix with
`/fix-issue 247`
