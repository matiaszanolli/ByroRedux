# SF-2026-09-11-D9-04: the #[must_use] MergeOutcome is let-_-'d at all four production call sites — the PresenceOnly signal #2709 created has no telemetry sink on Starfield ~100% of materials

**Issue**: #4289 — https://github.com/matiaszanolli/ByroRedux/issues/4289
**Labels**: low,nifal,tech-debt,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 9 — BGSM/BGEM External Material Flow
**Location**: `byroredux/src/asset_provider/material/merge.rs and byroredux/src/cell_loader/refr.rs — the four production call sites of merge functions returning MergeOutcome`
**Status**: NEW

## Description
`MergeOutcome` is marked `#[must_use]`, but all four of its production call sites discard the return value with `let _ = ...`. `#2709` introduced `MergeOutcome::PresenceOnly` specifically to signal "an external material was present but contributed nothing new" — a distinction load-bearing for the D8-02 glass-promotion overload this same audit found — but with the value discarded everywhere it's produced, there is no telemetry or diagnostic sink consuming it. On Starfield, where CDB-resolved materials make this outcome apply to close to 100% of materials, the signal is produced constantly and observed nowhere.

## Evidence
Grep-confirmed during this audit: all four production call sites of the merge functions returning `MergeOutcome` bind the result with `let _ = ...`.

## Impact
No functional defect (the `#[must_use]` lint is suppressed intentionally, not accidentally, at each site), but a missed diagnostic opportunity: a debug-server command or log line surfacing `MergeOutcome::PresenceOnly` counts would make the D8-02-class overload visible operationally, and currently nothing does.

## Related
Adjacent to #2709 (created the `PresenceOnly` variant) and D8-02 (this audit's finding on the overloaded `from_bgsm` meaning `PresenceOnly` is entangled with).

## Suggested Fix
Add a debug-server counter or `debug!`-level log line at one or more of the four call sites that records `MergeOutcome` outcomes (at least distinguishing `PresenceOnly` from a real merge), giving this signal an actual consumer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
