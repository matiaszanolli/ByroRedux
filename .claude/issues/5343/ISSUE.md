# #5343: EXT-D1-2026-10-05-01: #5169 left FO76's pigment lanes 0–2 on Starfield's 0..20 scale — a test pins FO76 pigments equal to Starfield's, and watal.md records no OPEN item

**Labels**: low,terrain-exterior,water,game:fo76,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5343

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-05.md` — `EXT-D1-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW (latent while FO76 is parse-only)
- **Dimension**: EXAL boundary discipline (unit/semantic completeness at the WATAL translate)
- **Location**:
  - `byroredux/src/env_translate.rs:914-919` (pigment lanes ÷ `STARFIELD_WATER_CONCENTRATION_REFERENCE` for every game); the test is at `:3564` (`assert_eq!(fo76.concentration[..3], sf.concentration[..3])`).
  - `docs/engine/watal.md:356-360`.
- **Status**: NEW. It is the second suggested-fix bullet of baseline EXT-D1-2026-10-02-01 / #5169 (closed), which the fix did not take up.
- **Tier Violated**: no-fabrication (Starfield's semantic applied to an uncensused FO76 lane)
- **Game Affected**: Fallout 76
- **Description**:
  - #5169 established that FO76 lane 3 is "a different quantity whose meaning is unestablished", and gated it to the zero sentinel.
  - The same census (baseline) found FO76's lanes 16/20/24 authored at 9e-5–0.52, not Starfield's 0–20. They still go through `/20` and reach `water.frag`'s pigment term as 0–0.026, applying Starfield's meaning at an unmeasured scale.
  - The new test asserts the FO76 and Starfield results are identical on lanes 0–2, which locks this in.
- **Impact**: An unvalidated FO76 pigment contribution, near-zero today. A future FO76 water pass would inherit it as if it had been decided.
- **Suggested Fix**:
  - Either census FO76 lanes 16–24 and give them a meaning, or gate them like lane 3 (zero sentinel for FO76) through the same table-shaped predicate.
  - Record the decision as OPEN in watal.md, and change the test to assert the chosen FO76 rule rather than equality with Starfield.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
