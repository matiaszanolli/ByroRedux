# CHAR-2026-11-07-L-10: docs/feature-matrix.md rates FO3/FNV VATS '~ plumbing'

**Issue**: #6769
**Labels**: documentation, low, game:fnv, game:fo3, character, doc-rot
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-10, Dim 5). Game: FO3, FNV.

## Description
`docs/feature-matrix.md` rates the FO3/FNV VATS runtime "~ plumbing". The runtime is a working console-driven VATS: mode machine with world freeze, targeting scan, AP-gated queue and confirm, skill pricing, Concentrated Fire, damage through the combat funnel, and exits on cell change, death and load. What is absent is the menu, per-limb damage, playback and camera. The row predates all of that (last edited `870722b99`, 2026-10-26); `ROADMAP.md:306` is accurate.

## Location
`docs/feature-matrix.md:284` ("VATS + Action Points runtime (#6350, open) | ✗ | ~ plumbing | ~ plumbing | ✗ | ✗ | ✗ | ✗") and the prose at `:324-329`.

## Evidence
`byroredux/src/systems/vats.rs` (`vats_mode_system`, `vats_scan`, `vats_queue_part`, `vats_confirm`, `vats_cancel`), `docs/smoke-tests/vats-targeting.sh`, `6186ab77e` (#6625).

## Impact
The status floor understates shipped behaviour and omits what is actually missing.

## Related
#6350, #6376 (added the row), `ROADMAP.md:306`.

## Suggested Fix
"~ console-driven, no menu / limb damage / playback" for FO3 and FNV.

## Completeness Checks
- [ ] **SIBLING**: grep the other capture docs, `charal.md`, `feature-matrix.md`, `ROADMAP.md` and code comments for the same stale claim
- [ ] Cite symbols, not line ranges, in the corrected prose
