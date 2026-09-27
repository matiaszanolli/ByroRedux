# EXT-D7-2026-09-27-01: #4730's contract guard cannot detect the *.sh.sh* regression it was written for

**Issue**: #4937
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,test-gap,tech-debt

**Severity**: LOW (guard quality; the workflow fix itself is correct)
**Dimension**: Acceptance gates and harness
**Tier Violated**: n/a
**Game Affected**: all
**Status**: NEW (incomplete fix of #4730)
**Location**:
`scripts/check-playable-smoke-contracts.sh` (the `LITERAL_GATES` scan added by `bb5c2c706`)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- The scan's regex `run_(declared_)?gate [a-zA-Z0-9-]+` stops at `.`.
- `run_gate m-exteriors.sh "$ext_game" static`, the exact line #4730 removed, therefore extracts `m-exteriors`. `docs/smoke-tests/m-exteriors.sh` exists, so the check passes.

## Evidence
Replayed in the main context: piping the pre-fix line through the scan's own `sed | grep -oE | sed` chain prints `m-exteriors`.

## Impact
The regression #4730 guards against would pass the contract lane again.

## Suggested Fix
- Capture the full token (`[^[:space:]"]+`), or assert that no gate token ends in `.sh`.
- Add a negative fixture line to the check's own self-test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **TESTS**: A regression test pins this specific fix
