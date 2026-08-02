# REN-D17-02: pathEnvironmentRadiance feeds DALC irradiance into the path integrator as if it were radiance

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2244

**Dimension**: 17 (BRDF / GI)
**Location**: `crates/renderer/shaders/include/lighting.glsl:225` (`pathEnvironmentRadiance`, DALC branch at line 231-232: `return sampleDalcCube(rayDir);`)
**Status**: NEW

**Description**: Every other consumer of `sampleDalcCube` in this codebase divides the result by PI before using it as a radiance-like quantity (`triangle.frag:2196` — `sampleDalcCube(R) * (1.0 / PI)`), treating the DALC cube's stored values as irradiance. `pathEnvironmentRadiance`'s DALC branch returns `sampleDalcCube(rayDir)` raw, with no `1/PI` factor, feeding an irradiance value directly into the path tracer's environment-escape term as if it were already radiance.

**Evidence**: `triangle.frag:2196` (`sampleDalcCube(R) * (1.0 / PI)`) vs. `lighting.glsl:232` (`return sampleDalcCube(rayDir);` — no scaling) — the two consumers disagree on whether the DALC sample needs the `1/PI` irradiance→radiance conversion.

**Impact**: Bounced/GI paths that escape the TLAS into a Skyrim DALC-authored cell receive an indirect environment contribution roughly pi times too bright relative to the direct-lighting convention used elsewhere in the same shader.

**Suggested Fix**: apply the same `* (1.0 / PI)` conversion in `pathEnvironmentRadiance`'s DALC branch as is already applied at the other `sampleDalcCube` call site.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
