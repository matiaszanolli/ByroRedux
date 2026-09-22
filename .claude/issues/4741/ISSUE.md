# AUD-2026-09-21-D4-02: audio_system registration comment cites a dead line number and a phrase that doesn't exist

**Issue**: #4741
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: LOW (documentation)
**Dimension**: 4 — Manager & ECS Lifecycle
**Location**: `byroredux/src/boot/schedule/late.rs:233-244`

## Description
The `audio_system` registration comment cites "line 650-656 above" quoting "MUST run BEFORE audio_system" — `late.rs` is 473 lines total and that quote appears nowhere. The text was written for `main.rs` in M27 (2026-05-23), carried verbatim through two file splits (#1858, #3855) without ever updating the pointer, and shifted again this cycle by an unrelated 3-line insert (#4574). The actual dependency is `camera_follow_system`'s note at `:17-18`. Mechanism is correct; only the citation is stale.

## Evidence
`wc -l late.rs` → 473. `grep -rn 'MUST run BEFORE audio_system'` → only this comment. `git log -S'line 650-656'` → `05fe2bac2`, `40d533a85`, `8c5e02aab`.

## Impact
Documentation only — same drift class #3522/#4146 already closed once on the neighboring `reverb_zone_system` comment.

## Related
#3522, #4146 (closed, same class); #1858, #3855 (the splits); #4574 (this cycle's shift).

## Suggested Fix
Name the dependency instead of a line number: "`camera_follow_system` (Late parallel batch) authors the camera pose this reads; exclusive sequencing runs after that batch." Drop the phantom uppercase quote.
