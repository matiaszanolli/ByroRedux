# Parser Discipline Audit: 2026-10-05

**HEAD**: a2c24b16e · **Baseline**: `AUDIT_PARSERS_2026-09-29.md` (HEAD 9fcfdc3fc) · **Audited**: Dims 1–6 (Size Discipline, Error Semantics, Version Gating, Corpus Gates, Decode-Consumer Wiring, I/O & Paths). All six were deep-audited because every dimension's Paths changed since the baseline. The changes were the six 09-29 fixes (#5006, #5007, #5008, #5012, #5014, #5016) and the Starfield CDB Phase 2 `MaterialIndex` (`224a19372`, `18fce7e43`, `978d25c19`, `870ea1d07`) · **Unchanged since baseline (skimmed)**: none

This run is one leg of `/audit-suite --preset comprehensive`. Following the suite rules, every dimension was analysed in this session, one after another, with no sub-agents. Per-dimension notes are at `/tmp/audit/parsers/dim_{1..6}.md`. The skill's Phase-3 `rm -rf /tmp/audit/parsers` was **deliberately not run**, because the suite orchestrator owns that cleanup. No engine binary was launched.

## Executive Summary

**Findings:** 0 CRITICAL · **1 HIGH** · **1 MEDIUM** · 5 LOW (7 total). All seven are NEW. Two are residuals of earlier fixes: #4650 (MenuXml) and #5006 (HKX). One was introduced by a fix: #5007.

**The HIGH and the MEDIUM:**
- **HIGH: the MenuXml splice budget counts splices, not bytes (PAR-D1-2026-10-05-01).** #4650's budget allows 256 splices of *any* size, so one fragment is parsed up to 256 times. A fragment of repeated `<rect/>` costs about 7.2 MiB of resident tiles per KiB. A 256 KiB fragment measured 9.6 M tiles and +1.83 GiB. A 1 MiB fragment compresses to 1.5 KB of zlib and extrapolates to about 7 GiB; a 4 MiB fragment (6 KB compressed) to about 29 GiB. The trigger is one replaced prefab in a mod `Misc.bsa`, read on the main thread at `--hud` load.
- **MEDIUM: the HKX sample cap is still 128× the vanilla maximum (PAR-D1-2026-10-05-02).** #5006 closed the one-block bypass. The absolute `MAX_TRANSFORM_SAMPLES` (16 M) is still the only total cap, and the new census measures vanilla's maximum at 124,821 samples. A 279 KB all-static clip decodes to 15.7 M samples (597 MiB) in 0.4 s. A 1.66 MB clip that binds the vanilla 99-bone skeleton decodes to 603 MiB.

