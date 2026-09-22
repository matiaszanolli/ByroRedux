# EXT-D5-2026-09-21-02: Flat water's visible surface motion runs along -scroll, upstream against the physics current and rapids foam

**Issue**: #4728
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: MEDIUM (visual; a canonical-contract divergence between consumers)
**Dimension**: Water translation (WATAL) contract × renderer Dim 8 consumption
**Tier Violated**: no-leak (the canonical meaning of `scroll_*` / `WaterFlow` is not honoured by one consumer)
**Game Affected**: all games with River/Rapids water; the wind term affects all water
**Location**:
- `crates/renderer/shaders/water.frag` (both `sampleScrollingNormal` branches: `+ scroll * time`)
- `:764-770` (waterfall branch, negates explicitly)
- `crates/renderer/shaders/water.vert:209-212` (wave-A `+t·ω`, wave-B `-t·ω`)
- `byroredux/src/env_translate.rs:911-918` (`scroll_a = flow·rate + …`)
- Contract: `crates/core/src/ecs/components/water.rs:254-258`, `:463`

## Description
A texture sampled at `T(x·s + v·t)` shows a feature moving at −v/s. Flat water sets `scroll_a ≈ +flow·rate`, so ripples and the 60%-weight wave-A crest travel upstream, opposite to floating-body drift and the rapids foam streaks (which travel downstream). The waterfall shader branch negates explicitly, citing the correct convention; the flat-water branch never applies it.

## Evidence
Source lines above, confirmed at HEAD `ee6d3fb39`. No guard pins pattern direction for the flat-water branches. Confirming the visible look needs a captured frame via `docs/smoke-tests/m-exteriors.sh` (water mode) or `docs/smoke-tests/w1-water-traversal.sh`.

## Impact
Every flowing water surface shows its main visible motion opposing the current that carries bodies and foam; most conspicuous on rapids where streaks and ripples visibly cross.

## Suggested Fix
Pick one convention and apply it everywhere: either subtract `scroll * time` in both flat-water branches and flip wave A's time sign (keeping the CPU mirror in `crates/physics/src/water.rs:378-383` in lockstep), or negate the synthesized flow/weather terms in the translate. Pin the chosen direction with a test.

## Related
EXT-D5-2026-09-21-01 (must land together), EXT-D3-2026-09-21-01 (shares the sign question for the wind term); shader half owned by `/audit-renderer` Dim 8

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D5-2026-09-21-02)
