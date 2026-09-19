---
description: "Parser-discipline audit of the non-NIF, non-ESM readers of untrusted game-archive input — BSA/BA2/CSG archives, BGSM/BGEM, Starfield CDB, Havok HKX packfiles, FaceGen sidecars, MenuXml parse side, Steam VDF/ACF"
argument-hint: "--focus <dimensions> [--crate bsa|bgsm|sfmaterial|hkx|facegen|menuxml|game-detect]"
---

# Parser Discipline Audit (archives, materials, packfiles, sidecars)

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

Archives and mod files are attacker-controllable. This skill audits *reader discipline* for formats no other audit owns; per-game content claims (parse rates, rendering) stay with the per-game audits; NIF blocks `/audit-nif`, ESM `/audit-esm`, `.spt` `/audit-speedtree`, `.pex`/`.psc` `/audit-papyrus`; HKX *playback* (cinematic systems, root motion, completion events) stays `/audit-scripting` Dim 8 — this skill owns the HKX *decode*.

| Crate | Reads | Consumer |
|---|---|---|
| `crates/bsa/src/` (`archive/`, `ba2.rs`, `csg.rs`, `uvd.rs`, `safety.rs`, `naming.rs`) | BSA v103/v104 zlib, v105 LZ4 **frame**; BA2 v1/2/3/7/8 GNRL + DX10 (DDS header reconstructed; v3 LZ4 **block**); FO4 `.csg`, `.uvd` header (`docs/engine/archives.md`, `docs/engine/fo4-csg-format.md`) | `byroredux/src/asset_provider/archive.rs`, `byroredux/src/cell_loader/precombined.rs` |
| `crates/bgsm/src/` | BGSM/BGEM v1–v22 + template chain | `byroredux/src/asset_provider/material/{provider,merge}.rs` |
| `crates/sfmaterial/src/` | Starfield `materialsbeta.cdb` | `byroredux/src/asset_provider/material/cdb.rs` (header probe only) |
| `crates/hkx/src/` | Havok 2010 packfile, 32-bit LE (Skyrim LE) + 64-bit LE (SE) | `byroredux/src/asset_provider/animation.rs` (sole caller) |
| `crates/facegen/src/` | `.egm` / `.egt` / `.tri` | `byroredux/src/npc_spawn/resumable.rs` (EGM only) |
| `crates/menuxml/src/parse.rs` | Oblivion/FO3/FNV menu XML (lenient scanner; the rest is `/audit-ui`) | `crates/menuxml/src/menu.rs` |
| `crates/game-detect/src/{vdf,steam,catalog}.rs` | Steam `libraryfolders.vdf` / `appmanifest_*.acf` (policy: `/audit-tooling` Dim 5) | launcher, `byro-detect` |

**Contract every reader must meet** (a finding violates one): (a) a file-controlled size is bounded *before* memory is reserved; (b) malformed bytes give `Err` with field context — never a panic, abort or unbounded loop; (c) a short/degraded decode is a documented `Ok` + warning or an error, never silent; (d) a real-data sweep exists and cannot go green vacuously.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: dimensions to run (default all 6). `--crate <name>`: one crate.
- Delta-first: run each `First step`, then `git log --since=<last AUDIT_PARSERS date> -- <Paths>`; deep-audit only dimensions whose Paths changed.

## Extra Per-Finding Fields

- **Dimension**: Size Discipline | Error Semantics | Version Gating | Corpus Gates | Decode-Consumer Wiring | I/O & Paths
- **Trigger Input**: the byte-level condition (field, value, file). Severity per `_audit-severity.md`: abort/OOM/stack overflow reachable from an archive or mod file = HIGH (CRITICAL if UB); vanilla content failing to load = HIGH; silently wrong bytes = MEDIUM–HIGH; missing gate/test = LOW–MEDIUM.

## Phase 1: Setup

