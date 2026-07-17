# FNV-D8-01: Bench-of-record (613 commits stale) significantly undersells current interior performance

- **Severity**: MEDIUM
- **Labels**: medium, performance, documentation
- **Location**: `ROADMAP.md:16-31` (bench table), `ROADMAP.md:694` (`R6a-stale-15`)

## Description
Live re-run of the documented Prospector Saloon repro at current HEAD (`c3e09bb5`) vs. the recorded R6a-stale-14 baseline (`1c26bc25`, 2026-06-03) came in significantly faster than the current ROADMAP framing suggests:

| Metric | Baseline | Live | Δ | Pre-collider target |
|---|---|---|---|---|
| wall FPS | 76.2 | 149.1 | +95.7% | 161.4 |
| fence ms | 11.12 | 4.87 | -56.2% | 2.62 |
| entities | 3516 | 3626 | +3.1% | ~2564 |
| draws | 1224 | 1224 | flat | — |

FPS closed from 47% of the pre-collider target to 92%; fence dropped from 4.2x target to 1.9x. `R6a-stale-15` currently frames this as an open, uninvestigated gap — the magnitude of this single sample suggests most of the recovery already happened as a side effect of Session 46-56 work, not that the gap sits untouched.

## Impact
Documentation/tracking-accuracy only. Real process risk: could misdirect future effort toward re-diagnosing a mostly-closed gap instead of the smaller residual (fence 4.87→2.62ms, entity count 3626→~2564).

## Caveats
Single, uncontrolled sample — no repeat runs, shared GPU with a sibling audit's own bench later in the session (no overlap, but thermal/driver-cache confounds not excluded). Direction/magnitude argue against pure noise, but this is not asserted as a corrected baseline.

## Suggested Fix
Run `R6a-stale-15`'s formal 3-scene multi-sample re-bench and update `ROADMAP.md`; re-scope or close the "fence recovery uninvestigated" framing if confirmed.

## Completeness Checks
- [ ] **TESTS**: Formal 3-scene multi-sample re-bench recorded as the new baseline before closing
