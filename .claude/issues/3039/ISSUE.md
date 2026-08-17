# FNV-2026-08-16-D8-01: playable-slice smoke gates are Skyrim-only; the reference title has none

**Issue**: #3039
**Severity**: MEDIUM
**Dimension**: 8 — Runtime gates
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_FNV_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 8 — Runtime gates).

**Location**: `docs/smoke-tests/p0-door-interaction.sh`:19 and the P1/P2 siblings

## Description

All three P0/P1/P2 gates for the project's active execution focus (the playable vertical slice) hard-code `SKYRIM_DATA` and skip when `Skyrim.esm` is absent.

**FNV — the project's declared reference title — has no playable-slice gate at all.** It has only a ragdoll gate (`docs/smoke-tests/m41-ragdoll.sh`:33, `FNV_DATA`).

## Evidence

Live probe on the FNV bench-of-record cell shows the slice is not merely ungated but **non-functional**:

```
byro> combat.approach 7
"combat.approach: entity 7 is not a damageable actor"
byro> input.press attack
"input.press: queued Attack through the R binding"
```

Re-verified 2026-08-17.

## Impact

Nothing exercises door interaction, character traversal or melee combat against FNV content — on the game the engine is calibrated against.

This is why #2986 (FO3/FNV actors get no `ActorValues`/`ActorVitals`) and #3004 (no Health term in the auto-calc) could both be true without any gate turning red. The probe output above is those two findings observed from the outside.

## Suggested Fix

Add FNV variants of the three playable-slice gates, parameterised on `FNV_DATA` — or better, parameterise the existing scripts by game so a new title costs a fixture rather than a script copy.

Note the gates will be RED for FNV until #2986 and #3004 land; that is the correct state, and worth landing the gate to make it visible.

## Related

- #2986 (ESM-D7-01), #3004 (RT-05) — the two findings this missing gate concealed
- #3003 (RT-04 — no CI runs any gate), #3001 (RT-02 — two gates already RED)

## Completeness Checks
- [ ] **PARAMETERISED**: The gate is game-parameterised rather than copy-pasted per title
- [ ] **HONEST-RED**: The FNV gate is allowed to fail until #2986/#3004 land, not weakened to pass
- [ ] **SKIP≠PASS**: Paired with #3003 so a data-less run is distinguishable
- [ ] **TESTS**: The FNV gate exercises door, traversal and melee as the Skyrim ones do

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3039 --json state` when live state is needed.*
