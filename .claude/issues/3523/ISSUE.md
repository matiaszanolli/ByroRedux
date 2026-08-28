# Issue #3523 — AUD-2026-08-27-D6-01

Source: `docs/audits/AUDIT_AUDIO_2026-08-27.md` · https://github.com/matiaszanolli/ByroRedux/issues/3523

Filed from `docs/audits/AUDIT_AUDIO_2026-08-27.md` (finding `AUD-2026-08-27-D6-01`). This is the **residual of CLOSED #3274** — that fix reached the three designated-authoritative sources; two secondary sites still assert the opposite at HEAD `969d81c8`.

- **Severity**: LOW
- **Dimension**: Manager Lifecycle & ECS/Cell Streaming (documentation)
- **Location**: `byroredux/src/components.rs:498-503` (`RegionAmbientRes` docstring); `ROADMAP.md:1107` (the struck-through Known-Issues M44 bullet)
- **Related**: #3274 (closed 2026-08-25, this is its residual); #3301 (the genuinely-still-pending REGN spatial emitter this docstring conflates with the shipped music path); #3181, #1859 (same doc-drift class)

## Description

#3274 (closed by `a924244e`) correctly fixed the three sources the audit skill designates authoritative — the crate docstring (`crates/audio/src/lib.rs:133-137`), `docs/feature-matrix.md:148-149`, and `ROADMAP.md:706`'s M44 active-milestone row. Two secondary sites still assert the opposite:

1. **`byroredux/src/components.rs:498-503`**, the docstring on `RegionAmbientRes` — the resource the feature is built on:

   ```
   /// carries FormIDs, not decoded audio; `asset_provider::audio`'s
   /// `resolve_sound_path`/`sound_archive_path` resolve them to archive
   /// paths, and a consumer (item 5's REGN-keyed `AudioEmitter`, not yet
   /// built) dispatches actual playback.
   ```

   The consumer *is* built — `dispatch_region_ambient_music` (`byroredux/src/asset_provider/audio.rs:158-205`), wired at four call sites, with 13 passing tests. It is not the "REGN-keyed `AudioEmitter`" that sentence anticipates (a spatial emitter is still correctly future-phase, tracked as #3301), but the parenthetical reads as "nothing dispatches playback", which is now false.

2. **`ROADMAP.md:1107`**, the closed Known-Issues bullet, still lists `Pending: FOOT records → per-material lookup (drops dirt hardcode), **REGN region-keyed ambient layers**, raycast-occlusion attenuation` — the exact clause #3274 corrected one screen up in the M44 row.

## Evidence

`grep -rn "not yet" byroredux/src/components.rs` → line 501. `git show a924244e -- crates/audio/src/lib.rs` shows the fix touched the crate docstring's Future-work bullet only; the commit's own message enumerates `lib.rs` / `feature-matrix.md` / `ROADMAP.md`'s M44 row and nothing else.

## Impact

Documentation only. Lower stakes than #3274 itself — the three primary sources are now right, so a contributor scoping REGN work from `docs/feature-matrix.md` lands correctly. The `components.rs` site matters more than the ROADMAP one because it is the docstring on the resource a REGN contributor would open first.

## Suggested Fix

In `byroredux/src/components.rs`, replace "and a consumer (item 5's REGN-keyed `AudioEmitter`, not yet built) dispatches actual playback" with "and `asset_provider::audio::dispatch_region_ambient_music` dispatches `music_form` through `AudioWorld::play_music`; `incidental_form` still has no consumer (#3301)". In `ROADMAP.md:1107`, narrow "REGN region-keyed ambient layers" to "REGN `incidental`/`sounds` ambient layers (background *music* shipped 2026-08-23)".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other status sites #3274's fix did not reach)
- [ ] **TESTS**: A regression test pins this specific fix
