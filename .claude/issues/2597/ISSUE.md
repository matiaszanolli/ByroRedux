# FO4-D4-01: bare (130..=139) FO4-DLC band check instead of named constants

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2597
**Finding ID**: FO4-D4-01

**Severity**: LOW
**Dimension**: 4 (NIF Parser)
**Location**: `crates/nif/src/blocks/shader.rs:1026`
**Status**: NEW

## Description
The FO4-DLC subsurface/rimlight/backlight gate uses a bare
`(130..=139).contains(&bsver)` range literal instead of the named
`FALLOUT4..FO4_DLC_UPPER`-style constants its sibling gates in the same file
use elsewhere.

## Evidence
```rust
// crates/nif/src/blocks/shader.rs:1026
let (subsurface_rolloff, rimlight_power, backlight_power) = if (130..=139).contains(&bsver)
```
Other BSVER-range gates in the same file reference named constants rather
than inlining the numeric band.

## Impact
Readability/maintainability only — the numeric band is correct today, but a
bare literal is one accidental typo away from silently drifting out of sync
with the named constants during a future edit, and gives no compile-time
signal if the FO4-DLC version band itself changes.

## Suggested Fix
Replace the bare `130..=139` with the same named-constant pattern
(`FALLOUT4..=FO4_DLC_UPPER` or equivalent) used by sibling gates in this
file.

## Completeness Checks
- [ ] **SIBLING**: Grep `shader.rs` for other bare BSVER numeric ranges while in this area
- [ ] **TESTS**: Existing BSVER-gated parse tests should continue to pass unchanged (behavior-preserving rename)
