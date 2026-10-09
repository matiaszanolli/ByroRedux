# #5395: LC-D3-01: The FO3/FNV voice file name skips the authored EDID truncation and uses the load-order FormID byte, so 72% of FNV and about 42% of FO3 vanilla lines (and every DLC line) never find their audio

**Labels**: medium,legacy-compat,audio,dialogue,bug,game:fnv,game:fo3
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5395

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-08.md` — `LC-D3-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `voice_path_candidates` (`byroredux/src/systems/dialogue_voice.rs:39`) formats the full lowercase EDIDs and `{:08x}` of the load-order FormID. Related to the open #5367 epic (Phase V), not a duplicate.

- **Severity**: MEDIUM. A translatable authored input is silently dropped: there is no voice, and the subtitle estimate is used instead. Nothing visible is removed.
- **Dimension**: 3 — Cross-game translation (an authored legacy convention mapped incompletely)
- **Location**: `byroredux/src/systems/dialogue_voice.rs:39-59` (`voice_path_candidates`), `:143-168` (segment loop); test `:216-238`
- **Status**: NEW. `f8950e7cc` (`#5367` Phase V). `AUDIT_AUDIO_2026-10-08` covers only cache eviction and lock order on this path.
- **Description**: The real FO3/FNV file name is not `<quest EDID>_<topic EDID>_<FormID>_<n>` but the following:
  - **Truncated EDIDs.** `lower(quest_edid[..10]) + "_" + lower(topic_edid[..25 - len(quest part)])`. The quest is cut to 10 characters, and the topic is cut so that quest plus topic total 25.
  - **Radio exception.** Radio quests (RadioNewVegas, GNR) use the untruncated EDIDs.
  - **FormID byte.** The FormID is the plugin-local object id with the top byte zeroed (`00xxxxxx`), even in DLC archives.

  `voice_path_candidates` formats the full quest and topic EDIDs and `{:08x}` of the *load-order* FormID. So it resolves only lines whose quest EDID is 10 characters or fewer and whose combined length is 25 or fewer, plus radio, and only for slot-0 records.
- **Evidence** (joining BSA names against `FalloutNV.esm` / `Fallout3.esm` INFO → QSTI quest EDID and parent DIAL EDID; script `/tmp/audit/legacy-compat/scripts/voice_join3.py`):

  | Archive | Joinable `.ogg` | Match full EDIDs (what Redux builds) | Match the truncation rule | Neither |
  |---|---:|---:|---:|---:|
  | FNV `Fallout - Voices1.bsa` | 52,876 | 14,953 (28.3%), of which 11,831 are radio-only | 41,039 | 6 |
  | FO3 `Fallout - Voices.bsa` | 25,743 | 15,044, of which 9,644 are radio-only | 16,094 | 5 |

  - **Truncation examples:** `vdoctors_doctormedical99ye` (`VDoctors` + `DoctorMedical99YES`), `vcg02_vcg02gssunnysmilesto` (`VCG02` + `VCG02GSSunnySmilesTopic004`), and FO3 `dialogueli_goodbye_00038825_1`.
  - **DLC FormID byte.** In `DeadMoney - Main.bsa` (5,352 voice files), `HonestHearts - Main.bsa` (4,540) and `OldWorldBlues - Main.bsa` (4,367), every file carries top byte `00`. An example is `nvdlc01eli_greeting_00011216_1`, while those INFOs are `0x01xxxxxx` in their own plugin.
  - **Bare-FormID fallback.** The `"{:08x}_{n}.ogg"` shape matches **0** files in either archive.
  - **Response numbering.** In 315 FNV and 19 FO3 INFOs the response suffixes do not run 1..k (for example `0015d97c` has only `_3`). The loop indexes `1..=segments` and returns `None` when `_1` is missing, instead of using `ResponseSegment::response_number`.
  - **Test.** The unit test pins only `vcg01_greeting_00107222_1`. Both of its EDIDs are short, so the truncation never shows.
