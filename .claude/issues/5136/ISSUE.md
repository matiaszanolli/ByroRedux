# #5136: EXT-D5-2026-09-29-01: #4932's per-game wind-angle frame table in watal.md asserts the wind-FROM convention for FO3/FNV/FO76/Starfield without a census, and hides that Oblivion is still rotated by +90° (#4910)

**Labels**: low, documentation, doc-rot, water, terrain-exterior

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-09-29.md`
**Severity**: LOW
**Dimension**: Water translation (WATAL); tier violated: no-fabrication (documentation)

## Location
`docs/engine/watal.md` §2 "Wind-angle frame per game (#4932)"; `byroredux/src/env_translate.rs` `watr_angle_to_engine_xz` (+90° for every game)

## Description
- The new table lists FO3/FNV, FO76 and Starfield layers as the "same wind-FROM bearing" as Skyrim/FO4. The census that justified the +90° conversion covered only Skyrim and FO4.
- The Oblivion row says layer 0 is a direction-of-travel angle measured counter-clockwise, not a bearing. The code nevertheless applies the single +90° bearing rotation to it.
- The table reads as settled fact and never cites #4910, so a #4910 fix would contradict the canonical doc.

## Evidence
Census this pass (raw layer angle minus the engine-frame NAM0 heading `atan2(−y, x)`, circular mean, speed > 0 layers only):
- FO4: −88.5° (n = 107, R = 0.65, Rayleigh p ≈ 2e-20) — reproduces #4727.
- FO76: −77.5° (n = 138, R = 0.36, p ≈ 1e-8) — supports the convention, more weakly.
- Starfield: +50.3° (n = 36, R = 0.21, p ≈ 0.2) — **no** support.
- FO3/FNV: no NAM0, so this method cannot test them.

## Impact
The doc states an unverified frame as fact for three games and hides a known-open defect. No runtime change beyond #4910.

## Related
#4910 (open; the numbers above are new evidence for it — FO76 supported, Starfield not), #4932 (closed), #4727.

## Suggested Fix
Mark the FO3/FNV/FO76/Starfield rows with their evidence (FO76 supported; Starfield unsupported; FO3/FNV untestable by NAM0). Flag the Oblivion row as currently mis-rotated, citing #4910. Fix the rows together with #4910.

Validated at HEAD 9fcfdc3fc: watal.md table rows unchanged and uncited to #4910; `watr_angle_to_engine_xz` is still `beta + FRAC_PI_2` for every game.

