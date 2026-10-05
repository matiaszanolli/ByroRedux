# #5256: FNV-2026-10-05-D6-01: FNV P0/P1 smoke entity floors (2500) sit above HEAD's Prospector population (~2,359) after #4813/#4814 — both gates should fail spuriously

Labels: medium,test-gap,bug,game:fnv,legacy-compat
Filed from: docs/audits/AUDIT_FNV_2026-10-05.md

**Source**: `docs/audits/AUDIT_FNV_2026-10-05.md` (FNV-2026-10-05-D6-01) · **Severity**: MEDIUM · **Dimension**: Smoke Gates · **Status**: NEW

## Description
- ROADMAP's bench-of-record paragraph records that `f87490826` (#4813/#4814/#4820, 2026-09-28 — Initially Disabled / Starts Dead withholding) took Prospector from 3,309 to **2,359 entities** (814 draws, 803 TLAS), "matching HEAD bit-for-bit".
- The P0 and P1 FNV gates read `entities=` from the source-cell `bench:` line and hard-fail below `2500` — a floor set against the 3,056 measured on 2026-08-27.
- `--bench-mode renderer-stepped` changes only timestep and camera (`bench.rs`), not spawning, so the gates load the same ~2,359 population — about 141 entities under the floor.
- `7d99ba7f0` retargeted P2 for the same #4813 content change and left P0/P1 alone; #5067 attributed the Prospector moves but did not touch the fixtures. The p5 (2026-10-01) and p2 (2026-10-02) gates of record ran after `f87490826` but check no floor. No P0/P1 FNV run is recorded since.

## Location
- `docs/smoke-tests/fixtures/fnv.env` — `P0_ENTITY_FLOOR=2500` ("Measured 3056 entities on this pose at 2026-08-27") and `P1_ENTITY_FLOOR=2500`
- Enforcement: `docs/smoke-tests/p0-door-interaction.sh` (`if (( entities < P0_ENTITY_FLOOR ))` → FAIL), `docs/smoke-tests/p1-character-traversal.sh` (`(( entities >= P1_ENTITY_FLOOR )) || fail "source cell populated only …"`)

## Evidence
```
fnv.env       P0_ENTITY_FLOOR=2500    P1_ENTITY_FLOOR=2500
ROADMAP.md:186-189  "… f87490826 … took it to today's 2359 entities / 814 draws / 803 TLAS,
                     matching HEAD bit-for-bit (f87490826^ reads 3309/904/893)"
p0-door-interaction.sh:173  if (( entities < P0_ENTITY_FLOOR )); then … FAIL
```
Predicted, not run — the audit suite forbids launching the engine.

## Impact
- `p0-door-interaction.sh fnv` and `p1-character-traversal.sh fnv` go red on a correct build; the red hides any real P0/P1 regression on the reference title behind a known-bad floor.
- `p5-door-transition fnv` shares P0's route but not its floor.
- A future reader either distrusts the gates or lowers the floor without measuring.

## Related
#5067, #4813, #4814, `7d99ba7f0` (the P2 retarget), #5237.

## Suggested Fix
- Run `p0-door-interaction.sh fnv` once to measure the source-cell count on the pose.
- Re-pin both floors under it with the fixture's usual margin and update the "Measured …" comment.
- Optionally have the fixture cite the ROADMAP figure it tracks.

## Completeness Checks
- [ ] **SIBLING**: Other per-game fixtures' `P0_ENTITY_FLOOR` / `P1_ENTITY_FLOOR` checked against their current measured populations (any content-withholding change since they were pinned)
- [ ] **TESTS**: The re-pinned floor is measured on HEAD and the measurement commit is cited in the fixture comment