- **Impact**: In a vanilla FNV session, about 37,900 of 52,900 voiced lines play silently on a subtitle-length timer. On FO3 it is about 10,700 of 25,700. The only log is `log::debug!("#5367 V: no voice file …")`, so this cannot be seen at default verbosity. No DLC-owned line can ever resolve, even with LC-D3-02 fixed.
- **Related**: LC-D3-02, `#5367`, `docs/engine/dialogue-trees.md` §5 (which records the untruncated convention).
- **Suggested Fix**:
  - Build the primary candidate with the truncation rule, then fall back to the full EDIDs (for radio).
  - Format `form_id & 0x00FF_FFFF`, or the plugin-local id from `GlobalFormIdResolver::resolve`.
  - Use each segment's `response_number`.
  - Pin it with a real-BSA test over a long-EDID line (for example `vdoctors_doctormedical99ye_…`) and a DLC line.

## Also reported as `FNV-2026-10-08-D5-01` (AUDIT_FNV_2026-10-08.md)

Cross-report duplicate merged at publish time; the sibling report's text follows.

**Source**: `docs/audits/AUDIT_FNV_2026-10-08.md` — `FNV-2026-10-08-D5-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM. Phase V's DLC half fails silently by construction and falls back to the subtitle estimate. Today it is masked by GAME-D2-02, which picks the wrong plugin folder, but it is an independent defect.
- **Dimension**: Ambient AI / dialogue data on FNV. The mechanism belongs to `/audit-gameplay` (Dim 2) and `/audit-audio`.
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:49-59` (`voice_path_candidates`, both shapes use `{:08x}` of `info_form_id`).
  - `byroredux/src/systems/dialogue_voice.rs:144-151` (the caller passes `info.form_id`, the global id).
  - `assets/debug_profiles.toml` FNV `default_sounds_bsas` (no DLC archive).
- **Status**: NEW. GAME-D2-2026-10-08-02 covers only the slot → plugin-folder mapping. Its suggested fix (`plugin_for_form_id`) leaves the file-name id unchanged.
- **Description**:
  - Bethesda's voice convention writes the INFO's FormID with the load-order byte stripped (plugin-local, 00 prefix).
  - A DLC INFO lives at global slot ≥ 1, e.g. `0x0101_1216` with DeadMoney at slot 1. So the composed name is `…_01011216_1.ogg`, while the shipped file is `nvdlc01eli_greeting_00011216_1.ogg`.
  - Separately, FNV DLC voice ships inside `<DLC> - Main.bsa`, not a `- Sounds.bsa`. Nothing mounts those archives into the `SoundArchiveProvider` pool, neither the profile nor any DLC-archive auto-mount.
- **Evidence**: voice-file names in the shipped archives (`strings` over the BSA name tables), grouped by the FormID's high byte:

  | Archive | 00 prefix | Any other prefix |
  |---|---|---|
  | `DeadMoney - Main.bsa` | 2,794 | 0 |
  | `HonestHearts - Main.bsa` | 2,277 | 0 |
  | `OldWorldBlues - Main.bsa` | 2,190 | 0 |
  | `LonesomeRoad - Main.bsa` | 1,123 | 0 |
  | `Fallout - Voices1.bsa` | 52,896 | 0 |

  Folder example: `sound\voice\deadmoney.esm\nvdlc01maleuniqueelijah`. Code:
  ```rust
  "sound\\voice\\{plugin}\\{voice_type}\\{}_{}_{:08x}_{response}.ogg", …, info_form_id
  ```
- **Impact**: in any FNV DLC session, every DLC-authored voiced line (8,384 files across the four story DLCs) stays silent. The Phase L Goodbye close falls back to the text estimate. `dt1` boots FalloutNV.esm alone, where slot 0 masks the bug.
- **Related**: GAME-D2-2026-10-08-02, FNV-2026-10-08-D2-01 (with D2-01 unfixed, DLC sessions select mostly DLC INFOs, so this bug decides whether they are voiced), #5367 (V), AUD-2026-10-08-D5-01.
- **Suggested Fix**: compose the name from `info.form_id & 0x00FF_FFFF` (for ESL/medium slots, the plugin-local id that `plugin_for_form_id`'s decomposition yields). For DLC sessions, add the loaded DLCs' `- Main.bsa` to the sound pool, or document the required `--sounds-bsa`. Add a unit case for a slot-1 INFO.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (FO3 vs FNV radio exception; DLC voice archives)
- [ ] **TESTS**: A regression test pins this specific fix (long-EDID line such as `vdoctors_doctormedical99ye_…` and a DLC line)
