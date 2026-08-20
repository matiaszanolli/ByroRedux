# #3189 — AUD-2026-08-20-D7-02: try_load_default_water_splash duplicates the --sounds-bsa scan and re-opens the same archive

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3189
- **Labels**: `low,tech-debt,bug`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: LOW
**Dimension**: Gameplay Audio Wiring
**Source**: `docs/audits/AUDIT_AUDIO_2026-08-20.md` (`AUD-2026-08-20-D7-02`) — HEAD `bb0b92f2`

## Location

- `byroredux/src/asset_provider/texture.rs` — `try_load_default_footstep`
- `byroredux/src/asset_provider/texture.rs` — `try_load_default_water_splash`
- `byroredux/src/boot.rs` — the two back-to-back call sites

## Description

`948f104a` added a second boot-time sound loader that is a structural copy of the first. Both are
invoked back-to-back with the same `args`, both scan for `--sounds-bsa` — `try_load_default_footstep`
with a hand-rolled `while i < args.len()` loop, `try_load_default_water_splash` with
`args.windows(2).find(..)`, **two different idioms for one job** — and **both call `Archive::open(path)`
independently on the same file**, so the BSA header plus the full folder/file record tables are parsed
twice per boot for one archive. `Fallout - Sound.bsa` is not small; this is duplicated I/O and
duplicated table allocation for no benefit.

Neither loader routes through `SoundCache` — each writes its decoded `Arc<StaticSoundData>` straight
into its own config resource (`FootstepConfig.default_sound`, `WaterAudioConfig.splash_sound`). This is
not the "first consumer wired without eviction" case the skill's Dimension 3 warns about (nothing was
wired *into* the cache), but it is the **second producer to route around it**, which weakens the
dormant-API argument in #859/#850 each cycle: the cache now has two natural consumers and zero actual
ones.

## Evidence

- `grep -n "Archive::open" byroredux/src/asset_provider/texture.rs` → two sites within ~50 lines of each
  other, both fed by the same `--sounds-bsa` value.
- `grep -n "sounds-bsa" byroredux/src/asset_provider/texture.rs` → the two divergent arg scanners.
- `byroredux/src/boot.rs` → `try_load_default_footstep(&mut world, args);` immediately followed by
  `try_load_default_water_splash(&mut world, args);`.
- `grep -rn "SoundCache" byroredux/src` returns only `byroredux/src/ownership_sample.rs`, a diagnostic
  read of a resource nothing installs.

## Impact

Boot-time only, no runtime cost, no correctness issue — the engine boots correctly with the archive
absent (both loaders log WARN and return, and `water_audio_system` no-ops on `splash_sound: None`, which
the `water_splash_event_reaches_audio_dispatcher` guard covers).

The cost is maintenance: two divergent arg parsers for one flag, and a third copy is the obvious next
step when FOOT records or REGN ambients need their own boot-time loader.

## Suggested Fix

Fold both into one `try_load_default_sounds(world, args)` that resolves `--sounds-bsa` once, opens the
archive once, and populates both `FootstepConfig.default_sound` and `WaterAudioConfig.splash_sound` from
that single handle — ideally through `SoundCache::get_or_load`, which would give the cache its first real
producer and let the eventual FOOT/REGN loaders reuse the decode. If the cache is wired, wire eviction
with it per **#850**.

## Related

- **#859** / **#850** — `SoundCache` dormancy and the eviction-with-first-consumer requirement.
- Directly against the project standing instruction to *"always prioritize improving existing code
  rather than duplicating logic"*.

## Completeness Checks

- [ ] **SIBLING**: the merged loader is the single `--sounds-bsa` entry point — no third scanner is left
      behind for a future FOOT/REGN loader to copy
- [ ] **TESTS**: `water_splash_event_reaches_audio_dispatcher` and the footstep boot guards still pass
      with the merged loader, including the archive-absent path
