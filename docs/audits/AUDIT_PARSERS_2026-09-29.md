# Parser Discipline Audit: 2026-09-29

**HEAD**: 9fcfdc3fc · **Baseline**: `AUDIT_PARSERS_2026-09-21.md` (HEAD f97775ca8) · **Audited**: Dims 1–6 (Size Discipline, Error Semantics, Version Gating, Corpus Gates, Decode-Consumer Wiring, I/O & Paths). All six were deep-audited, because every dimension's Paths changed since the baseline (the 25 fixes #4648–#4673, plus positional archive reads `1b8b21f3f`, `declared_size` `dbc1c88c2`, texture prefetch and the player/gear commits) · **Unchanged since baseline (skimmed)**: none

This run is one leg of `/audit-suite --preset comprehensive`. The suite said not to use sub-agents, so every dimension was analysed in this session, one after another. Per-dimension scratch files are at `/tmp/audit/parsers/dim_{1..6}.md`. The skill's Phase-3 `rm -rf /tmp/audit/parsers` was **deliberately not run**, because the suite orchestrator reads those files.

## Executive Summary

**Findings:** 0 CRITICAL · 0 HIGH · **3 MEDIUM** · 6 LOW (9 total). Six are NEW. Three are regressions, meaning the fix for a closed issue is incomplete: #4655, #4664 and #4673.

