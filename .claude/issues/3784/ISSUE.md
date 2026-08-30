# #3784: SF-D4-01: the Starfield Phase-0 baseline's unevaluated GBFM promote/defer rule now measures DEFER (2 of 2,465 unresolved) — and the real dominant class, PDCL at 74.9%, is unranked

**Labels**: documentation, low, legacy-compat, game:starfield, esm-plugin, doc-rot
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-30.md` — SF-D4-01 (LOW)
**Dimension**: 4 — ESM resolve-rate baseline
**Location**: `docs/engine/starfield-esm-phase0-baseline.md:134` (the GBFM promote/defer decision rule)

## Description

The Phase-0 baseline sets a decision rule that has never been evaluated:

> *"if the missing-form-id count is dominated by GBFM-targeted refs, Phase 3.5 promotes GBFM. If GBFM-targeted refs are <10% of skipped refs, defer."*

Measured for the first time this run. **The rule fires "defer" — decisively.**

## Evidence

Decomposition of every unresolved REFR in three Starfield cells:

| Cell | GBFM-targeted unresolved REFRs | Total unresolved |
|---|---:|---:|
| Cydonia | **2** | 2,465 (0.081%) |
| CydoniaMainLevel02 | 1 | 895 |
| Nishina01 | 0 | 4,218 |

The same measurement identifies what should take GBFM's place in the ranking: **PDCL, at 74.9% of unresolved REFRs** — roughly 900× more impactful by the identical metric, and not currently ranked anywhere.

Supporting context from the same decomposition: 92.9% of Cydonia's 2,465 unresolved REFRs are non-geometry *by design* (PDCL decals 74.9%, audio, actors, consciously skipped types). Only **175 of 27,898 REFRs (0.63%)** are genuinely missing placeable geometry — so Cydonia's effective geometry resolve rate is ~99.4%, not the headline 91.16%. Of those 175, the 144 model-less STAT/BNDS/ACTI/ARMO are exactly #1576's record list (BNDS 60 + STAT 44 + ACTI 33 + ARMO 7) and nothing else.

## Impact

Doc-only, but it closes a standing open question with a measurement and prevents the wrong prioritisation. As written, the baseline leaves GBFM promotion an open call that a future Phase-3.5 planner has to re-derive; and it leaves the actual dominant class (PDCL) unranked entirely. Anyone treating the 8.8% unresolved as a geometry gap is chasing the wrong 93% of it.

## Suggested Fix

1. Record the measurement at `:134`: GBFM-targeted refs are 0.081% of unresolved in Cydonia (2 of 2,465), 1 of 895 in CydoniaMainLevel02, 0 of 4,218 in Nishina01 → **rule fires DEFER, decision closed**.
2. Add PDCL to the ranking with its 74.9% figure, since the same rule applied to it would fire "promote".
3. Record the resolve-rate decomposition (92.9% non-geometry by design; 175 / 27,898 = 0.63% genuinely missing geometry) beside the 91.16% headline so the number is not misread as a geometry gap.

## Related

- #1576 — model-less STAT/BNDS/ACTI/ARMO drop (accounts for 144 of the 175)
- #3542 — shared CELL walker drops PGRE/PHZD (a separate, already-filed contributor)

## Completeness Checks
- [ ] **SIBLING**: Check whether `ROADMAP.md` or the `/audit-starfield` skill restate the 91.16% figure without the decomposition
- [ ] **TESTS**: N/A — documentation-only
