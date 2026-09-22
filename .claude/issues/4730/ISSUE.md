# EXT-D7-2026-09-21-01: m-exteriors-static CI gate runs docs/smoke-tests/m-exteriors.sh.sh — path never exists

**Issue**: #4730
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: MEDIUM (the only automated lane for the cross-game exterior readiness matrix is dead on arrival)
**Dimension**: Acceptance gates and harness
**Game Affected**: all
**Location**: `.github/workflows/playable-smoke.yml:103` (`run_gate m-exteriors.sh "$ext_game" static`), `run_gate` at `:51-63`

## Description
`run_gate` appends `.sh` to build `docs/smoke-tests/$gate.sh`. The `m-exteriors-static` arm passes `m-exteriors.sh`, a name that already ends in `.sh`, so the step executes `docs/smoke-tests/m-exteriors.sh.sh`, which does not exist. The `w1-water-traversal` and `groundcover-eval` arms are wired correctly; only the exterior matrix is broken. `scripts/check-playable-smoke-contracts.sh` does not inspect workflow gate paths, so nothing caught it. Confirmed unchanged at HEAD `ee6d3fb39`.

## Evidence
Replayed the workflow's `run_gate` body locally: `bash: docs/smoke-tests/m-exteriors.sh.sh: No such file or directory`, status 127.

## Impact
The cross-game exterior matrix (WATR provenance, waterline delta, image health, #4491's sky pixel gate, #4508's chrome ceiling) never runs in CI.

## Suggested Fix
`run_gate m-exteriors "$ext_game" static`. Add a contract check asserting every workflow gate resolves to an existing script.

## Related
#4492 (closed; introduced this arm), #4491, #4508

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D7-2026-09-21-01)
