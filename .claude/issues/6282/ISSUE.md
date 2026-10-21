# #6282: AUD-2026-10-21-D3-03: Audio docs lag the new codecs and consumers

**Labels**: low,audio,documentation,doc-rot

**Source**: docs/audits/AUDIT_AUDIO_2026-10-21.md · **Severity**: LOW · **Dimension**: Music & SoundCache / docs · **Location**: `crates/audio/src/lib.rs:41-45` (module doc), `crates/audio/src/lib.rs:1753` (`SoundCache` doc); `docs/feature-matrix.md:150-166`; `byroredux/src/asset_provider/audio.rs:225`

**Audit status**: NEW for the `lib.rs` items. The feature-matrix rows are part of **Existing: #4747** (status docs).

- **Description**:
  - The `8dabcda8d` codec refresh says the workspace "enables WAV/PCM and OGG/Vorbis explicitly". It omits `mp3`, which now carries Oblivion voice (#5526), and `flac`. `load_sound_from_bytes`'s own doc lists all four correctly.
  - The module and `SoundCache` docs still name "dialogue line" as a `SoundCache` use. Voice moved to `VoiceSoundCache` in #5382, and the real open-ended `SoundCache` consumer (script sounds, D3-02) is not mentioned.
  - The feature matrix has no row for script sound plays (`Sound.Play`, `PlaySound`, `PlaySound3D`, graph `SoundPlay.*`) or scripted `PlayMusic`. It does not say that `.fuz` voice depends on an optional host `ffmpeg`, and the music row still reads "Streaming music (OGG, crossfade)".
  - The REGN dispatcher's comment "No `MUSC`/`MUST` or `MSET` runtime exists yet" is now half-true: FO3/FNV `MUSC` is decoded (`misc/music.rs`, consumed by `PlayMusic`). Only Skyrim `MUSC`, `MUST` and `MSET` remain undecoded.
- **Impact**: Readers of the module docs and the status page get a wrong picture of which codecs are load-bearing and which consumers exist.
- **Suggested Fix**: Fold these into the #4747 doc pass. Add `mp3`/`flac` to the module doc, retarget "dialogue line" to script sounds, add the script-audio rows, and narrow the REGN comment to Skyrim `MUSC`/`MUST` + FNV `MSET`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other audio doc sites (ROADMAP M44 row, feature matrix) folded into the same pass)
