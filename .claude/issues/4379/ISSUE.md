# #4379 — TD7-006: The 8×8 blue-noise table's dimensions (`& 7`, `* 8`, `/ 64.0`) are re-typed at both consumers

**Labels**: low, shaders, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4379

- **Severity**: LOW · **Dimension**: 7
- **Location**: `crates/renderer/shaders/include/blue_noise.glsl:16`, `crates/renderer/shaders/composite.frag:286-288`, `crates/renderer/shaders/volumetrics_inject.comp:1375-1379` · **Status**: NEW · **Age**: composite consumer now drives the cloud step jitter (`564d0d2fe`, 09-13) · **Effort**: trivial · **Kind**: tech-debt
- **Suggested Fix**: Add `blueNoiseRankAt(ivec2)` derived from `BLUE_NOISE_RANKS.length()`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
