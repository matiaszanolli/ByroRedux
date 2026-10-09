**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-05.md` (HEAD `a2c24b16e`) · **Audited**: Dim 3 (21 decode commits on the listed paths, plus the new dialogue-voice translation) · **Unchanged since baseline (skimmed)**: Dim 1 (0 commits on the listed paths; guards run and out-of-path placement commits read), Dim 2 (4 commits on the listed paths, all owned by gameplay, CHARAL or NIFAL, or test-only)

# Legacy Compatibility Audit — 2026-10-08

## Method

This run is delta-scoped against the 2026-10-05 baseline. There are 116 commits in range.

- **Legacy source.** The Gamebryo 2.3 drive is **not mounted** (`/media/matias/Respaldo 2TB/...` is absent). I used `/mnt/data/src/reference/gamebryo-v32/Include` and `gamebryo-v26/NiAnimation` instead. Those trees hold headers only, so no `.cpp` bodies were available.
- **External references** (fetched to scratch, read-only):
  - xEdit `wbDefinitions{TES4,FO3,FNV,TES5,FO4}.pas` (dev-4.1.6).
  - CommonLibSSE-NG `BGSStoryManager{NodeBase,QuestNode,BranchNode}.h`.
  - The local UESP dump `Skyrim Mod/Mod File Format/{SMQN,SMBN,SMEN}.wiki` and the local GECK/CS wikis.
- **Raw-data censuses.** Python walkers over sub-records and BSA name tables only:
  - CTDA function 0 across all Oblivion, FO3, FNV (plus 10 DLC), Skyrim SE (plus 3 DLC and `Update.esm`), FO4 and Starfield masters.
  - Story Manager `SNAM` ordering in `Skyrim.esm`.
  - The voice file-name convention: FNV `Fallout - Voices1.bsa` (105,517 entries) and FO3 `Fallout - Voices.bsa` (51,553), each joined against the INFO → `QSTI` quest EDID and the parent-DIAL topic EDID. Also the FNV DLC `* - Main.bsa` voice entries.
- **Tests and tools run:**
  - `cargo test -p byroredux-core --lib math::coord`: 13 passed.
  - `cargo test -p byroredux-nif --lib strip`: 25 passed.
  - `cargo run --release -p byroredux-plugin --example cond_dump` on `FalloutNV.esm` `0x105CC7`.
  - Not run: the bin-crate REFR Euler guards. Nothing in range touched `euler.rs`.
- **Dedup sources:**
  - `/tmp/audit/issues.json` (113 open).
  - `gh` closed-issue searches.
  - Today's sibling reports: `AUDIT_ESM`, `AUDIT_SCRIPTING`, `AUDIT_GAMEPLAY`, `AUDIT_AUDIO`, `AUDIT_CONCURRENCY` and `AUDIT_CHARACTER`, all dated 2026-10-08.

No source, skill or issue was modified, and no engine was launched. Scratch files are in `/tmp/audit/legacy-compat/` (`dim_1..3.md`, `scripts/`, `xedit/`, `clib/`).

## Executive Summary

| Severity | NEW | Already tracked / cross-referenced |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 2 | 2 (ESM-2026-10-08-D4-01, ESM-2026-10-08-D4-02) |
| LOW | 0 | 2 (GAME-D5-2026-10-08-05, #5300 scope note) |

**Headline.** The two new MEDIUM findings are in `#5367` Phase V (`f8950e7cc`). It translates the authored FO3/FNV voice-file convention incompletely, in two independent ways.
1. **File name (LC-D3-01).** The name skips Bethesda's EDID truncation and uses the load-order FormID byte. As a result, 71.7% of vanilla FNV lines and about 41.6% of vanilla FO3 lines never resolve, and no DLC line ever can.
2. **Plugin folder (LC-D3-02).** The folder is resolved with an inverted load-order mapping that re-implements the `#3366` defect class. In any session with a `--master`, no line resolves at all.

The failure is silent in both cases: a `log::debug!` line, then the subtitle-length estimate is used instead.

**Coordinates and the legacy subsystems are clean.** No new duplicated swap, `4096` cell literal, matrix→quat path or destrip appears in range.

**Prior findings:**
- **`#5079`** is CLOSED by `7c7711cff` and verified.
- **`#5345`** (FO76 LSCR) and **`#5346`** (the duplicated Starfield lighting-tail decoders) are still open, with unchanged code.
- **`#4128`** is open and unchanged. **`#4127`** is closed.

---

## Dimension 1 — Coordinate + placement fidelity (unchanged; guards run)

**Listed paths.** No commits since `a2c24b16e` touched `crates/core/src/math`, `import/coord.rs`, `rotation.rs`, `strip.rs`, `cell_loader/euler.rs` or `camera.rs`.

