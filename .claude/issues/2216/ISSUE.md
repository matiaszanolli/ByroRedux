# RT-2: skyrim_se + fo4 runtime baselines stale on entities_total and skin_pool_live

Severity: LOW · Labels: low, tech-debt, bug
Source: docs/audits/AUDIT_RUNTIME_2026-07-27.md

Filed from `docs/audits/AUDIT_RUNTIME_2026-07-27.md` (RT-2 / RT-3). Consolidates `AUDIT_RUNTIME_2026-07-25.md` RT-4 / RT-5 / RT-6 / RT-7, which were marked NEW in that report but never published as issues.

## Description

Two runtime baselines have drifted past their gate direction on two metrics each, on both skyrim_se and fo4. Both drifts are re-measured and stable across two independent sweeps (07-25 and 07-27) and two methodologies. **Neither looks like a defect** — but the guards now fail every sweep, which is how a real regression gets lost in the noise.

| Corpus | Metric | Baseline | Current | Gate |
|---|---|---|---|---|
| skyrim_se `WhiterunDragonsreach` | `entities_total` | 6044 | 6391 (+5.74%) | ±2% band |
| skyrim_se `WhiterunDragonsreach` | `skin_pool_live` | 0 | 25 | `<= baseline` |
| fo4 `InstituteBioScience` | `entities_total` | 11279 | 12448 (+10.36%) | ±2% band |
| fo4 `InstituteBioScience` | `skin_pool_live` | 100 | 124 | `<= baseline` |

## Why both are probably benign

**`entities_total`** — the documented benign-creep pattern (#1705): non-rendering bodies (collision-only colliders, ragdoll rigs, markers) drift up as collision/actor work lands. The render-load contract corroborates it directly: FO4 `bench_draws_cmds` moved +0.6% (3800 -> 3824) against a +10.36% entity rise, and Skyrim's *fell* 7% (2614 -> 2432). More bodies, no more rendering.

**`skin_pool_live`** — Skyrim going **0 -> 25** live slots is the shape of skinned meshes that previously failed to skin now succeeding. `22798ecc` ("fix skin version gates", 2026-07-25) is the obvious candidate and lands in the same window. Read that way it is coverage, not cost. `skin_pool_overflow_attempts` is still `0` and `skin_pool_max` still `1364` on every corpus, so there is no pressure on the #1284 cap either way.

## Impact

Low direct impact; real indirect impact. Four MEDIUM gate trips fire on every runtime sweep for settled, expected drift. That is exactly the noise floor in which #2215 (a genuinely live regression, closed green under an ineffective fix) is easy to miss.

## Suggested Fix

1. Reconcile each entity delta once against active-behavior component counts (`entities` filtered to `WanderState`/`TravelState`/ragdoll/collider components should sum to the delta). If it does not reconcile, that is a real leak and deserves its own issue.
2. Confirm the `skin_pool_live` rise attributes to `22798ecc`.
3. Then `--regen` both baselines.

**Blocked on #2215** — regenerating now would bake the live draw-split regression into the guard.

4. Consider re-gating `skin_pool_live` as a tolerance band (like `entities_total`) rather than a one-sided `<=`, since it moves with skinning *coverage*, not only cost. A one-sided gate structurally mis-reads a coverage fix as a regression.

## Completeness Checks
- [ ] **SIBLING**: The other three baselines (fnv / fo3 / oblivion) are checked for the same drift before regen
- [ ] **TELEMETRY**: `--regen` is run only after #2215 is resolved and re-verified, so the refreshed baseline encodes a known-good state
