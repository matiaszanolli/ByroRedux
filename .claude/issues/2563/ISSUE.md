# OBL-D1-02: Four more controllers miss their until=10.1.0.103 Data ref (and NiFlipController its Accum Time/Delta)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2563
**Finding ID**: OBL-D1-02

**Severity**: LOW
**Dimension**: NIF Version Handling
**Location**: `crates/nif/src/blocks/controller/shader.rs:55-79,179-203,211-232`, `crates/nif/src/blocks/controller/mod.rs:345-368,591-620`
**Status**: NEW

## Description
Same root cause as OBL-D1-01 (this session) in five sibling parsers (`NiTextureTransformController`, `NiMaterialColorController`, `NiLightColorController`, `NiFloatExtraDataController`, `NiFlipController`) — all route through `NiSingleInterpController::parse`'s version-gated `interpolator_ref` prologue with no complementary `until="10.1.0.103"` Data-ref read. `NiFlipController`'s own code comment additionally asserts "nothing to read here," which is wrong for this exact pre-10.1.0.104 band.

## Evidence
Zero vanilla Oblivion files at `version <= 10.1.0.103` reference any of these types today (latent, not live), but it is a real trap for mod/legacy NetImmerse content.

## Impact
Latent only — no vanilla content affected. A real trap for mod/legacy NetImmerse content authored below v10.1.0.104.

## Related
OBL-D1-01 (this session, same root cause).

## Suggested Fix
Fix alongside OBL-D1-01, same shape — give each sibling controller its own version-gated Data-ref read.

## Completeness Checks
- [ ] **SIBLING**: Fixed together with OBL-D1-01 since it's the identical root cause across 6 parsers
- [ ] **TESTS**: A synthetic byte-stream regression test at v10.1.0.103 pins the correct gating for at least one representative sibling
