# AUD-2026-09-21-D3-01: Combat feedback system hand-rolls its own sound cache instead of SoundCache

**Issue**: #4739
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: LOW
**Dimension**: 3 — Music & SoundCache
**Location**: `byroredux/src/systems/combat_anim.rs:89-97` (`FeedbackScratch.sounds`), `:373-417` (`play_oneshot_cached`); `crates/audio/src/lib.rs:1460-1471` (#859 "Dormant API" contract), `:1513-1532` (`SoundCache::get_or_load`); `byroredux/src/ownership_sample.rs:67-72`

## Description
`combat_feedback_system` decodes its three pinned WAVs on first use and caches them in a closure-scratch `FxHashMap`, re-implementing `SoundCache::get_or_load` instead of using it. It is the first *runtime* sound producer to bypass `SoundCache` (the other two bypassers decode once at boot). The private map does cache a decode/extract miss as `None`, which `SoundCache::get_or_load` currently does not — a real gap worth closing in the shared cache rather than forking around.

## Evidence
`scratch.sounds.insert(path, decoded.clone())` (`:407`) after a `SoundArchiveProvider::extract` → `load_sound_from_bytes` chain. `git grep -n 'SoundCache' byroredux/src` hits only an unused `try_resource` and a save-registry test.

## Impact
Bounded today (~318 KB PCM, 3 keys). Structural cost: decoded audio invisible to `sound_cache_entries`/`bytes_estimate` telemetry; a second cache contract exists beside the crate's own; the next producer has two templates to copy.

## Related
#859, #850, #3189 (closed); AUD-2026-09-21-D5-01 (#4742, same system's swing path).

## Suggested Fix
Add a negative-cache arm to `SoundCache::get_or_load`. Install one engine `SoundCache` resource at boot and route `play_oneshot_cached` through it. Sample `bytes_estimate` in `ownership_sample.rs`.
