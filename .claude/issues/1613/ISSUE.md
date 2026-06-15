# AUD-2026-06-14-02: SoundCache::bytes_estimate telemetry docstrings claim present-tense stats wiring that does not exist

- **Issue**: #1613
- **Severity**: LOW
- **Labels**: low, tech-debt, documentation
- **Dimension**: SoundCache Growth
- **Location**: `crates/audio/src/lib.rs:1149-1150`, `:1257-1259` (docstrings); fn `bytes_estimate` at `:1260`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-06-14.md`

## Description
Docstrings claim `bytes_estimate` "surfaces the cache footprint to telemetry so a future unbounded-growth regression shows up in `stats` output rather than at OOM." No such wiring exists: `bytes_estimate` / `len` / `active_sound_count` / `pending_oneshot_count` have zero non-test call sites; `SoundCache` is never registered as a resource; the `stats` console command has no audio line. The claim is forward-looking written in present tense.

## Evidence
- `bytes_estimate` only referenced by `tests.rs::sound_cache_clear_drops_entries_and_bytes_estimate_tracks_pcm_size`.
- `SoundCache` never `insert_resource`-d. Re-confirmed 2026-06-15.

## Impact
Doc-accuracy only — safety claim is untrue today. `SoundCache` is dormant by design (#859), so practical risk is nil, but the docstring overstates present state.

## Related
#859 (dormant SoundCache, accepted); #850 (no LRU, accepted).

## Suggested Fix
Soften docstrings to future tense, OR wire `bytes_estimate` + `len` into `stats` output when the first `SoundCache` consumer lands (with eviction).
