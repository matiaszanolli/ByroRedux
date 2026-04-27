# NIF-D5-10: FO76 BSCollisionQueryProxyExtraData + NiPSysRotDampeningCtlr undispatched

URL: https://github.com/matiaszanolli/ByroRedux/issues/728
Labels: enhancement, nif-parser, low

---

## Severity: LOW (long-tail)

## Game Affected
FO76

## Location
- `crates/nif/src/blocks/mod.rs` — no dispatch arms

## Description
Two near-zero-volume FO76-specific types.
- `BSCollisionQueryProxyExtraData` (nif.xml line 8498) — collision-query-proxy metadata for FO76.
- `NiPSysRotDampeningCtlr` — damps particle-system rotation (e.g. spinning embers slowing down).

## Evidence
2026-04-26 corpus sweep:
- `SeventySix - Meshes.ba2` — 2 + 5 = 7 occurrences total

## Impact
Negligible — single-digit blocks total. Mostly a cleanliness metric.

## Suggested Fix
Add arms when the bigger FO76 gaps are addressed (e.g., bundle with #710 BSPositionData fix). ~15 LOC each.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-10)
- Ride-along: #710 (NIF-D5-03 BSPositionData — shares FO76 fix PR)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Other `NiExtraData` aliases at `blocks/mod.rs:360-379` follow same pattern; for the controller, see other `NiPSys*Ctlr` parsers
- [ ] **TESTS**: Byte-exact dispatch test for each type
