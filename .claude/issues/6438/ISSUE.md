# AUD-2026-10-27-D3-01: Folder-form music types (`DefaultBattle`, `DefaultExplore`, ...) reach `PlayMusic` and fail with a misleading "failed to decode" warning

Labels: low, audio, bug, game:fo3, game:fnv
Source: docs/audits/AUDIT_AUDIO_2026-10-27.md

**Report**: `docs/audits/AUDIT_AUDIO_2026-10-27.md` · **Finding**: AUD-2026-10-27-D3-01 · **Severity**: LOW · **Dimension**: Music & SoundCache
**Location**: `byroredux/src/systems/script_requests.rs:246-266` (`music_system`, `find_case_insensitive` at `:254`/`:288`); `crates/scripting/src/obscript_more/extras.rs:45-52` (`MusicTracks::from_index`); `docs/feature-matrix.md:159`
**Games**: FO3 / FNV

## Description
In both Fallout3.esm and FalloutNV.esm, 6 MUSC records author a folder `FNAM` (`battle\`, `explore\`, `base\`, `dungeon\`, `public\`, `tranquilitylane\`): a directory of variant tracks. Fallout3.esm scripts call `PlayMusic` on `defaultBattle` / `defaultExplore` (2026-10-21 SCTX census). `MusicTracks::from_index` filters out only empty files, so these reach `music_system`. There, the archive lookup misses on `music\battle\`; `find_case_insensitive(Data, "Music\battle\")` filters the empty trailing component and returns the existing `Data/Music/Battle` **directory**; `load_streaming_sound_from_file(dir)` then errors and the system warns `PlayMusic: 'music\battle\' failed to decode`. Nothing plays, and the log blames the decoder for what is really an unmade variant-selection policy. For SOUN this exact shape is fenced by `SounRecord::is_folder` / `sound_is_folder` ("any future consumer must gate on it", #3914); the MUSC path has no such gate. The feature matrix marks scripted `PlayMusic` as checked with no qualifier.

## Evidence
MUSC census: Fallout3.esm 31 records (6 with a folder FNAM), FalloutNV.esm 20 records (same 6 folders). On disk `Fallout 3 goty/Data/Music/Battle/` holds `Battle_01.mp3`.... `script_requests.rs:254` (`find_case_insensitive(&data, &format!("Music\\{}", track.file))`), `:288` (`.filter(|part| !part.is_empty())`), `:259-261` (decode warning). `from_index` filter: `!musc.file.is_empty() && !musc.editor_id.is_empty()` (verified at HEAD).

## Impact
FO3/FNV scripted battle and explore music cues are silent. Each attempt logs a WARN that sends triage toward the codec.

## Related
`sound_is_folder` (`byroredux/src/asset_provider/audio.rs`), #3914 (SOUN folder form), #3816 (REGN music targets), #6226.

## Suggested Fix
Gate a trailing-separator `file` before resolution with a once-per-type debug or info line ("folder music type - variant selection not modelled"), mirroring `sound_is_folder`, and qualify the matrix row. Choosing a variant is a separate policy decision, so do not add a fallback.

## Completeness Checks
- [ ] **SIBLING**: REGN/CELL music paths (#3816) checked for the same folder-form MUSC input
- [ ] **TESTS**: A regression test pins the folder-form gate (no decode warning, one info line)

