# TD2-108: NiSingleInterpController prologue reimplemented inline at 4 sites instead of calling NiSingleInterpController::parse

**GitHub Issue**: #2067
**Labels**: low,nif-parser,animation,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: canonical `controller/mod.rs:253-267` (`NiSingleInterpController::parse`); duplicated at `controller/shader.rs:56-63,180-186,212-219`, `controller/mod.rs:594-600`

## Description
Two family siblings (`controller/mod.rs:298,347,557` and `controller/shader.rs:107,139`) correctly call `NiSingleInterpController::parse(stream)?`; 4 others (`NiLightColorController`, `NiMaterialColorController`, `NiTextureTransformController` in `shader.rs`, and `NiFloatExtraDataController` in `mod.rs`) re-type the identical `parse_interp_controller_base(stream)? ` + conditional `interpolator_ref` prologue inline instead.

## Evidence
Confirmed live: `controller/mod.rs:253` defines `NiSingleInterpController::parse` wrapping `parse_interp_controller_base` + the version-gated `interpolator_ref` read. Confirmed the 4 named controllers each re-implement that exact 8-line prologue verbatim rather than calling the wrapper, while `controller/mod.rs:298,347,557` and `controller/shader.rs:107,139` already call it correctly.

## Suggested Fix
Call `NiSingleInterpController::parse(stream)?` and destructure at each of the 4 sites.

**Effort**: small

## Completeness Checks
- [ ] **SIBLING**: 2 of 6 sibling controller parsers already use the shared helper — the fix brings the remaining 4 in line, no new pattern introduced
- [ ] **TESTS**: Existing per-controller parse tests (`controller/tests.rs`) cover all 4 sites — purely mechanical swap
