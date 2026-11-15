# #6983: TD7-2026-11-15-04: `bindings.glsl` declares the sky-SH array and its loop with a bare `9` while `SKY_SH_COEFFICIENTS` is already in scope

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6983
- **Labels**: low,shaders,terrain-exterior,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 7 (magic numbers; shader lockstep)
- **Location**: `crates/renderer/shaders/include/bindings.glsl:283` (`vec4 skyDiffuseCoefficients[9];`) and `:291` (`for (uint i = 0u; i < 9u; ++i)`)
- **Age**: SKYAL sky SH (pre-window)
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW
- **Description**: `SKY_SH_COEFFICIENTS` (`shader_constants_data.rs:77`, emitted as `#define SKY_SH_COEFFICIENTS 9`, `shader_constants.glsl:278`) is the named count. `sky_irradiance.comp:9` uses it, and a source-scan test pins the producer (`shader_constants.rs:1188`, `:1215-1235`: the SSBO size, the dispatch count and the `sky_sh.glsl` case count). The consumer declaration and loop in `bindings.glsl` are outside that test, and `bindings.glsl` documents that it depends on `shader_constants.glsl`. The SSBO the consumer reads is sized by the Rust constant (`irradiance.rs`: `SKY_SH_COEFFICIENTS as u64 * 16`), so the consumer's `[9]` is a second copy of a lockstep number.
- **Impact**: the L2 SH count is mathematically fixed at 9, so a change is unlikely. If it ever changed, the consumer would read past the SSBO or ignore coefficients with no failing test, which is why this is listed at all.
- **Related**: #5620 (closed: the same constants for the compute passes); `docs/engine/shader-pipeline.md`.
- **Suggested Fix**: `vec4 skyDiffuseCoefficients[SKY_SH_COEFFICIENTS];` and `i < uint(SKY_SH_COEFFICIENTS)`; extend the `shader_constants.rs:1188` check to assert `bindings.glsl` contains the named form.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
