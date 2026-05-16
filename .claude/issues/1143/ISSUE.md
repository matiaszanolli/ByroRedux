# PERF-D1-NEW-01: volumetric dispatch gate is a runtime if on a host const

**GitHub**: #1143
**Severity**: LOW
**Audit**: AUDIT_PERFORMANCE_2026-05-16.md
**Status**: CONFIRMED

## Location
- `crates/renderer/src/vulkan/context/draw.rs:1410, 2191`
- `crates/renderer/src/vulkan/volumetrics.rs:124` (VOLUMETRIC_OUTPUT_CONSUMED = false)

## Summary
Two call sites in draw.rs check `VOLUMETRIC_OUTPUT_CONSUMED` as a runtime `if` even though
it's a `const bool`. LLVM may or may not DCE the enclosing if-let chain depending on optimizer
heuristics. Follow-up to 2026-05-10 PERF-GP-01 which added the const gate.

## Fix
Replace with `#[cfg(feature = "volumetrics")]` so the dispatch site is compile-time removed.
~5 LOC. Verify DCE under RenderDoc/Nsight before fixing if impact is unclear.
Related: #928 (future flip to enable volumetrics).
