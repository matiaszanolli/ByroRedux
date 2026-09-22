# AUD-2026-09-21-D5-03: Combat sound-path pin is tautological — a wrong path fails silently, no test resolves the WAVs

**Issue**: #4744
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: LOW
**Dimension**: 5 — Engine Consumers
**Location**: `byroredux/src/systems/combat_anim.rs:734-741` (sound loop of `fixture_paths_and_resource_contract_stay_aligned`), `:382-399` (`play_oneshot_cached` miss path); `byroredux/src/asset_provider/animation.rs:905-958`

## Description
The test's sound-path loop asserts `include_str!("combat_anim.rs").contains(SWING_SOUND_PATH)` against the same file that defines the constant — trivially true for any value, including a typo no archive contains. No test extracts/decodes the three combat WAVs from a real archive. At runtime, an extract miss is cached as `None` with no log, unlike REGN dispatch's explicit "not found in any --sounds-bsa archive" warning.

## Evidence
Paths correct at HEAD (confirmed present in SE/LE `Skyrim - Sounds.bsa` by a prior read-only probe). The gap is a future regression going unnoticed.

## Impact
A renamed or mistyped combat sound path would silently and permanently disable that sound for the session, with no test failure or log signal.

## Related
#4604 (closed, same vacuous-needle shape elsewhere in this file); AUD-2026-09-21-D5-04 (#4745, no smoke coverage either).

## Suggested Fix
Replace the self-referential loop with an `#[ignore]`d real-data check extracting/decoding all three paths from `Skyrim - Sounds.bsa`. Warn once on a non-empty-provider extract miss, mirroring the REGN dispatcher.
