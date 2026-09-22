# AUD-2026-09-21-D5-05: water_audio_system doc says ripples stay silent while the body plays them

**Issue**: #4746
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: LOW
**Dimension**: 5 — Engine Consumers
**Location**: `byroredux/src/systems/audio.rs:226-229`, `:319-340`, `:1`

## Description
`water_audio_system`'s doc says ripple markers intentionally stay silent, "only the edge-triggered splash is audible." The body plays the strongest ready ripple at ×0.45 whenever no splash fired, rate-limited by a per-surface cooldown. The doc and the ripple-playback code landed in the same commit (`948f104a3`) — the doc was wrong from its first commit. The module header also omits water audio entirely.

## Evidence
`git log -S'intentionally remain presentation' byroredux/src/systems/audio.rs` → `948f104a3`, whose diff already contains the ripple playback branch.

## Impact
Documentation only. A reader investigating an audible repeating water sound would be told ripples can't be the cause.

## Related
#3183, #4147 (closed, same system).

## Suggested Fix
State the actual contract (splashes edge-triggered, ripples audible at ×0.45 with per-surface cooldown, only when no splash fired). Add "water audio" to the module header.
