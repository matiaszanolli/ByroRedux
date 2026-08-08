# PAT-D6-01: Skyrim+/FO4/FO76/Starfield RACE DATA sub-record is never decoded

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2455
**Finding ID**: PAT-D6-01 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 6 — Per-game translation-survey gaps
**Location**: `crates/plugin/src/esm/records/actor/mod.rs:1024-1057` (`parse_race`)
**Status**: NEW

## Description
`parse_race`'s `DATA` arm is gated `Oblivion | Fallout3NV` (fixed under #1629 to stop mis-decoding Skyrim's 128/164-byte layout with the 36-byte TES4/FO3/FNV field order), but no replacement arm was ever added for Skyrim LE/SE/FO4/FO76/Starfield — those fall through to `_ => {}`, so `skill_bonuses`/`base_height`/`base_weight`/`race_flags` stay at hardcoded defaults for every Skyrim+ RACE record. The gap is self-documented (line 1032 comment) but unresolved.

## Evidence
Confirmed zero production consumers anywhere in the tree for these fields on any game — currently dormant since nothing reads them yet, even on the correctly-parsed games.

## Impact
No visible behavior divergence today (dormant), but this will surface silently-wrong (uniform 1.0/1.0 height-weight, empty skill bonuses) the moment per-race scaling or skill-bonus application is wired up for Skyrim+ — with no error, no log, and no test coverage (existing RACE assertions are OBL-only).

## Related
#1629 (CLOSED — the follow-up half of this fix; that issue stopped the *wrong* decode, this is the deferred "add the *right* decode"). Independently reached by `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`'s TD6-001 (same gap, stub/placeholder lens, LOW) — this issue is the single filing for both; do not file TD6-001 separately.

## Suggested Fix
Add a `Skyrim | Fallout4 | Fallout76 | Starfield` arm decoding the TES5+ 128/164-byte layout. At minimum add a `log::debug!` note when a non-OBL/FO3NV RACE `DATA` is skipped, mirroring the `xcll_size_sanity_warn` pattern.

## Completeness Checks
- [ ] **TESTS**: A regression test decodes a real Skyrim+ RACE `DATA` sub-record and asserts non-default `skill_bonuses`/`base_height`/`base_weight`
- [ ] **SIBLING**: Confirm no other TES5+ record type has a similar dormant-gate gap left over from the same #1629 fix pass
