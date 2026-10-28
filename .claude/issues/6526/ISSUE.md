# TD7-2026-10-27-01: The underwater god-ray march keeps its loop bounds and ~15 tuning literals inline in composite.frag

Labels: low, renderer, shaders, tech-debt, bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-27.md

# TD7-2026-10-27-01: The underwater god-ray march keeps its loop bounds and ~15 tuning literals inline in composite.frag

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` finding `TD7-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `crates/renderer/shaders/composite.frag` - `underwaterSurfaceFocus`, `underwaterSunVisibility`, `underwaterGodRays` (~:522-606); constants home `crates/renderer/src/shader_constants_data.rs`.

## Description
This is the one new shader block that the #6390-#6392 constant sweep did not reach (those moved the Fresnel, parallax-pass and radial-blur-tap ranges into `shader_constants_data.rs` and pinned them against inline literals). The god-ray block (067ce38ef, 296d21912, 99475136b) bypasses that file.
- **Budget literals**: `const int steps = 24` (the march, composite.frag:590) and `for (int i = 0; i < 6; ++i)` (occluder taps, :541). Together they are PERF-D5-01's 144 depth taps per submerged pixel; neither has a name.
- **Tuning literals**: Henyey-Greenstein `g = 0.6`; reach `900.0`; occluder slab `6.0 + 0.01*w` / `160.0`; tap fractions 0.15 / 0.4 / 0.75 and 60 / 180 / 420 BU; noise scales `/70`, `/37`; four drift speeds; `pow(ridge, 6.0) * 2.4`; `span * 1.5`; `k = 2.0 / span`; sun-elevation gates `0.05`.

## Evidence
`grep -n 'steps = 24\|i < 6' composite.frag` -> :541, :590; `grep GODRAY shader_constants_data.rs` -> no hits.

## Impact
A perf retune of the step count, or a future CPU mirror of the shaft occlusion, has no single home to change.

## Related
#6418 (IOR 1.333 in the same block), PERF-2026-10-27-D5-01 (cost), REN-D8-01/-02. Shader-struct/constant lockstep: see the `shader_constants` pin tests.

## Suggested Fix
Add `UNDERWATER_GODRAY_STEPS`, `UNDERWATER_GODRAY_OCCLUDER_TAPS`, `UNDERWATER_GODRAY_PHASE_G` and the reach/slab bounds to `shader_constants_data.rs`; keep the noise-shaping numbers as documented locals; fold #6418's IOR into the same change. Recompile `composite.frag.spv` with plain `-V` and re-run the shader-constants pin tests.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