**Added-line scan of the full diff.** I searched every added line for `(x, z, -y)`, `4096.0`, `from_mat3`, `EulerRot`, `zup_to_yup`, `cell_grid_to_world_yup` and `EXTERIOR_CELL_UNITS`. Nothing new needs action:
- **`streaming/mod.rs::world_pos_to_grid`** was moved by the `#5092` split (`54d713dee`). It already used floor plus the Z flip.
- **`systems/loading_model.rs`** adds a turntable `Quat::from_rotation_y`. That is an engine-space spin, not a Bethesda Euler path.
- **`shader_constants_data.rs`** emits `EXTERIOR_CELL_UNITS` from the core constant.

**Single-implementation checks:**
- The production `Quat::from_mat3` sites are still only the two documented exceptions: `nif/import/collision/mod.rs:731` and `physics/ragdoll.rs:838`.
- `strip::destrip` callers are `ni_tri_shape.rs:628`, `skin.rs:335` and `collision/shape.rs:668`.
- `normalize_quat` has only its two coord wrappers as callers.
- Every REFR, LSCR, door-transition and placement-LOD rotation goes through `euler_zup_to_quat_yup_refr`.
- `--rotation-mode` is still unclamped (`boot/cli.rs:453`).

**Out-of-path placement commits read:**
- **`5f1a862be` (`#5299`).** The Starfield XRGD bone positions are lifted inside `spatial_units::cell`, which is the correct single site. Euler angles stay at their wire values.
- **`3ad2dcd47` (`#5296`).** TRNS Around Origin is now read from `0x10000`. That matches the xEdit bit *index* and the census.
- **`d63131c57` (`#5231`).** No transform math changed.
- **`b7987d813` (`#5222`).** Owned by EXAL; it adds no coordinate math.

**No findings.**

## Dimension 2 — Legacy subsystem coverage (skimmed)

**In-range commits on the listed paths:**
- `00f580e09`: the `eat_sleep.rs` gameplay markers.
- `2464a52d7`: CHARAL `actor_values.rs`.
- `46f59e1d2`: a doc-only change to `material.rs`.
- `655b317c9`: the `anim_convert.rs` guard now uses `core::source_scan::production_text`. This closes a vacuous self-satisfying guard of the `#4842` class.

None of these touches animation, string-interning, scene-graph or property-sink fidelity.

**Spot-checks:**
- **Property sinks.** The four `NiFlagProperty` subtypes are unchanged in `legacy_properties.rs:1044-1081`: Specular → `specular_enabled`, Wireframe → LINE pipeline, Shade → `INSTANCE_FLAG_FLAT_SHADING`, and Dither ignored by design. `NiZBuffer` and `NiStencil` still have sinks. `NiFog` is still the documented limitation.
- **`#5079`.** Verified closed: there is one `npc_spawn::is_child_race` (`npc_spawn.rs:606`) with two callers and a per-game test.

**No findings.**

## Dimension 3 — Cross-game translation-pattern spot-check

**First-step greps:**
- **Pattern A.** The only raw `bsver` literal thresholds are still the three test asserts at `blocks/base.rs:530,550,551`.
- **New `GameKind` branches in `cell_loader/`:**
  - `exterior.rs:1894`: the Oblivion climate naming rung, `b4497ec1d`.
  - `terrain_lod.rs:462`: the Fallout-legacy authored-quad index, `#5222`.

  Each is a single keyed decision owned by EXAL, which is an acceptable shape.

**Decode commits verified:**
- **`#5295` INFO `DATA`.** It matches the xEdit TES4 layout (`Type, Next Speaker, Flags`, `SetOptionalFrom(2)`) and the FO3/FNV layout (`Flags 1` bit 7 = Speech Challenge, plus `Flags 2`). The Skyrim 8-byte form is carried separately.
- **`#5271` QSTI.** It is wire-discriminated: QSTI-less INFOs keep the DIAL-side semantics.

### LC-D3-01: The FO3/FNV voice file name skips the authored EDID truncation and uses the load-order FormID byte, so 72% of FNV and about 42% of FO3 vanilla lines (and every DLC line) never find their audio
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

### LC-D3-02: `plugin_file_for` reads FormID byte 0 as the `--esm` and byte *k* as `masters[k-1]`, the inverse of the masters-first load order, so every voice line resolves the wrong plugin folder in any session with a `--master`
- **Severity**: MEDIUM. Silent loss of the authored input across every multi-plugin session, which is the normal shape once any DLC is loaded.
- **Dimension**: 3 — Cross-game translation (load-order identity re-derived by hand)
- **Location**: `byroredux/src/systems/dialogue_voice.rs:66-85`
- **Status**: NEW. `f8950e7cc`. The defect class is the one `#3366` fixed in `cell_loader/load_order.rs::plugin_for_form_id`.
- **Description**:
  - Load order is masters first, then the main plugin: `cell_loader/load.rs:389-392` chains `masters` and then `esm_path` into `parse_record_indexes_in_load_order`, so slot 0 is the first `--master`.
  - `plugin_file_for` maps `byte == 0` to `esm_path` and `byte k` to `masters[k-1]`. That is correct only when there are no masters.
  - It also reads the top byte as a load-order position. `plugin_for_form_id`'s `#3366` doc explains why that breaks once a light master is present (slot ≠ position, and the `0xFE` space). The engine already installs `GlobalFormIdResolver` (`load_order.rs:268`, `resolve(form_id) -> FormIdPair`) for exactly this lookup.
