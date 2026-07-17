# TD4-002: audit-audio SKILL.md cites #1859 as still open — closed 2026-07-15

**GitHub Issue**: #2049
**Labels**: medium,tech-debt,documentation

**Severity**: MEDIUM
**Dimension**: 4 (Audit-Finding Rot)
**Location**: `.claude/commands/audit-audio/SKILL.md:85-93`

## Description
Tells the next audit agent that a `SoundCache` docstring path is "still open" as AUD-2026-07-02-01/#1859. Fixed in `37394005` (2026-07-14), closed on GitHub 2026-07-15, and `AUDIT_AUDIO_2026-07-16.md` already confirms it FIXED.

## Evidence
`gh issue view 1859 --json state,closedAt` → `{"closedAt":"2026-07-15T02:29:32Z","state":"CLOSED"}` (confirmed live). `crates/audio/src/lib.rs:1177` now reads the corrected path (`byroredux/src/asset_provider/texture.rs::try_load_default_footstep`, confirmed present at that exact location). The SKILL.md text at lines 85-93 still reads "This path drift is tracked as AUD-2026-07-02-01, still open as of the latest report" — confirmed unchanged.

## Impact
A future agent following the prose literally would spend a cycle "confirming" an already-fixed bug.

## Related
TD4-001 (same Phase-1 block); commit `37394005`.

## Suggested Fix
Replace with a closed-regression-guard note, only re-flag if the path drifts again.

**Age**: fix landed 2026-07-14; text unmodified since.
**Effort**: trivial

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — TD4-001 (same file), TD4-004, TD4-005 share this staleness class
- [ ] **TESTS**: N/A (documentation-only fix)
