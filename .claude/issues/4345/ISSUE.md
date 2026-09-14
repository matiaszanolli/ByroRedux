# #4345 — TD2-003: Henyey-Greenstein duplicated in `clouds.glsl` and `volumetrics_inject.comp`; the new copy lacks #1021's g-clamp

**Labels**: low, shaders, renderer, terrain-exterior, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4345

- **Severity**: LOW · **Dimension**: 2
- **Location**: `crates/renderer/shaders/include/clouds.glsl:61-65`, `crates/renderer/shaders/volumetrics_inject.comp:1329-1343` · **Status**: NEW · **Age**: cloud copy `c379898fd` (09-13); clamp `8ac5b91cf` (#1021, 05-14) · **Effort**: small · **Kind**: tech-debt
- **Finding**: Divergent fix history (the inject copy clamps `g` to ±0.999 and uses `PI`; the cloud copy has neither, verified). Not promoted to MEDIUM because the cloud `g` is currently a compile-time constant ≤ `CLOUD_PHASE_G0/G1`.
- **Suggested Fix**: A guarded *include/phase_functions.glsl* (new) with the clamped HG, called from both; add it to `crates/renderer/build.rs`'s dependency list.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
