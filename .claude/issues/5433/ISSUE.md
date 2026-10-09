# #5433: GAME-D2-2026-10-08-04: The FO3 profile ships no sound archives, so the FO3 half of #5367 Phase V never resolves through the profile path

**Labels**: low,gameplay,dialogue,audio,bug,game:fo3
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5433

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D2-2026-10-08-04` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Dim 2
- **Location**: `assets/debug_profiles.toml:134-150` (`[profiles.fo3]`)
- **Status**: NEW
- **Description**: Phase V is documented as "FO3/FNV", but only `[profiles.fnv]` gained a voice archive (`Fallout - Voices1.bsa`). `[profiles.fo3]` has no `default_sounds_bsas` at all, although `Fallout - Voices.bsa` and `Fallout - Sound.bsa` ship in the FO3 Data dir. `SoundArchiveProvider` is therefore empty, and every FO3 line quietly falls back to the estimate, along with FO3 footsteps, splash and REGN audio (#3788's FNV fix was never mirrored).
- **Suggested Fix**: Add `default_sounds_bsas = ["Fallout - Sound.bsa", "Fallout - Voices.bsa"]` to the FO3 profile, after verifying the voice path convention against the FO3 archive.

## Completeness Checks
- [ ] **SIBLING**: Oblivion profile sound archives checked
- [ ] **TESTS**: A regression test pins this specific fix