1. `mkdir -p /tmp/audit/parsers`; `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/parsers/issues.json`; dedup per `_audit-common.md`.
2. Read the newest `docs/audits/AUDIT_PARSERS_*.md` if any.
3. `cargo test -p byroredux-bsa -p byroredux-bgsm -p byroredux-sfmaterial -p byroredux-hkx -p byroredux-facegen -p byroredux-menuxml -p byroredux-game-detect`; record counts. Real-data suites, data present, one crate at a time: `BYROREDUX_REQUIRE_GAME_DATA=1 cargo test -p <crate> -- --ignored`. Never `ComponentDatabaseFile::parse` the vanilla CDB (~9.19 GiB tree, #4274); `real_cdb.rs` streams.

## Phase 2: Dimensions (max 3 concurrent agents)

### Dimension 1: Untrusted-size discipline
Paths: `crates/bsa/src/`, `crates/hkx/src/`, `crates/facegen/src/`, `crates/sfmaterial/src/reader.rs`, `crates/bgsm/src/reader.rs`, `crates/menuxml/src/parse.rs`, `crates/game-detect/src/vdf.rs`.
First step: `grep -n 'with_capacity\|vec!\[0\|read_to_end\|as usize' <Paths>`; trace each hit to the bound before it.
Guards (confirm live, not `#[ignore]`d): `entry_count_rejects_attacker_u32_max`, `over_ratio_payload_is_rejected_at_the_ceiling` (`crates/bsa/src/safety.rs`); `malicious_bsa_folder_count_u32_max_rejected` (`crates/bsa/src/archive/tests.rs`); `malicious_file_count_u32_max_rejected_before_allocation`, `lz4_flex_is_pinned_to_the_safe_decoder` (workspace `lz4_flex` keeps `safe-decode`; off = heap overflow no `catch_unwind` catches, HIGH), `lz4_decompress_is_panic_guarded` (`crates/bsa/src/ba2.rs`); `oversized_read_len_is_rejected_before_it_is_reserved` (`crates/bsa/src/csg.rs`); `decode_spline_animation_rejects_a_sample_count_bomb` (`crates/hkx/src/animation.rs`); `rejects_morph_count_over_cap` (`crates/facegen/src/egm.rs`); `parse_with_limits_rejects_object_tree_before_materialising_it` (sfmaterial).
Beyond the guards:
- One implementation of the ceilings — `MAX_ENTRY_COUNT` 10 M, `MAX_CHUNK_BYTES` 1 GiB, `MAX_RECORD_TOTAL_BYTES` 2 GiB, `inflate_bounded` / `inflate_bounded_zlib`. A reader passing a file-controlled length to `Vec::with_capacity`, or `read_to_end` on a decoder with no `declared + 1` cap, is the regression; so is a second copy of the constants.
- The ceilings are absolute, not file-relative: `read_general_records`, `read_dx10_records` and the BA2 name table reserve `Vec::with_capacity(count)` for up to 10 M entries before a tiny file fails its first read. Is a file-length bound warranted (LOW)?
- Sizes computed from data *outside* the archive (NIF-carried counts into `CsgArchive::read_psg`, #3758) need their own overflow-checked bound.
- File-value arithmetic (`count * stride`, `offset + len`) is `checked_*` or bounded by a preceding cap (hkx `bone_count > 4096` precedes its multiplies).
- Nesting: menuxml (tiles `depth > 48`, expressions `> 64`, `MAX_READ_DEPTH` 32, include-cycle set; `include_cycles_terminate`; `MenuFileSource` providers must stay archive-backed); bgsm templates (`DEPTH_LIMIT` 16 + visited set, `resolve_breaks_self_reference_cycle`). No explicit bound: sfmaterial `read_user_class` <-> `read_value` (only `ParseLimits::max_instances`; latent, production calls `probe_header`) and `vdf::parse_entries` (Steam-written, LOW) — re-check.
- `&str` byte-range slicing of disk-derived names panics mid-char (#3391); use `as_bytes()`.

### Dimension 2: Truncation, EOF and error semantics
Paths: Dimension 1's, plus `byroredux/src/asset_provider/{archive,animation,material/provider}.rs`, `byroredux/src/npc_spawn/resumable.rs` (consumer `Err` arms).
First step: list non-test `unwrap()` / `expect(` / `[idx]` on file-derived indices in Paths; read each consumer's `Err` arm.
- Short decode is a deliberate `Ok` + `log::warn!` (shipped padding deltas #622/#812); over-run is `Err(InvalidData)`. Guards: `short_decode_stays_ok_for_the_shipped_padding_deltas`, `decompress_chunk_zlib_short_stream_returns_actual_length`, `decompress_chunk_lz4_under_run_returns_actual_length_not_declared`. A short non-final DX10 chunk shifts every later mip under an unchanged synthesized DDS header — confirm the warning names the path.
- A checksum-only zlib failure retries as raw DEFLATE; a corrupt body must still error (`corrupt_adler32_trailer_recovers_via_raw_deflate`, `corrupt_deflate_body_still_errors`).
- Errors carry context (bgsm `Error::UnexpectedEof { offset }`, hkx labels, `checked_*` label); a bare `read_exact` `UnexpectedEof` from archive open needs the consumer to add the path.
- A bad asset costs the asset, not the cell, and a partial result must be signalled: hkx skips one bad annotation and keeps the clip (`read_annotations_skips_an_out_of_range_time_and_keeps_the_rest`); menuxml truncates an over-deep subtree with a warning. Flag any silent partial result.
- File-byte-reachable panics: NIF parse runs under `catch_unwind` in `byroredux/src/streaming.rs`; archive extract, hkx, bgsm, facegen and menuxml callers may run unguarded on the main thread. `crates/facegen/src/egm.rs` indexes `bytes[offset]` after its exact-size check — verify that precondition is unconditional.

### Dimension 3: Version and generation gating
Paths: `crates/bsa/src/archive/open.rs`, `crates/bsa/src/ba2.rs`, `crates/bgsm/src/{base,bgsm,bgem}.rs`, `crates/sfmaterial/src/reader.rs`, `crates/hkx/src/packfile.rs`, `crates/facegen/src/`.
First step: `grep -n 'version' <Paths>`; every gate keys off the file's *own* version field, never a `GameKind` sniff.
- BSA: allowlist {103,104,105}, unknown = `Err`; folder record 16 vs 24 B and `u64` offsets on v105; embed-name flag 0x100 only for >= 104; codec zlib v103/v104, **LZ4 frame** v105 — never `lz4_flex::block` (`synthetic_v105_block_codec_payload_is_rejected_by_frame_reader`); size-word bit 31 has no sourced meaning and is not acted on (#3367).
- BA2: exhaustive `match` over {1,2,3,7,8} (`unknown_version_rejected`); v2 header +8 B, v3 +12 B carrying `compression_method` (0 zlib, 3 LZ4 **block**, else `Err`: `v3_unknown_compression_method_rejected`); v1/v2/v7/v8 always zlib; per-chunk `packed_size == 0` = stored raw regardless of codec; DX10 DDS header 148 B (`build_dds_header_is_148_bytes`) with cubemap, mip clamp and per-DXGI pitch/linear-size flag.
- BGSM/BGEM: per-field `version >=` / `<` gates; no upper-bound rejection (2026-09-19 grep), so an unknown future version decodes with the newest layout — intended?
- CDB: header errors (`BadMagic`, `UnsupportedVersion`, `BigEndianUnsupported`); vocabularies pinned by `chunk_type_recognized_set_is_pinned`, `builtin_type_recognized_set_is_pinned`, `class_flags_known_mask_is_pinned`; `probe_header` tolerates unknown chunk FourCCs (#4273), the object-tree parse must reject them.
- HKX: `Packfile::parse` gates magic, pointer width 4|8, little-endian, `fileVersion == 8`, reuse-padding 0, `hk_2010*` (#4332, `rejects_packfiles_that_are_not_havok_2010_msvc_layout`); offsets derive from pointer size (`layout_walk_reproduces_the_skyrim_se_offsets`); a track binding a bone the skeleton lacks is dropped with a warning in `convert_hkx_clip` (#3013), never zip-truncated.
- FaceGen: magics `FREGM002` / `FREGT003` / `FRTRI003`; EGM demands *exact* file size (`rejects_size_count_mismatch`), so trailing bytes on a modded head are refused — intended?

### Dimension 4: Corpus and baseline gates
Paths: `crates/*/tests/*real*.rs`, `crates/bgsm/tests/parse_all.rs`, `crates/sfmaterial/tests/`, `crates/menuxml/tests/`, `crates/hkx/src/animation.rs` (test module), `.github/workflows/real-data-gates.yml`.
First step: `grep -rn '#\[ignore' crates/{bsa,bgsm,sfmaterial,hkx,facegen,menuxml}` vs `grep -n 'cargo test' .github/workflows/*.yml`; diff the two.
- Strict lane (#3850): `BYROREDUX_REQUIRE_GAME_DATA=1` makes an absent corpus panic and a set `BYROREDUX_<GAME>_DATA` override binding. Implemented in `crates/bsa/tests/{bsa_real,ba2_real,csg_real}.rs`, `crates/bgsm/tests/parse_all.rs`, `crates/sfmaterial/tests/real_cdb.rs`, `crates/facegen/tests/parse_real_facegen.rs`.
- Verified 2026-09-19, re-check: the only scheduled real-data lane runs `byroredux-nif` harnesses; no workflow executes the bsa/bgsm/sfmaterial/hkx/facegen/menuxml `--ignored` suites, so a regression there is invisible until run by hand (one MEDIUM test-gap finding unless that changed).
- Vacuous-green sites, verify each: `crates/menuxml/tests/{vanilla_corpus,fo3_corpus}.rs` are plain `#[test]` (unset env prints "skipping" and passes; override unchecked; off the strict lane); hkx real-data tests are `#[ignore]` but SKIP-and-return on a missing archive even under `REQUIRE`; `open_materials_archive` (`crates/bgsm/tests/parse_all.rs`) returns `None` when the archive is absent *or fails to open*, so a broken `Ba2Archive::open` reads as a skip.
- Assertion strength: `real_cdb.rs` asserts only non-zero counts; bgsm asserts a `MIN_SUCCESS_RATE` floor over `Fallout4 - Materials.ba2` only (no Skyrim SE / FO76 BGSM sweep). Tabulate (format x game) sweeps and list gaps.
- ROADMAP compat-matrix rates are NIF-level; any BGSM/BA2/CDB rate claim in `ROADMAP.md` or `docs/feature-matrix.md` needs a dated measurement + sweep command. CI has no game data, so each format branch needs an in-memory fixture (`build_v105_archive`, `PackfileBuilder`, `synth_egm`, `bgsm::tests::minimal_v2_bytes`).

### Dimension 5: Decode <-> consumer wiring
Paths: `crates/bgsm/src/`, `byroredux/src/asset_provider/material/{merge,cdb}.rs`, `crates/facegen/src/lib.rs`, `byroredux/src/npc_spawn/resumable.rs`, `crates/hkx/src/animation.rs`, `byroredux/src/asset_provider/animation.rs`, `crates/bsa/src/uvd.rs`.
First step: list `pub` fields of `BgsmFile` / `BgemFile` / `HkxAnimation` / `EgmFile`; grep each in its consumer.
- BGSM/BGEM fields vs `merge_external_material` (`byroredux/src/asset_provider/tests/bgsm_merge.rs`): classify unconsumed fields renderer-relevant (finding) vs editor-only. The merge takes `&mut ImportedMaterial`; a widened signature is a NIFAL violation (`/audit-nifal`).
- CDB is presence-only (`MergeOutcome::PresenceOnly`); per-field extraction is Phase 2, #3398 (OPEN, verified 2026-09-19; spike `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md`). Do not file "CDB fields unused" as new.
- FaceGen `.egt` / `.tri` have no consumer — documented in `crates/facegen/src/lib.rs` (#3544, closed by doc correction); only EGM reaches `apply_morphs`. Re-file only if that claim changes.
- UVD: envelope only, consumed in `cell_loader/precombined.rs`; payload undecoded by design.
- Consumed-but-unparsed: a consumer `unwrap_or(default)` standing in for an unexposed field. ESM/NIF cases (TXST DecalData #3638, SkinAttach) go to `/audit-esm` / `/audit-nif`.

### Dimension 6: I/O and path robustness
Paths: `crates/bsa/src/{naming.rs,archive/mod.rs,ba2.rs}`, `byroredux/src/asset_provider/{archive,texture,material/provider}.rs`, `byroredux/src/asset_provider/tests/`, `crates/game-detect/src/{vdf,steam}.rs`, `crates/menuxml/src/parse.rs`.
First step: `cargo test -p byroredux --bin byroredux asset_provider`; read `archive_precedence.rs` and `archive_siblings.rs`.
- Key: both readers use `to_lowercase().replace('/', "\\")` (Unicode fold); consumer normalisers `normalize_mesh_path`, `normalize_texture_path`, `normalize_material_path`, `strip_build_prefix`, `canonical_texture_key` — check open-time and lookup keys agree and non-ASCII names do not diverge from ASCII-only folds elsewhere.
- Siblings: `numeric_sibling_paths` (pure; shared with the launcher validator): `Foo.bsa` -> 2..9, `Foo0.bsa` -> 1..9, `Foo01.ba2` -> 02..09, mid-series none; guards `siblings_*` in `archive_siblings.rs`; `open_with_numeric_siblings` de-dups by case-fold.
- Provider order is **last-listed wins** across a chain (#3637); `every_content_provider_resolves_collisions_last_wins` scans providers for a first-wins loop; `count_shadowed_entries` logs shadowing. Within one archive a duplicate normalised name silently overwrites in the `HashMap` (BSA and BA2) — is it logged?
- File model: one `Mutex<File>` per archive, no mmap, seek+read serialised (#360); bounds checks must fail on a replaced archive, not return garbage (`extract_rejects_compressed_payload_too_short`).
- VDF/ACF: malformed input rejected, not half-read (`malformed_documents_are_rejected_rather_than_half_read`); `libraryfolders.vdf` from `steamapps/` + `config/` is unioned; ACF `installdir` is joined under `steamapps/common/` with no traversal check in `installs_in_library` (Steam-written, LOW).

## Phase 3: Merge and Cleanup

Write `docs/audits/AUDIT_PARSERS_<TODAY>.md`: Executive Summary (findings by severity; crates swept; real-data suites run vs skipped), deduplicated Findings, and a Gate Matrix (crate x {size caps, error policy, version gate, corpus sweep, CI lane}). Cross-audit dedup: NIF/ESM sizes `/audit-nif` Dim 1 / `/audit-esm` Dim 1; `unsafe` `/audit-safety`; MenuXml rendering `/audit-ui`. Then `rm -rf /tmp/audit/parsers` and suggest `/audit-publish docs/audits/AUDIT_PARSERS_<TODAY>.md` (labels: `import-pipeline` BSA/BA2/CSG/FaceGen, `nifal` BGSM/CDB, `animation` HKX, `ui` MenuXml, `test-gap` gates, `tech-debt` game-detect; `game:*` when title-specific).
