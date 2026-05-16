# Issue #1119 — TD4 magic-number codegen batch

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-16.md` (Top 5 Medium #4)
**Severity**: LOW × 8
**Labels**: low, renderer, tech-debt
**Status**: PARTIAL — 5 of 8 items closed by commit 15ee3169

## Items

### Closed (15ee3169)

- ✅ **TD4-203** — `BLOOM_INTENSITY` mirrored + drift test
- ✅ **TD4-204** — `VOLUME_FAR` mirrored + drift test
- ✅ **TD4-205** — Water motion enum (CALM/RIVER/RAPIDS/WATERFALL) mirrored + drift test
- ✅ **TD4-206** — `DBG_*` bit flags × 10 mirrored + drift test
- ✅ **TD4-208** — `THREADS_PER_CLUSTER` mirrored + drift test

### Deferred

- ⏳ **TD4-201** — 32 `bsver()` bare integer compares (own refactor PR, mechanical sweep)
- ⏳ **TD4-202** — 142 `data.len() >= N` subrecord-size gates (very large; own refactor PR)
- ⏳ **TD4-207** — Caustic / SSAO / SVGF-temporal / TAA compute shaders still use bare `8`; blocked on shader-compile environment supporting `GL_GOOGLE_include_directive` for those 4 shaders

## Notes

- The codegen path (`shader_constants_data.rs` → `build.rs` → `shaders/include/shader_constants.glsl`) is now extended to cover floats, enums, and bit flags in addition to the prior integer-only constants.
- Drift tests live in `crates/renderer/src/shader_constants.rs::tests` and read shader source text directly (no SPIR-V compile needed).

Continue with: `/fix-issue 1119` (for TD4-201/202/207 batch) or split into separate issues.
