# SF-D4-01: sf_smoke's hand-maintained DISPATCH_HANDLED_FOURCCS byte-coverage list has drifted again — OMOD, LVSP, SCEN are real dispatch arms it still reports as skip

**Issue**: #4278 — https://github.com/matiaszanolli/ByroRedux/issues/4278
**Labels**: medium,esm-plugin,tech-debt,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 4 — Starfield ESM Resolve-Rate Baseline
**Location**: `byroredux/src/commands (or debug-server) sf_smoke DISPATCH_HANDLED_FOURCCS list`
**Status**: NEW

## Description
`sf_smoke`'s hand-maintained `DISPATCH_HANDLED_FOURCCS` list — used to report which record FourCCs the diagnostic considers "handled" vs. "skip" — has drifted from the actual dispatch table again: `OMOD`, `LVSP`, and `SCEN` are real, live dispatch arms in the ESM parser, but the hardcoded list still reports them as "skip". This is the same recurring drift class a prior audit found for `LCTN`.

## Evidence
Live-verified during this audit with a real engine build against vanilla `Starfield.esm`: `--sf-smoke citycydoniamainlevel` resolved 25,433/27,898 REFRs = 91.2% (matching the documented Phase 1 baseline, no regression); the `OMOD`/`LVSP`/`SCEN` dispatch arms are confirmed present and functioning in the live binary despite the diagnostic's hardcoded list saying otherwise.

## Impact
Cosmetic only — no REFRs are actually mis-resolved; the resolve-rate baseline itself is correct and unregressed. The impact is purely that `sf_smoke`'s own coverage report misleads a developer reading it about which record types are handled.

## Related
Same recurring drift class as the prior LCTN drift finding (referenced in this audit but not separately numbered as an open issue).

## Suggested Fix
Regenerate or mechanically derive `DISPATCH_HANDLED_FOURCCS` from the actual dispatch table (rather than hand-maintaining a parallel list) so it cannot drift again, or add a compile-time/test-time assertion that the two stay in sync.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