**Previous findings (9, issues #5006–#5016):**
- Six are closed, and each fix is in place in code: #5006, #5007, #5008, #5012, #5014, #5016.
- Three are still open and unchanged in code: #5009 (debug-load raw extract loop), #5010 (BGSM diagnostics silent on the template path) and #5011 (Oblivion EGM / BGSM sweep weakness).
- The #5007 fix introduced the false-positive include cycle in PAR-D6-2026-10-05-01.

**Crates swept:** `byroredux-bsa`, `byroredux-bgsm`, `byroredux-sfmaterial` (now including `index.rs`), `byroredux-hkx`, `byroredux-facegen`, `byroredux-menuxml` (parse side) and `byroredux-game-detect` (`vdf`, `steam`). Consumers traced: `byroredux/src/asset_provider/material/{cdb,merge,provider}.rs`, `asset_provider/animation.rs`, `npc_spawn/resumable/runtime.rs`, `hud.rs` and `debug_load.rs`.

**Unit tests** (`cargo test -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml,game-detect}`, isolated target dir): all green. **331 passed and 48 ignored, plus 6 doctests.**
- Breakdown: bsa 111+11 ignored (lib) plus 18 ignored in `tests/`; bgsm 33; sfmaterial 32+6; hkx 24; facegen 31; menuxml 39; game-detect 55.
- `cargo test -p byroredux --bin byroredux -- asset_provider convert_hkx_clip` (toolchain 1.96.0): 214 passed, 9 ignored.

**Real-data suites run** (strict lane: `BYROREDUX_REQUIRE_GAME_DATA=1`, `--release`, `--test-threads=1`, one crate at a time, matching the CI `parsers` job, with `BYROREDUX_SKYRIMLE_DATA` set): **48 tests, all green.**
- **bsa: 29.** 11 in-crate, 11 `ba2_real`, 6 `bsa_real`, 1 `csg_real`.
- **bgsm: 3.** FO4: 6,616 / 6,616 BGSM and 283 / 283 BGEM. FO76: 29,989 / 29,991, plus 2 JSON files in the allowed bucket.
- **sfmaterial: 3.**
  - Exact pin of 97 classes and 1,438,780 values.
  - `MaterialIndex`: 500,403 keys, built in 1.6 s with a 244 MB high-water mark.
  - DLC SFBGS007: 500,385 keys.
- **hkx: 3.**
  - Cart idle and LE/SE parity.
  - New census: 6,126 clips. Maximums: `max_frames_per_block` 256, frames 1,471, blocks 15, samples 124,821.
- **facegen: 4.**
- **menuxml: 6.**

**Real-data suites skipped:** none in the six parser crates. The byroredux `--ignored` archive suites were not re-run; their code is unchanged since 09-29.

**Probes:** the HIGH, the MEDIUM and PAR-D6-2026-10-05-01 were each reproduced with a throw-away probe crate outside the repo. The probes live in the session scratchpad (`hkxprobe`, `menuprobe`, `cdbprobe`). Each uses path dependencies on the repo crates and the isolated target `/mnt/data/tmp/parsers-probe-target`, and drives the real public API.

**Cross-audit dedup:**
- Checked against `/tmp/audit/issues.json`, closed-issue searches, and today's `AUDIT_{CONCURRENCY,ECS,NIF,PERFORMANCE,RENDERER,SAFETY}_2026-10-05.md`.
- **PERF-D3-2026-10-05-01** already owns the `MaterialIndex` main-thread build cost and multi-index residency. It is not re-filed here. One data point for it: today's build measured a 244 MB high-water mark, including the 105 MB CDB blob, against the 468–470 MB quoted in `real_cdb.rs` and `cdb.rs`. Two indexes plus extraction peaked at 340 MB RSS in `cdbprobe`.
- The `ECS-2026-10-05` routing note covers the flaky `starfield_mat` test that shares a cache key.
- **#5210** (CDB spec and doc rot) covers the stale "Phase 2 should return `Merged`" comment in `merge.rs`.
- **#5232** is a NIF-harness skip-green and is not in a parser crate.

---

## Findings

### PAR-D1-2026-10-05-01: The MenuXml include budget counts splices, not bytes, so one small compressed fragment expands into gigabytes of tiles
- **Severity**: HIGH
- **Dimension**: Size Discipline
- **Location**: `crates/menuxml/src/parse.rs:588` (`MAX_INCLUDE_SPLICES`), `crates/menuxml/src/parse.rs:597-637` (`IncludeState`), `crates/menuxml/src/parse.rs:710-800` (`splice_include`). Production fetch: `byroredux/src/hud.rs:209-211` (`HudAssets::menu_xml` → `extract_or_warn`). Parse sites: `crates/menuxml/src/menu.rs:147,170,316`.
- **Status**: NEW. This is a residual of #4650: the exponential blow-up is fixed, and the remaining growth is 256× linear.
- **Trigger Input**: a menu document carrying up to 256 `<include src="g.xml"/>` elements, plus a prefab `menus\prefabs\g.xml` whose body is N copies of `<rect/>`. Either file can come from a mod `Oblivion - Misc.bsa` / `Fallout - Misc.bsa`, where the last-listed archive wins. The repetitive body compresses about 700:1.
- **Description**:
  - #4650 bounded includes by nesting (`depth + 1`) and by a global count of 256 splices. #5007 then made the fetch O(1) per path.
  - Nothing bounds the *bytes* spliced or the *tiles* produced. Each splice re-parses the whole fragment into fresh `TileSeed`s, so output is up to 256 × the fragment.
  - The 48-level tile cap limits depth only. There is no total tile cap.
  - #5007's cache also keeps every distinct fetched fragment's bytes alive for the whole `parse_document` call. Before #5007, each fragment was dropped after its splice.
- **Evidence** (probe `menuprobe`, release build; the root carries 300 includes and the budget stops at 256):
  ```
  fragment   4 KiB (  585 tiles): fetches 1, document tiles   149,761, parse  20 ms, VmHWM +28 MiB
  fragment  16 KiB ( 2340 tiles): fetches 1, document tiles   599,041, parse  82 ms, VmHWM +114 MiB
  fragment  64 KiB ( 9362 tiles): fetches 1, document tiles 2,396,673, parse 328 ms, VmHWM +458 MiB
  fragment 256 KiB (37449 tiles): fetches 1, document tiles 9,586,945, parse 2.08 s, VmHWM +1,834 MiB
  zlib -9: a 1 MiB fragment = 1,559 B; a 4 MiB fragment = 6,130 B
  ```
  Growth is exactly linear, at about 7.2 MiB resident per KiB of fragment. A 1 MiB fragment therefore costs about 7.3 GiB and a 4 MiB fragment about 29 GiB.
- **Impact**:
  - Out-of-memory, or a multi-second stall, on the main thread at `--hud` load. The HUD then evaluates and lays out every tile each frame.
  - The trigger is a few KB inside a mod archive.
  - The skill's severity rule applies: an OOM reachable from an archive or mod file is HIGH.
  - Vanilla corpora are unaffected: 89 documents and 1,868 tiles.
- **Related**: #4650, #5007, PAR-D1-2026-09-21-04, PAR-D1-2026-09-29-02
- **Suggested Fix**:
  - Add a per-document byte budget for spliced text, for example a small multiple of the largest vanilla prefab, measured from the three corpora.
  - Or add a total tile cap in `parse_element_content`, or both.
  - Stop splicing and warn once (`warn_once("budget", …)`) when either budget is exhausted.
  - Pin it with the probe's 64 KiB × 256 case.

### PAR-D1-2026-10-05-02: After #5006, a 279 KB HKX clip still decodes to 15.7 M samples (597 MiB), because the absolute cap sits 128× above the vanilla maximum
- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**: `crates/hkx/src/animation.rs:52` (`MAX_TRANSFORM_SAMPLES` = 16,000,000), `crates/hkx/src/animation.rs:68` (`MAX_FRAMES_PER_BLOCK` = 256), `crates/hkx/src/animation.rs:355-384` (dimension gate), `crates/hkx/src/animation.rs:472-481` (sample expansion). Consumer: `byroredux/src/asset_provider/animation.rs:403` (`convert_hkx_clip`).
- **Status**: NEW. This is a residual after #5006, whose specific bypass is closed. It descends from #3011 and #4655.
- **Trigger Input**: an `hkaSplineCompressedAnimation` with `num_blocks = 4096`, `max_frames_per_block = 256`, `num_frames = 4096 × 255 + 1`, all masks 0, and blocks chained at exactly `mask_size` bytes each.
- **Description**:
  - #5006's cap makes each block cost its full `transform_count × 4` mask table. That ties output to file size at about 64 samples (about 2.5 KB decoded) per file byte, whatever the track count.
  - The only absolute bound remains `MAX_TRANSFORM_SAMPLES` = 16 M. The census added with the fix now measures vanilla's largest clip at **124,821** samples (`paired_dlc1seranaholdsvyrthur.hkx`, 201 tracks × 621 frames). The cap is therefore 128× vanilla.
  - The fix comment claims "decoded output stays proportional to file bytes". That is true only with a 2,500× constant.
- **Evidence** (probe `hkxprobe`, a hand-built 64-bit Havok 2010 packfile through `decode_spline_animation`):
  ```
  T=15 blocks=4096 mfpb=256: file   279,041 B -> Ok, 15 tracks x 1,044,481 frames = 15,667,215 samples (597 MiB) in 0.40 s
  T=15 blocks=4096 mfpb=257: file   279,041 B -> Err(InvalidData("unsupported spline clip dimensions"))
  T=99 blocks=4096 mfpb=40:  file 1,655,297 B -> Ok, 99 tracks x 159,745 frames = 15,814,755 samples (603 MiB) in 0.46 s
  VmHWM: 735,460 kB
  ```
- **Impact**:
  - The impact is the same as PAR-D1-2026-09-29-01 and the original #4655: about 600 MiB peak during the decode.
  - The 99-track form binds to the vanilla skeleton, so `convert_hkx_clip` keeps it as keys for the session (about 2 GB per clip, as measured on 09-21).
  - It runs on the main thread, on Skyrim only. The trigger is one mod-overridden `.hkx` of an ordinary size: a 279 KB to 1.7 MB file is unremarkable for a paired animation.
- **Related**: #5006, #4655, #3011, PAR-D1-2026-09-29-01
- **Suggested Fix**:
  - Set `MAX_TRANSFORM_SAMPLES` from the census, for example 1 M (8× the vanilla maximum).
  - Have `skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings` assert `max_samples` against it, so a vanilla re-export that grows trips the real-data gate.
  - Add the probe's 279 KB case as a unit test.

### PAR-D2-2026-10-05-01: `cdb_material_index` drops archive open/extract errors with `.ok()?` and memoises the failure silently
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `byroredux/src/asset_provider/material/cdb.rs:111-113`; log strings at `cdb.rs:117` and `cdb.rs:124`
- **Status**: NEW (introduced by `224a19372` / `18fce7e43`)
- **Trigger Input**: a discovered `materialsbeta.cdb` whose archive cannot be re-opened, or whose entry fails to re-extract, at first `.mat` lookup. Examples: an archive replaced mid-session, an I/O error, or a corrupt chunk past the header that the discovery probe read.
- **Description**:
  - The lazy build runs `let archive = Archive::open(source).ok()?; let bytes = archive.extract(inner).ok()?;`. This is the raw-`.ok()` shape #4658 removed from the providers.
  - Only the `MaterialIndex::build` `Err` arm warns. The `None` is then cached for the process, so every `.mat` lookup against that CDB silently degrades to the Phase-1 PBR fallback with no log line.
  - Separately, both of the function's log messages contain 22 embedded spaces (`"…material index built                      ({} keyed objects)"`), because a `\` line continuation was lost.
- **Evidence**: `cdb.rs:111-131`; `cat -A` of lines 117 and 124.
- **Impact**: an operator cannot tell "this CDB failed to load" from "this material is not in any CDB". The effect is diagnostic only: rendering falls back, and nothing crashes.
- **Related**: #4658, #3398, PERF-D3-2026-10-05-01
- **Suggested Fix**: match the open and extract results and `log::warn!` naming the source, path and error before memoising `None`, as `Archive::extract_or_warn` does. Restore the `\` continuations in both format strings.

### PAR-D2-2026-10-05-02: `MaterialIndex::build` degrades silently: a missing index, a row/instance count mismatch, and an unchecked `DBFileIndex` payload all return `Ok`
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs:191-226` (`build`), `crates/sfmaterial/src/index.rs:355-358` (`capture_instance`), `crates/sfmaterial/src/index.rs:503-595` (`stream_db_file_index`)
- **Status**: NEW (introduced by `224a19372`)
- **Trigger Input**: a CDB (mod or Creation) with any of: no `BSComponentDB2::DBFileIndex` instance; a `Components` table whose length differs from the number of instances after the index; or a `DBFileIndex` chunk of kind `DIFF`/`USRD`, or one carrying trailing bytes.
- **Description**: contract (c) requires a degraded decode to be a documented `Ok` with a warning, or an error. `build` breaks it in four ways:
  1. **No `DBFileIndex`.** `base` stays `None` and every instance is skipped, so the result is `Ok` with an empty index. The consumer only logs "material index built (0 keyed objects)" at info level.
  2. **Row/instance mismatch.** The `Components[j]` ↔ instance `j + 2` alignment is the load-bearing join (measured 1,438,778 / 1,438,778 on the base CDB), but `build` never compares `rows.len()` with the number of instances after the index. `capture_instance` drops an unmatched instance silently, and a one-row shift would attribute every texture to the wrong object.
  3. **Unchecked payload.** `stream_db_file_index` has no `ObjectTrailingBytes` check, unlike `consume_object`. It also reads a `DIFF`/`USRD` payload's inline fields in non-diff offset order.
  4. **Contextless error.** A second `DBFileIndex` fails as `WrongChunkType { wanted: Objt, got: Objt }`, which names nothing useful.
- **Evidence**: see the locations. `cdbprobe` shows the base and SFBGS007 indexes agreeing on the one sampled path that resolves, so vanilla shows no symptom.
- **Impact**: a non-vanilla CDB yields wrong or empty Starfield materials with no warning. Vanilla is clean.
- **Related**: #3398, PAR-D4-2026-10-05-01
- **Suggested Fix**:
  - At the end of `build`, return an error, or a typed warning the consumer logs, when no index was seen or when `rows.len()` differs from the count of instances after the index.
  - Check trailing bytes in `stream_db_file_index`.
  - Return a dedicated `DuplicateDbFileIndex` error variant.

### PAR-D3-2026-10-05-01: The sfmaterial skip path still walks fields in declaration order after #3398 moved the read path to `read_order`
- **Severity**: LOW
- **Dimension**: Version Gating
- **Location**: `crates/sfmaterial/src/reader.rs:796-835` (`skip_user_class_body` iterates `field_layout`), compared with `crates/sfmaterial/src/reader.rs:997-1050` (`read_user_class_body` iterates `read_order`). Contract stated at `crates/sfmaterial/src/types.rs:127-134`.
- **Status**: NEW (gap left by `224a19372`)
- **Trigger Input**: a CDB class whose declaration order differs from offset order **and** that has a variable-size inline field (`String`) or a chunk field (`List`/`Map`) among the reordered ones.
- **Description**:
  - `Class::read_order`'s own doc says "any sequential reader MUST walk this order".
  - The skip path still consumes inline bytes, and queues side chunks, in declaration order. The read path and the skip path can therefore disagree on how many bytes a field consumes, or on which `LIST`/`MAPC` belongs to which field.
  - In vanilla, `XMCOLOR` is the only divergent class. It is four `u8`s with no chunk fields, so both orders consume identical bytes, which is why nothing fails today.
- **Evidence**: the two loops above. `validate_instances` (the exact 97 / 1,438,780 pin) runs the skip path. `MaterialIndex::build` skips everything before the index and reads everything after it, so it uses both paths. No test pins skip ≡ read for a reordered class with a variable-size field.
- **Impact**: latent. A mod or Creation CDB with such a class would desync the skip path, giving `ObjectTrailingBytes`/`WrongChunkType`, or a skipped instance that consumes the wrong side chunks. The real-data pin validates only the skip path.
- **Related**: #3398, #4275
- **Suggested Fix**: iterate `read_order` in `skip_user_class_body`'s non-diff arm, as the read path does. Add a reordered-class fixture with a `String` field and a `List` field, and assert that skip and read consume the same bytes and chunks.

### PAR-D4-2026-10-05-01: The two real-data gates added since 09-29 can pass vacuously or under-assert
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/hkx/src/animation.rs:1236-1305` (`skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`); `crates/sfmaterial/tests/real_cdb.rs:161-187` (`material_index_dlc_cdb_resolves_the_base_corpus`)
- **Status**: NEW
- **Trigger Input**: n/a (gate weakness)
- **Description**:
  - **HKX census.**
    - It asserts only `max_mfpb <= 256`. It has no `clips > 0` check and no pinned clip or skip counts (6,126 / 1,573 today).
    - A `Packfile::parse` or extract regression therefore counts every file as "skipped" and passes with `max_mfpb = 0`.
    - It reads raw fields and never runs `decode_spline_animation`, so a vanilla clip rejected by any other gate stays invisible. That includes the `mask_size` tie and the block-chain checks.
    - The name promises "ceilings" (plural), but frames, blocks and samples are not asserted. Asserting samples is also the natural guard for PAR-D1-2026-10-05-02.
  - **SFBGS007 DLC test.**
    - `if !dlc.exists() { eprintln!(SKIP); return; }` passes green under `BYROREDUX_REQUIRE_GAME_DATA`, against the #3850 strict-lane contract.
    - It asserts only `.is_some()`. Every keyed object returns `Some`, even with zero textures, so the DLC CDB's `Components` ↔ stream alignment (PAR-D2-2026-10-05-02) is unverified.
  - **Doc drift.** The base test's comment records "468 MB HWM / 2.2 s"; this run measured 244 MB / 1.6 s on the same machine.
- **Evidence**: the test bodies; strict-lane output `census: 6126 clips, 1573 non-clip/undecodable files; max mfpb=256, max frames=1471, max blocks=15, max samples=124821`.
- **Impact**: a decode regression in HKX, or a misaligned DLC CDB, can pass the nightly `parsers` lane.
- **Related**: #3850, #4660, #5006, #5011, PAR-D2-2026-10-05-02
- **Suggested Fix**:
  - **Census:** pin the clip count, bound the skip count, decode each clip through `decode_spline_animation` and count the rejects (0 expected), and assert the frame, block and sample maximums against the gate constants.
  - **DLC test:** route the DLC skip through `require_game_data`. Assert the five Nightmare slots as the base test does.

### PAR-D6-2026-10-05-01: #5007's pre-fetch cycle check rejects a distinct higher-priority include whenever a lower-priority spelling is on the stack
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/menuxml/src/parse.rs:729-749` (`splice_include` candidate keys and cycle check)
- **Status**: NEW (introduced by `2465740cc`, the #5007 fix)
- **Trigger Input**: a fragment resolved via the 2nd or 3rd candidate spelling (e.g. authored `menus\x.xml`) that includes `x.xml`, while a *different* file exists at the first-priority `menus\prefabs\x.xml`.
- **Description**:
  - #5007 tests **all three** candidate keys against the nesting stack before fetching: `keys.iter().any(|k| includes.seen.…contains(k))`.
  - Before #5007, the cycle key was the *resolved* candidate, meaning the first spelling that exists. If any lower-priority spelling of an include equals an ancestor, the include is now rejected as a cycle, even though resolution would have picked a different, higher-priority file.
- **Evidence** (probe `menuprobe --bin fp`):
  - Setup: `menus\x.xml` = `<rect name="outer"><include src="x.xml"/></rect>` and `menus\prefabs\x.xml` = `<rect name="prefab_x"/>`. The root includes `menus\x.xml`.
  - Result: the tiles are `["root", "outer"]`. `prefab_x` is dropped with the warning "include cycle on 'x.xml'".
  - Under the pre-#5007 logic, `menus\prefabs\x.xml` resolves first, is not on the stack, and is spliced.
- **Impact**: a mod menu or prefab silently loses content when it mixes `menus\`-relative and bare spellings. Vanilla includes resolve via `menus\prefabs\` and pass, but the corpus prints its 1,868-tile total without pinning it.
- **Related**: #5007, #4650, PAR-D1-2026-10-05-01
- **Suggested Fix**:
  - Resolve first through the cache. The cache already makes a repeat probe O(1), so moving the fetch back ahead of the check does not reintroduce the quadratic cost.
  - Then cycle-check only the resolved key.
  - Pin the probe case, and pin the vanilla tile total in `vanilla_corpus.rs`.

---

## Prior-finding verification (AUDIT_PARSERS_2026-09-29)

| Prior finding | Issue | State now |
|---|---|---|
| D1-01 HKX one-block bypass | #5006 (closed) | Fixed: `MAX_FRAMES_PER_BLOCK` 256 rejects the probe's mfpb=257 form. Residual absolute cap: PAR-D1-2026-10-05-02 |
| D1-02 MenuXml quadratic fetch | #5007 (closed) | Fixed: budget-first check and a per-path cache; fetches = 1 in every probe. The new cycle over-approximation is PAR-D6-2026-10-05-01; the splice amplification is PAR-D1-2026-10-05-01 |
| D2-01 BA2 DX10 debug panics | #5008 (closed) | Fixed: `chunk_hdr_len != 24` is `InvalidData` naming the record; the `start_mip` check only warns. No `debug_assert!` on a file field remains in any parser crate |
| D2-02 debug-load raw loop | #5009 (OPEN) | Unchanged (`byroredux/src/debug_load.rs:167-202`) |
| D3-01 BGSM diagnostics silent | #5010 (OPEN) | Unchanged (`crates/bgsm/src/template.rs:257` still calls `parse_bgsm`) |
| D4-01 Oblivion EGM / BGSM sweeps | #5011 (OPEN) | Unchanged (`parse_real_facegen.rs:296-305` still skips green) |
| D5-01 spec-off BGSM roughness | #5012 (closed) | Fixed: the #3639 fallback is gated on `leaf.specular_enabled` (`merge.rs:1349`) |
| D5-02 FaceGen docs | #5014 (closed) | Fixed: EGT R → height, C → width in both code and test; docs corrected |
| D6-01 `installdir` on Windows | #5016 (closed) | Fixed: the string predicate `is_bare_install_dir`, pinned on every host |

## Gate Matrix

| Crate / format | Size caps | Error policy | Version gate | Corpus sweep (strict lane) | CI lane |
|---|---|---|---|---|---|
| bsa: BSA | ✓ `checked_entry_count` / `checked_chunk_size` / `inflate_bounded(_zlib)`; file-relative `capacity_hint` | ✓ labelled `Err`; consumers via `extract_or_warn` (except `debug_load.rs`, #5009) | ✓ {103,104,105}; LZ4 **frame** on v105 | Oblivion brute force; SSE Meshes0; FNV; FO3 open+list only | ✓ nightly `parsers` |
| bsa: BA2 | ✓ per-chunk and per-record caps; DX10 pitch saturates | ✓ path-named; `chunk_hdr_len` hard `Err` (#5008) | ✓ {1,2,3,7,8}; method 0/3 | FO4 v8 brute force + Textures1; FO76 40 open + 1 DX10; Starfield | ✓ nightly |
| bsa: CSG / UVD | ✓ | ✓ | magic | FO4 object 0 (no DLC CSGs); UVD has no consumer | ✓ nightly / n/a |
| bgsm | ✓ string ≤ remaining; template depth 16 + visited set | ✓ offset `Err`; lossy strings | per-field gates; **diagnostics silent on the BGSM template path** (#5010) | FO4 100%, FO76 100% + 2 JSON; **no diagnostic asserts** (#5011) | ✓ nightly |
| sfmaterial (tree) | ✓ chunk/field clamps; nesting 64 | ✓ typed errors | ✓ v4, BE rejected, vocabulary pins; **skip path in declaration order** (D3-01) | exact 97 / 1,438,780 (skip path) | ✓ nightly |
| sfmaterial (`MaterialIndex`) | per-instance decode; `stream_list` count ≤ payload | **✗ silent degradation** (D2-02); consumer `.ok()?` (D2-01) | inherits the tree reader | base: 500,403 keys + 5 slots pinned; DLC: **skip-green, `.is_some()` only** (D4-01) | ✓ nightly |
| hkx | strings ≤ 256 B, `max_frames_per_block` ≤ 256; **absolute samples cap 128× vanilla** (D1-02) | ✓ labelled; annotation skip | ✓ #4332 gates, pointer-derived layout | SE cart + LE/SE parity; **census asserts mfpb only** (D4-01) | ✓ nightly |
| facegen | ✓ caps + exact size | ✓ (consumer logs at debug) | ✓ magics; int16 EGM; planar i8 EGT; R/C fixed | FNV/FO3 pinned; Oblivion EGM weak (#5011) | ✓ nightly |
| menuxml | tile depth 48, expression 64, splice count 256; **no byte/tile budget** (D1-01) | ✓ char-safe scanner; warn-once per cause | n/a | Oblivion, FO3, FNV (tile total printed, not pinned) | ✓ nightly |
| game-detect | ✓ VDF depth 32 | ✓ malformed rejected; `installdir` containment on every host | n/a | unit only | ✓ unit tests in `ci.yml` |

## Guards confirmed live (not `#[ignore]`d, passing this run)

- **Size ceilings:**
  - `entry_count_rejects_attacker_u32_max`, `over_ratio_payload_is_rejected_at_the_ceiling`, `malicious_bsa_folder_count_u32_max_rejected`, `malicious_file_count_u32_max_rejected_before_allocation`.
  - `lz4_flex_is_pinned_to_the_safe_decoder`, `lz4_decompress_is_panic_guarded`, `oversized_read_len_is_rejected_before_it_is_reserved`.
  - `decode_spline_animation_rejects_a_sample_count_bomb`, `decode_spline_animation_rejects_frames_beyond_the_declared_blocks`, `decode_spline_animation_rejects_a_block_claiming_its_whole_clip`.
  - `rejects_morph_count_over_cap`, `parse_with_limits_rejects_object_tree_before_materialising_it`.
- **Nesting and includes:** `include_cycles_terminate`, `self_including_fragment_costs_one_fetch`, `budget_exhausted_includes_never_fetch`, `resolve_breaks_self_reference_cycle`, `self_referential_struct_is_rejected_instead_of_overflowing_the_stack`, `pathological_nesting_hits_the_depth_cap`.
- **Error semantics:** `dx10_open_rejects_a_non_24_chunk_hdr_len_as_invalid_data`, `dx10_open_tolerates_non_monotonic_start_mip_without_panicking`, `extract_dx10_rejects_a_short_non_final_chunk_naming_the_entry`, `short_decode_stays_ok_for_the_shipped_padding_deltas`, `corrupt_adler32_trailer_recovers_via_raw_deflate`, `corrupt_deflate_body_still_errors`, `read_annotations_skips_an_out_of_range_time_and_keeps_the_rest`.
- **Version and layout:**
  - `synthetic_v105_block_codec_payload_is_rejected_by_frame_reader`, `unknown_version_rejected`, `v3_unknown_compression_method_rejected`, `build_dds_header_is_148_bytes`.
  - `chunk_type_recognized_set_is_pinned`, `builtin_type_recognized_set_is_pinned`, `class_flags_known_mask_is_pinned`, `read_order_reorders_xmcolor_shape`.
  - `rejects_packfiles_that_are_not_havok_2010_msvc_layout`, `layout_walk_reproduces_the_skyrim_se_offsets`.
  - `rejects_size_count_mismatch`, `non_square_header_maps_rows_to_height_and_columns_to_width`.
- **Paths and parsing:** `siblings_*`, `every_content_provider_resolves_collisions_last_wins` (within the 214 `asset_provider` tests), `malformed_documents_are_rejected_rather_than_half_read`, `only_a_bare_directory_name_is_a_valid_installdir`, `pathological_nesting_hits_the_depth_cap`.
- There is exactly one implementation each of `MAX_ENTRY_COUNT`, `MAX_CHUNK_BYTES` and `MAX_RECORD_TOTAL_BYTES` (`crates/bsa/src/safety.rs`).

## Publishing

`/audit-publish docs/audits/AUDIT_PARSERS_2026-10-05.md`

| Finding | Suggested labels |
|---|---|
| PAR-D1-2026-10-05-01 | `high` `bug` `ui` `memory` `safety` |
| PAR-D1-2026-10-05-02 | `medium` `bug` `animation` `memory` `safety` `game:skyrim` |
| PAR-D2-2026-10-05-01 | `low` `bug` `nifal` `game:starfield` |
| PAR-D2-2026-10-05-02 | `low` `bug` `nifal` `game:starfield` |
| PAR-D3-2026-10-05-01 | `low` `bug` `nifal` `game:starfield` |
| PAR-D4-2026-10-05-01 | `low` `bug` `test-gap` |
| PAR-D6-2026-10-05-01 | `low` `bug` `ui` |

Label caveat for the publish summary: CDB reader findings map to `nifal`, as the skill directs for BGSM and CDB. The sfmaterial crate has no label of its own.