**What the MEDIUMs are:**
- **HKX sample bomb is still open (Regression of #4655).** The new check ties the frame count to the file's blocks, but a single block may declare up to 4,096 frames. The exact clip from the 2026-09-21 proof of concept still decodes: 16,873 bytes become 16M samples, with a 660 MB high-water mark.
- **MenuXml include fetch is quadratic (NEW).** The #4650 fix moved the include cycle check to *after* the archive fetch, and the splice budget is also checked only after the fetch. A 440 KB self-including fragment costs 20,001 fetch-and-inflate rounds and 8.8 GB of inflate. Doubling the fragment size roughly quadruples the time.
- **BA2 DX10 panics in debug builds (NEW).** Two `debug_assert!`s run on file-controlled fields: `chunk_hdr_len` and `start_mip` order. A crafted mod BA2 therefore aborts `cargo run` at boot. Release builds only warn.

**Previous findings (25, issues #4648–#4673):** all 25 issues are closed. The fixes were re-verified in code:
- 22 are in place.
- **#4655** is incomplete (PAR-D1-2026-09-29-01).
- **#4664** fires only on a dormant fallback path (PAR-D3-2026-09-29-01).
- **#4673** is bypassable on Windows (PAR-D6-2026-09-29-01).

**Crates swept:** `byroredux-bsa` (BSA, BA2, CSG, UVD, naming, safety, read_at), `byroredux-bgsm`, `byroredux-sfmaterial`, `byroredux-hkx`, `byroredux-facegen`, `byroredux-menuxml` (parse side plus the `menu.rs` load path), and `byroredux-game-detect` (`vdf`, `steam`). Consumers were traced in `byroredux/src/asset_provider/{archive,texture,texture_prefetch,animation,material/{provider,merge,cdb}}.rs`, `byroredux/src/npc_spawn/resumable.rs`, `byroredux/src/hud.rs` and `byroredux/src/debug_load.rs`.

**Unit tests** (Phase 1, `cargo test -j 4 -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml,game-detect}`, isolated target dir): all green. 313 passed and 45 ignored, plus 6 doctests.
- Breakdown: bsa 108+1, bgsm 33, sfmaterial 29+6, hkx 23, facegen 30, menuxml 34, game-detect 49.
- `cargo test -p byroredux --bin byroredux -- asset_provider convert_hkx_clip`: 203 passed, 9 ignored.

**Real-data suites run** (strict lane: `BYROREDUX_REQUIRE_GAME_DATA=1`, `--release`, `--test-threads=1`, one crate at a time, the same command as the CI `parsers` job): **45 tests, all green.**
- **bsa: 29.**
  - Oblivion v103 brute force: 17 archives, 147,629 files, 0 errors.
  - SSE Meshes0 v105: 18,862 NIFs, 0 errors.
  - FO4 Meshes.ba2 v8: 34,995 NIFs, 0 errors.
  - `declared_size` equals `extract` on FO4 Meshes (42,426) plus Textures1 (DX10), and on Oblivion Meshes (20,182): 0 mismatches.
  - FO76: 40 BA2s, 679,240 entries.
  - FO3: 16 BSAs.
  - Starfield: full sweep.
- **bgsm: 3.** FO4 6,616 BGSM and 283 BGEM at 100%. FO76 29,989/29,991, plus 2 JSON-text files in the allowed bucket.
- **sfmaterial: 1.** CDB exact pin of 97 classes and 1,438,780 values.
- **hkx: 2.** SE cart family and the LE/SE parity test.
- **facegen: 4.**
- **menuxml: 6.**
- The byroredux `archive_precedence` suite (`--ignored`) added 3 more.

**Real-data suites skipped:** the byroredux cart and draugr installs (about 1 GB resident each), `facegen_texture_fallback`, `fo4_palette_corpus` and `default_sound_candidates`. All four are outside the reader-discipline scope.

**Probes:** every MEDIUM claim, and the vanilla counts in PAR-D5-2026-09-29-01, were reproduced with a throw-away probe crate outside the repo. It lives at `/tmp/audit/parsers/probe`, uses path dependencies on the repo crates and builds into the isolated target `/mnt/data/tmp/parsers-probe-target`. It builds each crafted input and drives the real public API.

**Cross-audit dedup:**
- Checked against `/tmp/audit/issues.json`, closed-issue searches, and today's `AUDIT_{CONCURRENCY,ECS,NIF,NIFAL,PERFORMANCE,RENDERER,SAFETY}_2026-09-29.md`. There is no overlap.
- `AUDIT_SAFETY_2026-09-29` confirms the LZ4 `safe-decode` pin. This audit agrees.
- `PERF-D7-2026-09-29-02` (the `GearImportLoader` archive re-open) is a performance finding and was not re-filed.
- #4752 (the debug port reads arbitrary local files, OPEN) is related to PAR-D2-2026-09-29-02 but is not the same defect.

---

## Findings

### PAR-D1-2026-09-29-01: HKX #4655's frame bound can be bypassed with one block whose `max_frames_per_block` is about `num_frames`, so the 17 KB → 16M-sample clip still decodes
- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**: `crates/hkx/src/animation.rs:337-367` (dimension gate), `crates/hkx/src/animation.rs:450-459` (sample expansion)
- **Status**: Regression of #4655 (incomplete fix, `a323138df`)
- **Trigger Input**: an `hkaSplineCompressedAnimation` with `num_blocks = 1` and `max_frames_per_block = num_frames + 1` (4,096 at most), where `transform_count × num_frames` sits just under 16,000,000 and all masks are 0 (every track static).
- **Description**:
  - The #4655 gate is `num_frames <= num_blocks * (max_frames_per_block - 1) + 1`, with `max_frames_per_block <= 4096` and `num_blocks <= 4096`.
  - Nothing ties a block's frame count to the bytes that block carries. With all-static masks, a block costs only `transform_count * 4` mask bytes, whatever its frame count.
  - The regression test `decode_spline_animation_rejects_frames_beyond_the_declared_blocks` uses `max_frames_per_block = 16`, so only the easy case is pinned.
  - The fix commit states that the 17 KB / 610 MiB clip is closed. It is not.
- **Evidence** (probe `hkx-frames`; the builder mirrors `packfile::fixtures::PackfileBuilder`'s 64-bit layout):
  ```
  tracks=4096 frames=3906 blocks=1 mfpb=3907: file 16873 B -> Ok, 4096 tracks x 3906 frames = 15998976 samples in 1.20 s
  tracks=99 frames=161616 blocks=40 mfpb=4096: file 16641 B -> Ok, 99 tracks x 161616 frames = 15999984 samples in 1.07 s
  tracks=8 frames=3906 blocks=2 mfpb=16: file 561 B -> Err(InvalidData("unsupported spline clip dimensions"))
  VmHWM: 659668 kB
  ```
- **Impact**: the same as the original PAR-D1-2026-09-21-02.
  - The decode itself peaks at about 610 MiB.
  - The 99-track form binds fully to the vanilla 99-bone skeleton. `convert_hkx_clip` then keeps about 2 GB of keys per clip for the session.
  - It runs on the main thread (`byroredux/src/asset_provider/animation.rs`) and affects Skyrim only.
  - The trigger is one mod-overridden `.hkx`, since the last-listed archive wins.
- **Related**: #4655, #3011, PAR-D1-2026-09-21-02
- **Suggested Fix**:
  - Cap `max_frames_per_block` at a measured vanilla ceiling, measured with a quick census of Skyrim SE `Animations.bsa`. The Havok default is 256.
  - Or bound total output samples by the spline-data bytes actually present.
  - Add the `blocks=1, mfpb=3907` probe as the regression case.

### PAR-D1-2026-09-29-02: MenuXml fetches an `<include>` before the cycle and budget checks, so fetch and inflate work grows quadratically with fragment size
- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**: `crates/menuxml/src/parse.rs:676-719` (`splice_include`); production source `byroredux/src/hud.rs:205-207`
- **Status**: NEW (introduced by `20faaf89b`, the #4650 fix)
- **Trigger Input**: a prefab fragment `menus\prefabs\g.xml` whose body is N copies of `<include src="g.xml"/>`, meaning N self-includes. The same happens with any include after the 256-splice budget is spent.
- **Description**:
  - `splice_include` first runs `candidates.iter().find_map(|p| src.menu_xml(p))`. Only after that does it check `seen_includes` for a cycle and `splice_budget == 0`.
  - The production `HudAssets::menu_xml` is `self.misc.extract_or_warn(path)`: a full BSA extract plus inflate, with no cache.
  - Before #4650, the cycle key was the authored spelling and was checked before any fetch, so a self-include cost nothing.
  - Now every include element in every parsed fragment costs one fetch. That includes rejected cycles and everything after the budget is exhausted.
  - A fragment of S bytes therefore costs about S/22 fetches of itself, or O(S²) inflate. The budget allows up to 256 such fragments.
- **Evidence** (probe `menuxml-selfinc`, release build, each fetch inflating a zlib copy the way a BSA extract does):
  ```
  self-include fragment  22023 B ( 1000 includes): fetches  1001, inflated    22.0 MB,   9 ms
  self-include fragment 110023 B ( 5000 includes): fetches  5001, inflated   550.2 MB, 161 ms
  self-include fragment 440023 B (20000 includes): fetches 20001, inflated  8800.9 MB, 2.30 s
  ```
  - Four times the size costs about fourteen times the time. The compressed payload stays tiny because the text repeats, so the archive itself stays small.
  - Each cycle also logs one `warn!`, which gives 20,000 log lines for the last case.
- **Impact**: the `--hud` launch hangs on the main thread, with minutes of stall per multi-MB fragment. The trigger is a replaced or modified `Oblivion - Misc.bsa` / `Fallout - Misc.bsa`. Vanilla does not trigger it; the Oblivion, FO3 and FNV corpora pass.
- **Related**: #4650, PAR-D1-2026-09-21-04
- **Suggested Fix**:
  - Return early when `*splice_budget == 0` before touching the source.
  - Cycle-check every candidate spelling against `seen_includes` before fetching.
  - Cache fetched fragment bytes per `parse_document`, so a repeated include is a map hit.

### PAR-D2-2026-09-29-01: BA2 DX10 open panics in debug builds on two file-controlled fields
- **Severity**: MEDIUM
- **Dimension**: Error Semantics
- **Location**: `crates/bsa/src/ba2.rs:653-658` (`debug_assert_eq!(chunk_hdr_len, 24, …)`), `crates/bsa/src/ba2.rs:771-777` (`debug_assert!(monotonic, …)` over chunk `start_mip`)
- **Status**: NEW
- **Trigger Input**: a DX10 BA2 record whose `chunk_hdr_len` (bytes 14..16 of the record) is not 24, or whose chunks' `start_mip` is not non-decreasing.
- **Description**:
  - Both asserts sit in `read_dx10_records`, which runs from `Ba2Archive::open`.
  - Release builds compile them out and warn instead (`ba2.rs:672-681`, `:778-787`).
  - Debug builds panic. `Archive::open` runs at boot from `open_with_numeric_siblings` on the main thread, with no `catch_unwind`.
  - The asserts are deliberate: #1079 and #1176 chose "`debug_assert` catches it in dev". That still violates the reader contract "malformed bytes give `Err`, never a panic".
  - #4656 fixed the sibling debug-only panic (a `u32` overflow) in this same file at MEDIUM.
- **Evidence** (probe `ba2-dx10-asserts`, a one-record DX10 v1 BA2):
  ```
  debug:   control: open Ok (1 files)
           chunk_hdr_len=32: PANIC: assertion `left == right` failed: BA2 DX10 record has chunk_hdr_len=32 (expected 24) …
           start_mip 1,0: PANIC: BA2 DX10 chunks non-monotonic on start_mip: [1, 0] — synthesized DDS header would misdescribe payload
  release: control / chunk_hdr_len=32 / start_mip 1,0: open Ok (1 files) each
  ```
- **Impact**: a debug-build engine (`cargo run`, the documented developer path) aborts at startup on a third-party repacked or crafted mod BA2. Release builds are unaffected.
- **Related**: #4656, #4155, #1079, #1176, #1825
- **Suggested Fix**:
  - Delete both `debug_assert`s; the warns already cover them.
  - Or make `chunk_hdr_len != 24` an `InvalidData` naming the record, because the reader cannot parse any other length correctly anyway.
  - Add the two probe cases as unit tests (`read_dx10_records` needs a `ReadAt`/`Read` seam, or build a temp file).

### PAR-D2-2026-09-29-02: The debug-load NIF resolver keeps the pre-#4658 raw extract loop, with first-listed-wins order and BSA-only opening
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `byroredux/src/debug_load.rs:185-212` (`resolve_nif_bytes`)
- **Status**: NEW (owner `/audit-tooling`, per `_audit-owners.md`)
- **Trigger Input**: a `byro-dbg` NIF load request whose path is present but corrupt in a `--bsa` archive, or any `--bsa` that is a BA2.
- **Description**:
  - The loop is `for window in args.windows(2) { … BsaArchive::open(archive_path) … if let Ok(data) = archive.extract(path) { return Some(data) } }`.
  - It has the exact shape #4658 removed from the providers: non-`NotFound` errors are dropped silently.
  - It is first-listed-wins, which inverts #3637.
  - It re-opens (and re-indexes) every `--bsa` per request.
  - A BA2 `--bsa` fails `BsaArchive::open` with "not a BSA file", which is logged as a failure to open.
- **Evidence**: see the location. `every_content_provider_resolves_collisions_last_wins` scans providers only, so it cannot see this loop.
- **Impact**: debug tooling only. A present-but-corrupt override reads as "not found", or silently resolves to an earlier archive's copy. On FO4 or Starfield (`--bsa *.ba2`), archive-backed debug NIF loads never resolve.
- **Related**: #4658, #3637, #4752 (the same function's path policy, a security issue, OPEN)
- **Suggested Fix**: resolve through `build_texture_provider(&args).extract_mesh(path)`, which the cell-request branch already builds, or through `extract_first` over `Archive::open`.

### PAR-D3-2026-09-29-01: #4664/#4672 BGSM diagnostics never fire on the production path, because the template resolver parses with the non-diagnostic `parse_bgsm`
- **Severity**: LOW
- **Dimension**: Version Gating
- **Location**: `crates/bgsm/src/template.rs:27`, `crates/bgsm/src/template.rs:257`; `byroredux/src/asset_provider/material/provider.rs:358-411`
- **Status**: Regression of #4664 (incomplete fix, `f97a5d183`). The same gap covers #4672's lossy-string warning.
- **Trigger Input**: a BGSM leaf or template with trailing bytes, `version > 22`, or a non-UTF-8 string.
- **Description**:
  - `MaterialProvider::resolve_bgsm` calls `self.bgsm_cache.resolve(&mut reader, &key)`.
  - `TemplateCache::resolve_depth` parses every file in the chain with `parse_bgsm(&bytes)`, which discards `ParseDiagnostics`.
  - `parse_bgsm_diag` and its three `warn!`s (lossy strings, unconsumed bytes, version over ceiling) exist only in the `ResolveError::DepthLimit` recovery arm. The same function describes that arm as "effectively dormant", since vanilla depth is at most 3.
  - The BGEM path (`provider.rs:614`) is correct.
- **Evidence**: the commit message of `f97a5d183` says "the production provider warns once per file with the path". In the code, a normal BGSM load never reaches `parse_bgsm_diag`.
- **Impact**: layout drift, post-v22 content or lossy paths in BGSM files (leaf or template) decode silently. This is the exact silence #4664 was filed to end. Vanilla is zero-noise either way.
- **Related**: #4664, #4672, PAR-D4-2026-09-29-01
- **Suggested Fix**:
  - Have `TemplateCache` call `parse_bgsm_diag` and surface the diagnostics per resolved file, either returned alongside the `ResolvedMaterial` or through a warn callback on `TemplateResolver`.
  - Add a provider-level test with a trailing-junk BGSM.

### PAR-D4-2026-09-29-01: Two sweeps are weaker than their issues claim: the Oblivion EGM test skips green under REQUIRE and pins nothing, and the BGSM sweeps never check the #4664 drift signals
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/facegen/tests/parse_real_facegen.rs` (`parse_vanilla_headhuman_egm_oblivion`); `crates/bgsm/tests/parse_all.rs:242`, `crates/bgsm/tests/parse_all.rs:351`
- **Status**: NEW
- **Trigger Input**: n/a (gate weakness).
- **Description**:
  - **Oblivion EGM test** (added by #4665):
    - It resolves `BYROREDUX_OBLIVION_DATA` by hand. A missing directory, or a set-but-wrong override, gives `eprintln!("skipping…"); return;` with no `require_game_data`, so it is green in the strict lane.
    - It asserts only `num_vertices > 0` and `morphs > 0`.
    - Its output still says "record these in the EXPECT table". This run measured 1,736 verts and 80 morphs.
    - It carries none of the #4653 int16 checks that the FNV/FO3 test has: peak |raw| near 32767 and zero non-finite values.
  - **BGSM sweeps:**
    - Both call `parse()` and count `Ok` only.
    - Vanilla is measured zero-noise (0/36,888 files with leftover bytes; only v2 and v22 in use), and #4664's suggested fix asked for both checks in the FO4/FO76 sweep.
    - A gating regression that reads the wrong fields but consumes every byte, or leaves bytes over without erroring, stays green.
  - **Other (format × game) gaps:**
    - FO3 BSA is open-and-list only, with no extract sweep.
    - Skyrim LE BSA is covered only through hkx.
    - FO76 DX10 is exercised by one texture.
    - The FO4 DLC CSGs have no test.
- **Evidence**: the test body; the strict-lane log line `[Oblivion] headhuman.egm: 1736 verts, 80 morphs (record these in the EXPECT table)`.
- **Impact**: an Oblivion EGM decode regression, or a BGSM version-gating regression, can pass the nightly `parsers` lane.
- **Related**: #4660, #4665, #4664, #4653
- **Suggested Fix**:
  - Route the Oblivion test through a `Game` variant with `require_game_data`, and pin 1,736 verts / 80 morphs plus the int16 range and non-finite checks.
  - Assert `unconsumed_bytes == 0 && version_over_ceiling.is_none()` in both BGSM sweeps, via `parse_bgsm_diag` and `parse_bgem_diag`.

### PAR-D5-2026-09-29-01: Spec-disabled BGSM roughness has two contradictory contracts
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**: `byroredux/src/asset_provider/material/merge.rs:647`, `byroredux/src/asset_provider/material/merge.rs:658-659`, `byroredux/src/asset_provider/material/merge.rs:1102-1134`; `byroredux/src/asset_provider/tests/bgsm_merge.rs:1409-1462`
- **Status**: NEW (owner overlap `/audit-nifal`)
- **Trigger Input**: vanilla BGSMs with `specular_enabled = false`, `smoothness >= 1.0` and no gloss (`smooth_spec`) map.
- **Description**: the merge states two rules for the same case.
  - **What the code says.** The #4836/#4941 comment on the disabled arm says "Roughness and the tint are left to whatever the NIF side classified" (`:647`). That arm derives no roughness.
  - **What actually happens.** The #3639 near-mirror fallback, `if leaf.smoothness >= 1.0 && material.textures.smooth_spec.is_none() { roughness_override = Some(NEAR_MIRROR_NEUTRAL_ROUGHNESS) }`, runs after the chain walk with no `specular_enabled` check. Its stated premise is "smoothness 1.0 lowered roughness to the 0.04 floor above", which holds only on the enabled arm.
  - **What the test pins.** `bgsm_specular_disabled_zeroes_specular_and_keeps_matte_roughness` pins `Some(0.5)` and calls it "the matte neutral default". So the spec-off outcome depends on an unrelated fallback firing.
- **Evidence** (probe `bgsm-specoff`, vanilla material archives):
  ```
  Fallout4 - Materials.ba2   : 6,616 BGSM; spec off 467; off & smoothness>=1 304; + no smooth_spec map 247 (219 without a template)
  SeventySix - Materials.ba2 : 25,888 BGSM; spec off 653; off & smoothness>=1 623; + no smooth_spec map 619 (603 without a template)
  e.g. props\comicsandmagazineshighres\backside\comicbackblue.bgsm, setdressing\paintingsgeneric\paintinggeneric13.bgsm, decals\stain11b.bgsm
  ```
  One authoring intent (specular off) produces two outcomes:
  - 247 FO4 and 619 FO76 files get roughness 0.5.
  - The other spec-off files keep the NIF-side value.
- **Impact**: the visible effect is small today, for two reasons:
  - `metalness_override = 0` keeps these materials out of the `metalness > 0.3` environment-reflection gate (`triangle.frag:2963-2966`).
  - A zeroed `specStrength` removes direct highlights.

  Any roughness consumer that specStrength does not scale still sees 0.5. The contract is also self-contradictory, which the renderer audit's #4836 already tripped on: its evidence reads `roughness_override=Some(0.5)` as "the same treatment as roughness".
- **Related**: #4654, #4836, #4941, #3639, #3905
- **Suggested Fix**:
  - Choose one rule for the disabled arm (an explicit matte roughness, or NIF-side) and set it inside that branch.
  - Gate the #3639 fallback on `leaf.specular_enabled`.
  - Align the comment and the test with the chosen rule.

### PAR-D5-2026-09-29-02: FaceGen docs still describe the pre-#4653/#4668 layouts in four places
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**: `crates/facegen/src/egm.rs:65-66`, `crates/facegen/src/egt.rs:15-22`, `crates/facegen/src/tri.rs:33-34`
- **Status**: NEW
- **Trigger Input**: n/a (documentation).
- **Description**:
  - **EGM.** The `EgmMorph::scale` doc says it "multiplies the f16 delta". Deltas have been int16 since #4653.
  - **EGT header size.** The header block adds up to 8 + 5×u32 + `padding: [u8; 32]` = 60 bytes. `HEADER_BYTES` is 64, and both the parser and the test synth treat bytes 28..64 (36 bytes) as padding.
  - **EGT R/C labels.** The block labels `R` "image rows (width)" and `C` "image columns (height)", but a row count is a height. Square vanilla images hide this. It would transpose a non-square mod EGT once a compositor exists.
  - **TRI.** The module doc says "24 further bytes" follow the ten header words. 8 + 40 = 48, so 16 bytes remain, which is what the test synth pads.
- **Evidence**: see the locations.
- **Impact**: misleading format documentation for the future EGT compositor and `.tri` lip-sync work (both have no consumer yet, #3544).
- **Related**: #4653, #4668, #3544
- **Suggested Fix**: correct the four doc sites, and name the EGT fields per the SDK (C columns = width, R rows = height).

### PAR-D6-2026-09-29-01: #4673's `installdir` containment is bypassable on Windows
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/game-detect/src/steam.rs:163-182` (`installs_in_library`), test `absolute_or_escaping_installdir_is_rejected`
- **Status**: Regression of #4673 (incomplete fix, `08b72eede`; policy owner `/audit-tooling` Dim 5)
- **Trigger Input**: an `appmanifest_*.acf` whose `installdir` is `\Somewhere` (rooted, no prefix) or `D:Somewhere` (prefix, no root), on Windows.
- **Description**:
  - The check rejects only `authored.is_absolute()` or a `ParentDir` component, then runs `steamapps.join("common").join(install_dir)`.
  - The `std::path` docs say that on Windows `\temp` and `c:temp` are **not** absolute.
  - `join` with a rooted prefixless path "replaces everything except for the prefix", and with a prefixed rootless path it "replaces self". Both forms pass the check and escape `steamapps\common`.
  - The test's `..\..\evil` case passes on Linux for the wrong reason. Backslash is not a separator there, so the value is one `Normal` component and the join stays contained. The test goes green only because that directory does not exist.
- **Evidence**: see the location; `Path::is_absolute` and `Path::join` docs (Windows semantics).
- **Impact**: on Windows, a tampered Steam manifest can make the launcher or `byro-detect` report an install anywhere on the drive. This needs a modified Steam install.
- **Related**: #4673, PAR-D6-2026-09-21-04, #4759
- **Suggested Fix**:
  - Accept only a single `Component::Normal` after splitting on both `/` and `\` on every platform.
  - Add Windows-shaped cases (`\x`, `C:x`) that assert on the check itself, not on the directory being absent.

---

## Prior-finding verification (AUDIT_PARSERS_2026-09-21)

| Prior finding | Issue | State now |
|---|---|---|
| D1-01 HKX string quadratic | #4648/#4649 | Fixed. Class names are held by reference; `read_cstr` has a 256 B cap. |
| D1-02 HKX absolute sample cap | #4655 | **Incomplete**: PAR-D1-2026-09-29-01 |
| D1-03 BA2 DX10 `u32` overflow | #4656 | Fixed (`u64` + saturate, test pinned) |
| D1-04 MenuXml include explosion | #4650 | Exponential blow-up fixed (`depth + 1`, 256-splice budget). New quadratic fetch: PAR-D1-2026-09-29-02 |
| D1-05 MenuXml mid-char panic | #4651 | Fixed (`take_text`) |
| D1-06 sfmaterial recursion | #4657 | Fixed (`nested()` depth 64 on all four recursive functions) |
| D1-07 absolute reservations | #4661 | Fixed (`capacity_hint`, CSG table vs EOF, folder-count reconcile) |
| D2-01 extract errors swallowed | #4658 | Fixed in providers. Leftover in `debug_load.rs`: PAR-D2-2026-09-29-02 |
| D2-02 BA2 short-decode warnings | #4662 | Fixed (path named; short non-final DX10 chunk is `Err`) |
| D2-03 `.fnt` panic | #4652 | Fixed (`fnt.get(12..)`) |
| D2-04 stale LZ4 test | #4663 | Fixed (removed) |
| D3-01 BGSM ceiling / leftover | #4664 | **Production path silent**: PAR-D3-2026-09-29-01 |
| D4-01 no parser CI lane | #4659 | Fixed (`parsers` job) |
| D4-02 strict-lane holes | #4660 | Fixed for the listed sites. New hole in the #4665 Oblivion test: PAR-D4-2026-09-29-01 |
| D4-03 sweep gaps | #4665 | Mostly fixed (see PAR-D4-2026-09-29-01) |
| D4-04 temp BSA leak | #4666 | Fixed |
| D5-01 EGM half-float | #4653 | Fixed (int16 × scale; FNV/FO3 tests assert zero non-finite and peak near 32767) |
| D5-02 BGSM `specular_enabled` | #4654 | Fixed (+ #4836 / #4941). Contract wrinkle: PAR-D5-2026-09-29-01 |
| D5-03 BGSM deferred ledger | #4667 | Fixed |
| D5-04 EGT/TRI format | #4668 | Fixed in code. Docs lag: PAR-D5-2026-09-29-02 |
| D5-05 UVD docs | #4669 | Fixed; still no production caller, as documented |
| D6-01 BA2 `name_table_offset` | #4670 | Fixed |
| D6-02 duplicate keys / BSA normalisation | #4671 | Fixed |
| D6-03 BGSM strict UTF-8 | #4672 | Decode fixed (lossy). BGSM warning silent: PAR-D3-2026-09-29-01 |
| D6-04 VDF recursion / `installdir` | #4673 | Depth cap fixed. Containment Windows gap: PAR-D6-2026-09-29-01 |

## Gate Matrix

| Crate / format | Size caps | Error policy | Version gate | Corpus sweep (strict lane) | CI lane |
|---|---|---|---|---|---|
| bsa: BSA | ✓ `checked_entry_count` / `checked_chunk_size` / `inflate_bounded(_zlib)`; file-relative `capacity_hint` | ✓ labelled `Err`; consumers via `extract_or_warn` (except `debug_load.rs`, D2-02); duplicates warned | ✓ {103,104,105}, LZ4 **frame** v105 only | Oblivion 17/17 brute force; SSE Meshes0 brute force; FNV in-crate; FO3 open+list only; declared_size = extract | ✓ nightly `parsers` |
| bsa: BA2 | ✓ per-chunk + per-record total; DX10 pitch saturates | ✓ path-named; **✗ debug-build panics** (D2-01) | ✓ {1,2,3,7,8}, method 0/3 | FO4 v8 brute force + Textures1; FO76 40 open + 1 DX10; Starfield full sweep | ✓ nightly |
| bsa: CSG | ✓ table vs EOF, `read_psg` PSG-space bound, tail clamp | ✓ short interior chunk → `Err` | magic | FO4 object 0 (no DLC CSGs) | ✓ nightly |
| bsa: UVD | ✓ relation checks | ✓ | magic | example only (no consumer) | n/a |
| bgsm | ✓ string ≤ remaining; template depth 16 + visited set | ✓ offset `Err`; lossy strings | per-field gates; ceiling + leftover diagnostics **silent on BGSM main path** (D3-01) | FO4 100%, FO76 100% + 2 JSON; **no diagnostic assertions** (D4-01) | ✓ nightly |
| sfmaterial | ✓ chunk/field clamps; nesting 64 | ✓ typed errors; `probe_header` tolerant | ✓ v4, BE rejected, vocabulary pins | exact 97 / 1,438,780 | ✓ nightly |
| hkx | strings ≤ 256 B, counts capped; **✗ frames per block loose** (D1-01) | ✓ labelled; annotation skip | ✓ #4332 gates, pointer-derived layout | SE cart family + LE/SE parity | ✓ nightly (LE `--skip` + warning when unset) |
| facegen | ✓ caps + exact size | ✓ (consumer logs at debug) | ✓ magics; int16 EGM; planar i8 EGT | FNV/FO3 pinned; **Oblivion EGM weak** (D4-01) | ✓ nightly |
| menuxml | tile depth 48, expression 64, splice budget 256; **✗ fetch before budget/cycle** (D1-02) | ✓ char-safe scanner, `.fnt` guarded | n/a | Oblivion, FO3, FNV | ✓ nightly |
| game-detect | ✓ VDF depth 32 | ✓ malformed rejected; unreadable logged | n/a | unit only | ✓ unit tests in `ci.yml`; **Windows containment gap** (D6-01) |

## Guards confirmed live (not `#[ignore]`d, passing this run)

- **Size ceilings and decoders:** `entry_count_rejects_attacker_u32_max`, `over_ratio_payload_is_rejected_at_the_ceiling`, `capacity_hint_is_bounded_by_remaining_bytes`, `malicious_bsa_folder_count_u32_max_rejected`, `malicious_file_count_u32_max_rejected_before_allocation`, `lz4_flex_is_pinned_to_the_safe_decoder`, `lz4_decompress_is_panic_guarded`, `oversized_read_len_is_rejected_before_it_is_reserved`, `chunk_table_past_eof_is_rejected_by_name`, `decode_spline_animation_rejects_a_sample_count_bomb`, `decode_spline_animation_rejects_frames_beyond_the_declared_blocks`, `rejects_morph_count_over_cap`, `parse_with_limits_rejects_object_tree_before_materialising_it`, `linear_size_saturates_instead_of_overflowing_at_u16_max_dimensions`.
- **Nesting and cycles:** `include_cycles_terminate`, `resolve_breaks_self_reference_cycle`, `self_referential_struct_is_rejected_instead_of_overflowing_the_stack`, `builtin_ref_chain_is_bounded`, `pathological_nesting_hits_the_depth_cap`.
- **Short decodes and corrupt streams:** `short_decode_stays_ok_for_the_shipped_padding_deltas`, `decompress_chunk_zlib_short_stream_returns_actual_length`, `decompress_chunk_lz4_under_run_returns_actual_length_not_declared`, `extract_dx10_rejects_a_short_non_final_chunk_naming_the_entry`, `corrupt_adler32_trailer_recovers_via_raw_deflate`, `corrupt_deflate_body_still_errors`, `read_annotations_skips_an_out_of_range_time_and_keeps_the_rest`, `extract_rejects_compressed_payload_too_short`, `corrupt_override_falls_through_and_is_named_once`.
- **Version and layout gates:** `synthetic_v105_block_codec_payload_is_rejected_by_frame_reader`, `unknown_version_rejected`, `v3_unknown_compression_method_rejected`, `build_dds_header_is_148_bytes`, `chunk_type_recognized_set_is_pinned`, `builtin_type_recognized_set_is_pinned`, `class_flags_known_mask_is_pinned`, `probe_header_tolerates_an_unrecognized_chunk_type`, `rejects_packfiles_that_are_not_havok_2010_msvc_layout`, `layout_walk_reproduces_the_skyrim_se_offsets`, `rejects_size_count_mismatch`, `convert_hkx_clip_drops_only_the_out_of_range_bound_track`.
- **Concurrency and file model:** `concurrent_extracts_read_their_own_entries`, `synthetic_v105_concurrent_extracts_read_their_own_bytes`, `concurrent_reads_get_their_own_chunk_bytes`.
- **Paths, precedence and parsing:** `siblings_*` (7), `every_content_provider_resolves_collisions_last_wins`, `malformed_documents_are_rejected_rather_than_half_read`, `absolute_or_escaping_installdir_is_rejected` (see D6-01 for its blind spot).
- There is exactly one implementation each of `MAX_ENTRY_COUNT`, `MAX_CHUNK_BYTES` and `MAX_RECORD_TOTAL_BYTES` (`crates/bsa/src/safety.rs`).

## Publishing

`/audit-publish docs/audits/AUDIT_PARSERS_2026-09-29.md`

| Finding | Suggested labels |
|---|---|
| PAR-D1-2026-09-29-01 | `medium` `bug` `animation` `memory` `safety` `game:skyrim` |
| PAR-D1-2026-09-29-02 | `medium` `bug` `ui` `safety` |
| PAR-D2-2026-09-29-01 | `medium` `bug` `import-pipeline` `safety` |
| PAR-D2-2026-09-29-02 | `low` `bug` `tech-debt` |
| PAR-D3-2026-09-29-01 | `low` `bug` `nifal` |
| PAR-D4-2026-09-29-01 | `low` `bug` `test-gap` `game:oblivion` |
| PAR-D5-2026-09-29-01 | `low` `bug` `nifal` `game:fo4` `game:fo76` |
| PAR-D5-2026-09-29-02 | `low` `documentation` `doc-rot` |
| PAR-D6-2026-09-29-01 | `low` `bug` `tech-debt` |

Label caveats to flag in the publish summary:
- BSA/BA2/CSG and FaceGen have no label of their own, so they map to `import-pipeline`.
- game-detect and the debug-load path (tooling) map to `tech-debt`.
