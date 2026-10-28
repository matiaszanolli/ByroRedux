# FNV-2026-10-27-03: three FNV live gates are missing from the smoke contract checker, and m42-package-procedures.sh is missing from the smoke index

Labels: low, tech-debt, test-gap, game:fnv, legacy-compat, bug
Source: docs/audits/AUDIT_FNV_2026-10-27.md

**Source**: `docs/audits/AUDIT_FNV_2026-10-27.md` finding `FNV-2026-10-27-03` (Dimension 6, gates; owner `/audit-tooling` / audit infrastructure) · Severity LOW (test-gap) · Status NEW (third recurrence of the class; #5427 and #6057 are closed) · Game: FNV.

**Location**: `scripts/check-playable-smoke-contracts.sh` (the loop at ~:119 lists `dt1-dialogue-layers m42-eat-sleep sm1-story-manager dt2-skyrim-forcegreet m47-3-obscript fo4-ragdoll p4-fnv-quest-route p5-fnv-quest-persistence`; no mention of `m42-package-procedures`, `vats-targeting` or `w2-dynamic-water`); `docs/smoke-tests/README.md` (no `m42-package-procedures` row; `vats-targeting.sh` and `w2-dynamic-water.sh` are indexed).

## Description
`f2aa5c5da` (m42-package-procedures), `59d69f763` (vats-targeting) and `288ea59c4` (w2-dynamic-water) each added an FNV gate that exits 77 on missing data. None of the three joined the contract checker's loops, so the SKIP != PASS contract is unenforced for them. CLAUDE.md calls the smoke-test README "the one list to trust", yet it omits the M42 package-procedures gate.

## Evidence
`ls docs/smoke-tests` shows `m42-package-procedures.sh`, `vats-targeting.sh`, `w2-dynamic-water.sh`. `grep -n "m42-package\|vats-targeting\|w2-dynamic" scripts/check-playable-smoke-contracts.sh` returns nothing; the README has rows for `vats-targeting` (line 61) and `w2-dynamic-water` (line 301) but none for `m42-package-procedures` (only `m42-eat-sleep`, line 62).

## Impact
A dataless runner could report these three gates as a pass or a hard failure instead of the contractual SKIP (exit 77), and nothing in CI would notice. The index is incomplete.

## Related
#5427, #6057 (earlier recurrences), #4724.

## Suggested Fix
Add the three gates to the checker loops and the M42 gate to the README. Consider a checker self-test that fails when a `docs/smoke-tests/*.sh` is in neither the loops nor an explicit exemption list, which would end the recurrence.

## Completeness Checks
- [ ] **SIBLING**: Every `docs/smoke-tests/*.sh` cross-checked against both the checker loops and the README index
- [ ] **TESTS**: The proposed self-test fails when a new gate script is added without a loop entry or exemption

