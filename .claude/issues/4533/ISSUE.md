# REN-D8-2026-09-20-02: normalize_fog_tint's 'black remains black absorption' doc + test contradict 3ce970a5a's shader branch — zero-tint + extinction now scatters neutrally; no test ties the GLSL side

- **ID**: REN-D8-2026-09-20-02
- **Labels**: low,renderer,documentation,doc-rot,test-gap
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Volumetrics
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D8-2026-09-20-02)

**Location**: `byroredux/src/env_translate.rs` (`normalize_fog_tint`) + `black_or_invalid_fog_tint_stays_finite_and_absorptive` test vs the shader branch from `3ce970a5a`

**Description**
The CPU-side doc/test say a black fog tint stays purely absorptive; the shader (since 3ce970a5a) scatters neutrally for zero-tint + extinction>0, ungated on interior. The CPU and GLSL sides of one behavior are pinned and documented against each other.

**Evidence**
Audit D8, 2026-09-20.

**Impact**
Silent-regression vector for interior dust shafts: a future 'fix' to either side 'restoring' consistency breaks the other.

**Suggested Fix**
Decide the intended semantics, align doc + test + shader, and add a GLSL-side tie.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
