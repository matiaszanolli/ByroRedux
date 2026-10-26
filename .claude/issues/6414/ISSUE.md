# EXT-D3-2026-10-26-03: Model-tier yaw uses a bare 6.28318530718 next to `GROUNDCOVER_TWO_PI` in the same shader

Labels: low,terrain-exterior,shaders,tech-debt,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW / **Dimension**: 3 / **Tier Violated**: n/a
- **Game Affected**: all
- **Location**: `crates/renderer/shaders/groundcover_models.comp:649`
- **Status**: NEW (literal is pre-baseline; 1ca5032ae now uses `GROUNDCOVER_TWO_PI` ~30 lines away in `gcWindLean`, so the shader is inconsistent)
- **Description**: `float yaw = gcUnit(gcModelHash(h, 5u)) * 6.28318530718;` is the only tau literal left in the `groundcover_*` set; the skill rule treats a bare literal in `groundcover_*` GLSL as a finding.
- **Evidence**: `grep -n 6.2831 crates/renderer/shaders/groundcover_*.{vert,frag,comp}` -> only this line; `GROUNDCOVER_TWO_PI` is a generated define (`shader_constants_data.rs:798`).
- **Impact**: none functional (tau to 11 digits). Tech debt / drift risk only.
- **Suggested Fix**: replace with `GROUNDCOVER_TWO_PI`.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
