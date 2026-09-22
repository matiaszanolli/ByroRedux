# AUD-2026-09-21-D5-04: No smoke fixture supplies --sounds-bsa; Skyrim fixture also lacks Skyrim - Animations.bsa, so no audio consumer is ever exercised end to end

**Issue**: #4745
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: LOW
**Dimension**: 5 — Engine Consumers
**Location**: `docs/smoke-tests/fixtures/{oblivion,fo3,fnv,fo4,skyrim_se}.env`; `docs/smoke-tests/p2-melee-core.sh:155-170`; `docs/engine/p2-combat-anim-sound-fixture.md`; `byroredux/src/boot/cli.rs:303-336`

## Description
None of the five smoke fixtures pass `--sounds-bsa`; `expand_game_profile_args` only expands profile archives under `--game`, not `--esm`, so every smoke route's `SoundArchiveProvider` is empty. The Skyrim fixture also lacks `Skyrim - Animations.bsa`, so `populate_draugr_combat_clips` installs nothing and `combat_feedback_system` returns on its first line. On a device-less runner, `play_oneshot` also returns before queueing, so queue-count observations are unobservable there regardless of archives.

## Evidence
`sed -n '/FIXTURE_ARCHIVE_ARGS=(/,/)/p' docs/smoke-tests/fixtures/*.env` → no `--sounds-bsa` anywhere, confirmed at HEAD.

## Impact
No end-to-end evidence exists for any audio path via the smoke harness; the planned P2 combat-sound gate can't pass as designed against its own named route. Test infrastructure only.

## Related
#3788 (closed, same gap on FNV previously); #4551 (closed); AUD-2026-09-21-D5-01 (#4742), D5-03 (#4744). Owner of fixtures: `/audit-runtime`.

## Suggested Fix
Add `--sounds-bsa` (and `Skyrim - Animations.bsa` for Skyrim) to the fixtures whose gates will assert audio. Add a monotonic `oneshots_requested` counter to `AudioWorld`, incremented before the manager-none gate.
