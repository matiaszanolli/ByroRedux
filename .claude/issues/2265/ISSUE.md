# TD7-001: MAX_TRANSPARENT_SKIPS / MAX_OPAQUE_LAYERS — the same 8-layer ray-walk budget is hand-declared independently in three GLSL files

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2265

**Dimension**: 7 (Magic Numbers & Hardcoded Constants)
**Location**: `crates/renderer/shaders/include/raytrace.glsl:64`, `crates/renderer/shaders/water.frag:252`, `crates/renderer/shaders/include/shadow_transport.glsl:11`
**Status**: NEW

**Description**: Three separate GLSL functions each declare `const int MAX_TRANSPARENT_SKIPS = 8;` (raytrace.glsl's `traceReflection`, water.frag's alpha-cutout skip walk) or the differently-named but semantically identical `const int MAX_OPAQUE_LAYERS = 8;` (shadow_transport.glsl's `traceShadowTransmittance`, added 2026-08-01 by the shadow-policy refactor `1fb79038`). All three bound the same shape of work — a ray-query loop that walks past alpha-tested/uncovered geometry with an 8-layer safety cap — yet the value is redeclared by hand at every call site rather than sourced from `crates/renderer/src/shader_constants_data.rs` (the project's documented single-source-of-truth for values shared between Rust and GLSL). Two of the three sites are new since the 07-25 audit; the pattern is regrowing rather than converging — Session 62's new shadow-transport module chose to duplicate the 8-cap under yet another name rather than reuse the existing one.

**Evidence**:
```
crates/renderer/shaders/include/raytrace.glsl:64:    const int MAX_TRANSPARENT_SKIPS = 8;
crates/renderer/shaders/water.frag:252:              const int MAX_TRANSPARENT_SKIPS = 8;
crates/renderer/shaders/include/shadow_transport.glsl:11:  const int MAX_OPAQUE_LAYERS = 8;
```

**Impact**: Cosmetic/maintainability today — all three values are still numerically identical (8), so there is no live drift. But this is exactly the shape of bypass the shader-constants provenance rule exists to prevent: a future tuning pass has three independent call sites to find and update by hand, with no compiler or test tripwire if one is missed.

**Related**: Distinct from Existing #2229 (REN-D3-02, the `FOG_VOLUME_CLUSTER_DIM`/`MAX_FOG_VOLUMES_PER_CLUSTER` Rust-vs-GLSL bypass) — this is a GLSL-vs-GLSL intra-shader duplication with no Rust side at all.

**Suggested Fix**: Add `pub const MAX_ALPHA_SKIP_LAYERS: u32 = 8;` (or similar) to `shader_constants_data.rs`, `#include "include/shader_constants.glsl"` at all three call sites, and replace all three local declarations with the shared name.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
