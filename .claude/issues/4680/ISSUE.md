# CHAR-2026-09-21-D1-03: body_condition_base = 100 has no row in the CHARAL capture

**Severity**: LOW
**Dimension**: Ruleset Seam
**Game**: FO3 / FNV

## Description

The #4447 fix correctly moved the base-100 constant onto the profile. It left the capture document silent, even though that document is the stated authority for every CHARAL constant. A profile-row audit can verify every other FO3/FNV row against `charal-fnv-fo3-ruleset.md`, but not this one.

## Evidence

Verified at HEAD `ee6d3fb39`: `crates/core/src/character/profile.rs`'s `body_condition_base: Option<f32>` field doc cites "GECK Stats List" ("FO3/FNV seed the seven GECK limb-condition AVs at 100 (\"GECK Stats List\" — the AV names live in `consumables::BODY_CONDITION_VALUES`)"), and `FALLOUT3`/`FALLOUT_NEW_VEGAS` both set `body_condition_base: Some(100.0)`. `docs/engine/charal-fnv-fo3-ruleset.md` has no occurrence of "body condition" or of the seven AV names; the only citation for the constant is `docs/engine/playable-vertical-slice.md` (GECK Stats List, referenced from the vertical-slice log, not the capture).

## Impact

A future edit to the constant has no capture line to be checked against, which is the exact gap the no-guessing doctrine guards.

## Related

#4447, #4453 (the unsourced-profile-row class).

## Suggested Fix

Add a "Body-condition AVs (7) — base 100, GECK *Stats List*" row to the FNV/FO3 capture, naming `consumables::BODY_CONDITION_VALUES`.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D1-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