- **Evidence**:
  ```rust
  // dialogue_voice.rs:73-78
  let byte = (form_id >> 24) as usize;
  let raw = if byte == 0 {
      esm_path.as_str()
  } else {
      masters.get(byte - 1).map(String::as_str)?
  };
  ```
  With `--master FalloutNV.esm --esm DeadMoney.esm`, Doc Mitchell's `0x00107222` resolves to `sound\voice\deadmoney.esm\…` and Dead Money's `0x01…` lines resolve to `falloutnv.esm`. Neither exists. The `dt1-dialogue-layers.sh` gate passes only because it runs `--esm FalloutNV.esm` with no master.
- **Impact**: Every voice line fails in a session with masters (DLC interiors, or FO3/FNV plus any DLC). Combined with LC-D3-01, this is total voice loss there, and it falls back silently to the subtitle estimate.
- **Related**: LC-D3-01, `#3366` (closed), `#5367`.
- **Suggested Fix**: Resolve the owning plugin through `GlobalFormIdResolver::resolve(info.form_id)`, which gives the plugin name and the local id that LC-D3-01 also needs. Delete `plugin_file_for`. Add a `--master` + `--esm` unit test.

### Cross-referenced — already filed today, not re-filed

- **ESM-2026-10-08-D4-01 (Story Manager `SNAM` is the *previous* sibling, but is walked as the next).** I reached the same conclusion independently and add two sources the ESM report does not cite:
  1. **CommonLibSSE-NG** `BGSStoryManagerNodeBase`: `BGSStoryManagerNodeBase* previousSibling; // 30 - SNAM`.
  2. **UESP** `Skyrim Mod/Mod File Format/SMQN.wiki` and `SMBN.wiki`: SNAM = "0 (If first child) or reference to prior sibling".

  My own `Skyrim.esm` census: the `SNAM` target has the older FormID on 303/443 edges. Sibling steps are FormID-increasing on 231/347 under the "previous" reading versus 116/347 under the decoded "next" reading. The CLOC order under "previous" begins `QuestNode, CompanionsNode, IntroScenesNode, CWChangeLocationScenes(0x0), …`, which matches the ESM report.
- **ESM-2026-10-08-D4-02 (SM fields left "[open]").** CommonLibSSE confirms the xEdit reading:
  - `DNAM` is `u16 nodeFlags` {Random, WarnIfNoChildQuestStarted} plus `u16 questFlags` {DoAllBeforeRepeating, SharesEvent, NumQuestsToRun}. That is `0x2` = Warn and `0x40000` = Num quests to run.
  - `XNAM` = `maxQuests`, `MNAM` = `numQuestsToStart`, and `FNAM` = `perQuestFlags`.
- **GAME-D5-2026-10-08-05 (CTDA fn 0 `GetButtonPressed` premise).** Cross-game addendum:
  - **xEdit tables.** Index 0 is `GetWantBlocking` in TES5 (`wbDefinitionsTES5.pas:220`) and FO4 (`:262`). TES4, FO3 and FNV have no index 0.
  - **Census of fn-0 CTDAs.** FNV `FalloutNV.esm` and all 10 DLC esms: 0. `Fallout3.esm` 0, `Oblivion.esm` 0, `Fallout4.esm` 0, `Starfield.esm` 0. `Skyrim.esm` 19, `Update.esm` 9, `Dawnguard.esm` 8 and `Dragonborn.esm` 2, all on IDLE records.
  - **Effect.** The game-agnostic `ConditionFunction::from_index` (`crates/scripting/src/condition.rs:191-229`) therefore relabels Skyrim's `GetWantBlocking` as a constant −1 and fires on no Fallout or Oblivion record.
  - **Sunny's package.** `cond_dump` on `0x105CC7 SunnyMeetPlayerDialoguePackage` prints `fn=79` (GetQuestVariable).
  - IDLE conditions are not evaluated today, so there is no runtime effect yet.
- **`#5300` scope note.** Skyrim and FO4 INFO response flags live in `ENAM`, which is not decoded, so `goodbye()`, `random()` and `say_once()` are false on every Skyrim/FO4 INFO. The decode must be per game: in xEdit FO4, `ENAM` bit 0 is "Start Scene on End", not Goodbye.

---

## Scratch-file reconciliation

`/tmp/audit/legacy-compat/dim_1.md`, `dim_2.md` and `dim_3.md` were checked against this report.
- **`dim_1.md`** and **`dim_2.md`** have no findings.
- **`dim_3.md`** has LC-D3-01, LC-D3-02 and the four cross-references.

All of them appear above, and nothing was dropped.

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-10-08.md
```
Labels:
- **LC-D3-01:** `medium` `bug` `legacy-compat` `audio` `dialogue` `game:fnv` `game:fo3`.
- **LC-D3-02:** `medium` `bug` `legacy-compat` `audio` `dialogue` `game:fnv` `game:fo3`.
