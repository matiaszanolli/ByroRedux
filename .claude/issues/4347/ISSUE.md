# #4347 — TD2-005: Rec. 709 luminance is written out ~20 times

**Labels**: low, shaders, renderer, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4347

- **Severity**: LOW · **Dimension**: 2
- **Location**: `crates/renderer/shaders/svgf_atrous.comp:72-74`, `crates/renderer/shaders/svgf_temporal.comp:60-62`, `crates/renderer/shaders/include/pbr.glsl:417-419`, `crates/renderer/shaders/volumetrics_inject.comp:281`, plus inline in `taa.comp`, `composite.frag`, `water.frag`, `presentation.frag`, `include/lighting.glsl` and `triangle.frag` (×6) · **Status**: NEW · **Effort**: small · **Kind**: tech-debt
- **Finding**: The inject shader claims agreement with `byroredux_core::radiometry::linear_srgb_luminance`, but every other consumer retypes the weights; the two svgf `luminance()` functions are byte-identical. All consumers already include the generated header.
- **Suggested Fix**: Emit `LUMA_REC709` from `crates/renderer/src/shader_constants_data.rs`, sourced from `crates/core/src/radiometry.rs`, and replace every copy.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
