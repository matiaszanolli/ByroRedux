# Issue #1550: DIM3-02: parse_ctda has no game/length plumbing — silent length-gate trap

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: LOW · **Dimension**: 3 (ESM Coverage) — import-pipeline / tech-debt · **Status**: NEW

**Location**: `crates/plugin/src/esm/records/condition.rs:222-279`

## Description
`parse_ctda(&SubRecord) -> Option<Condition>` is the single decode point for all games yet takes no game/version context; a "wrong layout" is signalled only via a silent `None`. This is the structural reason OBL DIM3-01 hid — there is no warn or length-mismatch diagnostic. The XCLL decode already uses a `(game, len)` sanity-warn pattern that this path should mirror.

## Evidence
Static read of the function signature and the silent `data.len() < 28` early return.

## Impact
Defense-in-depth gap; future per-game CTDA layout drift will fail silently the same way.

## Related
DIM3-01 (the 24-byte Oblivion CTDA bug this trap concealed).

## Suggested Fix
Route the length→layout decision through the same `(game, len)` sanity-warn pattern XCLL uses, so an unexpected length logs rather than silently dropping.

## Completeness Checks
- [ ] **SIBLING**: Same silent-`None` diagnostic gap checked in the other per-game ESM sub-record decoders
- [ ] **TESTS**: Test asserts an unexpected CTDA length logs a warn rather than silently dropping
