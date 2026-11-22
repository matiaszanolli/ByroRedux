# Parser Discipline Audit: 2026-11-21

**HEAD**: 0878f5b54 · **Baseline**: `AUDIT_PARSERS_2026-11-20.md` (HEAD f3713bf5e) · **Mode**: delta. The only commits on this skill's paths since the baseline are the 24 fixes for the baseline's findings (#7100, #7123–#7145), so this audit reviews those fixes: does each fix close its finding, did it add a bug, and did it miss a sibling site. Code no fix touched was not re-audited.

Three review agents (Sonnet) each took one group of fix commits; the coordinator re-read every reported finding against the code before keeping it. No tracked file changed. The probes live under `/tmp/audit/parsers/{bink,arch,misc}/` and were built in separate target dirs.

## Commits reviewed

| Group | Commits | Fixes |
|---|---|---|
| Bink | 1ddefe5c4, 1e202d894, 6387737d7, 9e45e0261, ffa152e0d, 55093b246, b515d7594, 82c913ef2, 6c6d27584, c6f09518f, b08c28ebb | #7123, #7125, #7127, #7128, #7134, #7135, #7136, #7137, #7138, #7139, #7143 |
| Archives / paths / streaming | 970d6bb3f, a4d0aba84, 590aded9e, 77a8abe1d, 020d8b001, 3ede92180, 99eee822c | #7100, #7131, #7129, #7141, #7142, #7145, #7144 |
| HKX / CDB / EGM / BGSM | 7bab3a1ae, 6c29d48b6, 53a0c08b7, 16197b1c8, 7190db972, e9b1a25ef | #7124, #7132, #7126, #7130, #7140, #7133 |

**Verified complete** (the fix closes its finding; residuals are filed below where noted):
- **Bink:**
  - #7123: `check_bink1_movie` gates both `probe` and `Movie::open` before `VideoDecoder::new`; 32767², 8193×16 and 4097×4096 are refused.
  - #7127, #7128 (every public open path runs under `guard_open`), #7134, #7135 (the flag masks live only in `Bk2::open`, so `probe` and `open` agree; the refusal maps to *skipped*), #7136, #7137 (the census cannot go green vacuously under `REQUIRE_GAME_DATA`) and #7138: complete.
  - #7139: `bink1_track_ids` reads through `bytes.get` with `map_while` and is bounded for any track count. Probes with IDs [5,6], [1,1] and [2,1] always picked a track inside the table.
  - #7143: `create_new` with mode 0600; the panic-path residual is D2-03.
  - #7125: complete for Bink 2 only. Bink 1 is D1-01.
- **Archives:**
  - #7100: the plugin uses `inflate_bounded_zlib`; the ceiling check runs before the reservation.
  - #7131: complete for BA2; the BSA and CSG siblings are D2-04.
  - #7129: the backoff rides the existing per-handle `retry_after_frame`, so there is no new map.
  - #7141: the allocation residual is D1-03.
  - #7142: `find_case_insensitive` checks every component, and `resolve_under` refuses `..\C:\x`.
  - #7145: the scan residual is D6-01.
  - #7144: part (a) is complete (the loose tier wins exactly as in `extract_first`); part (b)'s residual is D6-02.
- **HKX / CDB / EGM / BGSM:**
  - #7124: `reader::walk_list` is the one LIST walker for consume, skip and stream; the sibling is D2-01.
  - #7126: the nif `objects()` map keeps first-wins; hkx `class_at` binary-searches a stable table. The ragdoll sibling is D1-02.
  - #7130: one bounded `report_once` set serves every consumer; no silent `.ok()` is left.
  - #7132: no allocation on the happy path; the residual is D2-06.
  - #7133: `decode_np_ragdoll` refuses anything but v11 with 8-byte pointers.
  - #7140: the test is D4-02.

## Test runs

**Default lane** (`cargo test -p <10 parser crates>`, rustc 1.100.0): all green.

| Crate | Unit | Integration (run / ignored) |
|---|---|---|
| bsa | 121 (+11 ignored) | `ba2_real` 1/12, `bsa_real` 1/10, `csg_real` 0/1 |
| bgsm | 35 | `parse_all` 1/3 |
| sfmaterial | 53 | `header_smoke` 6, `real_cdb` 0/5 |
| hkx | 75 (+10 ignored) | none |
| facegen | 31 | `parse_real_facegen` 0/5 |
| menuxml | 68 | `{fnv,fo3,vanilla}_corpus` 0/14 |
| game-detect | 60 | none |
| bink2 | 21 | `probe_stream` 2 |
| binkaudio | 3 | `fixture` 2 |
| video | 23 (+1 ignored) | none |

`cargo test -p byroredux --bin byroredux asset_provider`: 277 passed, 16 ignored. The other session's uncommitted `commands/assets.rs` compiled.

**Strict real-data lane** (`BYROREDUX_REQUIRE_GAME_DATA=1 cargo test --release -p <crate> --no-fail-fast -- --ignored`, one crate at a time, `BYROREDUX_STEAM_COMMON` set). Only the crates whose code changed were run.

**All green, nothing skipped.**

| Crate | Passed | Wall | Peak RSS |
|---|---|---|---|
| video | 1 (`every_installed_bink_movie_decodes_to_its_last_frame`: the #7137 census and the #7139 whole-soundtrack check over 101 movies) | 8 m 41 s | 0.45 GB |
| bsa | 34 (11 lib + 12 `ba2_real` + 10 `bsa_real` + 1 `csg_real`) | 9 m 39 s | 0.98 GB |
| sfmaterial | 5 (`real_cdb`, including the #7124 list guards on the vanilla CDB) | 4 s | 1.0 GB |
| bink2 | none (no ignored tests; the real-data coverage is the video sweep) | 0 s | n/a |

bgsm, hkx, facegen and menuxml had no code changes and were not re-run; their last strict run was green on 2026-11-20.

## Executive Summary

| Severity | NEW | Existing (open) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 14 | 0 |

- **HIGH, PAR-D1-01.** #7125 bounds a frame's audio *before* decoding only on Bink 2. On Bink 1, the dependency decodes the whole record into a `Vec` first, and `frame_cap` is compared after that. A zero-filled record claiming a huge decoded size costs about 20× its own size (64 MiB → 1.36 GB, 6 s on the frame loop), and the cost grows with the file. That allocation-failure abort is beyond `catch_unwind`.
- **MEDIUM, PAR-D2-01.** #7124 made an `Objects` list that keys nothing an error, but only inside the `"Objects"` arm. A CDB whose `DBFileIndex` class declares no `Objects` field never enters that arm and builds an `Ok` index on which every lookup misses. This is the #5320 class again.
- **LOW.** Every LOW finding is a residual or sibling of a fix just landed:
  - quadratic ragdoll fixup lookups (D1-02);
  - an unbounded directory listing in the loose walker (D1-03);
  - soundtrack truncation and panic-path temp-file leaks (D2-02, D2-03);
  - BSA, CSG, Bink 1 `probe` and CDB-peek errors that carry no context (D2-04, D2-05, D2-06);
  - an overloaded `ErrorKind::Unsupported` (D2-07);
  - a plausible Bink Audio over-read (D2-08);
  - a flaky streamer test (D4-01);
  - the masked BGSM-ledger and mesh-prefix scans (D4-02, D6-01);
  - the archive-replacement residue in the texture streamer (D6-02);
  - an ASCII-only mesh-key fold (D6-03).
- **Process, not filed.** Commit 970d6bb3f (#7100) ends with a `Co-Authored-By: Claude` trailer, against the owner's global rule. It is already on `origin/main`.

## Findings

### HIGH

#### PAR-D1-01: Bink 1 decodes a frame's whole audio record before the per-frame cap is checked; a record's decoded-size claim reserves ~20x the record
- **Severity**: HIGH (abort/OOM reachable from a mod file)
- **Dimension**: Size Discipline
- **Location**: `crates/video/src/lib.rs` `extract_audio` (`pcm.extend(decoder.decode_record(record)…)`, ~l.329) and `decode_soundtrack` (the `pcm.len() > soundtrack.frame_cap` check, ~l.375); contrast the Bink 2 pre-check `packet.decoded_bytes as usize / 2 > soundtrack.frame_cap` (~l.446)
- **Status**: NEW. Incomplete fix of #7125 (closed): the commit says "both codecs stop at a frame past it", but only Bink 2 refuses before decoding.
- **Trigger Input**: a loose `Data\Video\*.bik` (revision `i`) with one audio track whose frame-0 audio record is large and zero-filled. The record's leading u32, the decoded-bytes claim, is set near `0x7fff_fff0`.
- **Description**: the frame cap is applied to `pcm.len()` after `decode_record` has returned its whole output. The decoder honours the record's size claim, and a claim of 8192 yields exactly 8192 bytes of WAV. With a huge claim it keeps decoding until it runs off the record ("an audio block reads past its record"). The error is graceful, but the output buffer has already been reserved and filled. `Movie::open` runs this synchronously on the engine's frame loop (`byroredux/src/video.rs`).
- **Evidence**: probe `binkprobe audiohuge 2147483632 <record_bytes>` (release, black-box API use, no dependency source read):

  | record | peak RSS | wall |
  |---|---|---|
  | 4 MiB | 88 MB | 0.37 s |
  | 16 MiB | 341 MB | 1.47 s |
  | 64 MiB | 1,364 MB | 6.1 s |

  The coordinator re-read `extract_audio` and `decode_soundtrack`: the cap check runs after `decode_frame` returns.
- **Impact**: a hostile or broken mod movie freezes the game for seconds and costs memory in proportion to its file size. A few hundred MiB of record reaches the multi-GB range, where an allocation failure aborts the process.
- **Related**: #7125, PAR-D1-02 of 2026-11-20 (the Bink 2 twin, fixed by #7125's pre-check).
- **Suggested Fix**: in the Bink 1 frame closure, read `record[..4]` as the decoded-bytes claim and return `Err` when `claim / 2 > frame_cap`, mirroring the Bink 2 check, before calling `decode_record`. Add a test that drives `extract_audio` with that hostile record, since today's tests drive `decode_soundtrack` only through closures. Re-run the real-data sweep to confirm no vanilla record trips the check (the slack was measured over 58 files with sound).

### MEDIUM

#### PAR-D2-01: a CDB whose `DBFileIndex` class declares no `Objects` field builds an `Ok` index with no keyed objects
- **Severity**: MEDIUM (silently wrong output: every `.mat` lookup misses; the #5320 class)
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs` `stream_db_file_index` (the `for field in chunk_fields` match; the #7124 gate `if pid_misses == keyable_rows` sits inside the `"Objects"` arm, ~l.852)
- **Status**: NEW (sibling of #7124, closed)
- **Trigger Input**: a mod `materialsbeta.cdb` whose `BSComponentDB2::DBFileIndex` class declaration has no `Objects` field. That is schema-valid, and the class table is file-controlled.
- **Description**: the gate only runs when the field loop reaches `"Objects"`. Without the field, `objects_by_key` stays empty and no error is raised. `Components` still has its `RowInstanceMismatch` backstop, but nothing guards the key table. A smaller issue at the same site: `NoKeyedObjects { rows: keyable_rows }` prints "0 rows" when every row lacked a DBID, even though the table had rows.
- **Evidence**: verified by reading lines 770–870; no `saw_objects` and no post-loop `objects_by_key.is_empty()` check exist. No synthetic CDB was built.
- **Impact**: every Starfield material from that CDB falls back to the PBR presence-only path, and nothing is logged. Vanilla is unaffected (the five real-CDB tests pass).
- **Related**: #7124, #5320, #5899
- **Suggested Fix**: after the field loop, return `NoKeyedObjects` (or a new `MissingObjectsList`) when `objects_by_key.is_empty()`. Carry the total row count beside `keyable_rows` in the error. Add a fixture whose class omits `Objects`.

### LOW

#### PAR-D1-02: `np_ragdoll` `Fixups::pointer` / `class_at` (and nif `HavokPackfile::class_name_at`) are still linear per call
- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `crates/nif/src/blocks/collision/np_ragdoll.rs:269-299` (`Fixups::pointer`, `Fixups::class_at`); `crates/nif/src/blocks/collision/havok_packfile.rs:216` (`class_name_at`)
- **Status**: NEW (sibling of #7126, closed: only `objects()` got the map)
- **Trigger Input**: a `bhkRagdollSystem` blob with a few MB of local or virtual fixups, plus 256 bodies (`MAX_ELEMENTS`) whose compound shapes carry 64 children (`MAX_COMPOUND_CHILDREN`). The bodies can share one compound object.
- **Description**: each lookup runs `.iter().find` over the whole fixup table. That is about 33k calls × several hundred thousand entries.
- **Evidence**: the code was read (`data.local_fixups.iter().find(|f| f.src_offset == src)`; `class_name_offsets.iter().position`). The cost is an estimate from the caps; it was not timed.
- **Suggested Fix**: build the `src → dst` and `data_offset → class` maps once when `Fixups` is constructed, and give `class_name_at` the map `objects()` already builds.

#### PAR-D1-03: `sorted_entries` collects a whole directory listing before the loose budget is charged
- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `byroredux/src/asset_provider/archive.rs:107` (`sorted_entries`), called from `walk` and `index_bounded`
- **Status**: NEW (residual of #7141, closed)
- **Trigger Input**: one loose directory under `Data\` holding millions of files.
- **Description**: `read_dir(dir).flatten().collect()` materialises every `DirEntry` before `MAX_LOOSE_ENTRIES` is charged, so memory is bounded by the directory, not by the budget. `sort_by_key(DirEntry::file_name)` also allocates an `OsString` on every comparison.
- **Suggested Fix**: keep only the `budget - examined` smallest names (a bounded max-heap), or at least use `sort_by_cached_key`; charge the top-level listing the same way.

#### PAR-D2-02: a soundtrack cut at the movie's running-time cap is not reported as a problem
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs` `decode_soundtrack` (the `over` branch, ~l.385)
- **Status**: NEW (residual of #7139, closed)
- **Description**: the branch logs a warning and truncates, but it does not call `stop`. So `Audio.problem` stays `None`, and both `video.status` and the sweep's completeness check treat the truncated track as whole.
- **Suggested Fix**: route the branch through `stop(..)`, or document why audio past running time plus slack is safe to drop silently.

#### PAR-D2-03: the soundtrack temp WAV leaks when decoding panics
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs` `decode_soundtrack` (`create_temp_file` before the loop; `remove_file` only on the `Err` and zero-sample arms)
- **Status**: NEW (residual of #7143 / #7128, closed)
- **Description**: a panic in `decode_record`, `binkaudio::Decoder::decode` or the frame closure unwinds to `guard_open`, which correctly returns `Decode`. The 0600 WAV, up to ~1.4 GB, stays in the temp dir. Verified by reading; no panic trigger is known.
- **Suggested Fix**: a drop guard that removes the path unless it is disarmed on success.

#### PAR-D2-04: BSA and CSG payload reads still fail with a bare "failed to fill whole buffer"
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/bsa/src/archive/extract.rs` (the `read_exact_at(..)?` calls), `crates/bsa/src/csg.rs:341`
- **Status**: NEW (sibling of #7131, closed: BA2 only)
- **Trigger Input**: a truncated BSA or CSG, or a record whose offset is past EOF.
- **Description**: these errors name no entry, offset or length. A cosmetic leftover is that labels double up: the plugin prints "Compressed record X: ESM record X: …", and `or_warn` prints "'p' failed to extract: BA2 zlib chunk of 'p': …".
- **Suggested Fix**: reuse the `ChunkSite` context wrapper (keeping the error kind) at those reads, and drop the duplicated outer labels.

#### PAR-D2-05: a truncated Bink 1 header gives `probe` an opaque I/O error
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs` `probe_unguarded` (two `read_exact` calls)
- **Status**: NEW
- **Description**: a 100-byte file claiming 0xFFFFFFFF frames or tracks returns `Unreadable("failed to fill whole buffer")`, while `Movie::open` describes the same file. Nothing is allocated (6.9 MB peak RSS).
- **Suggested Fix**: name the table that ran short (header, track table or frame index).

#### PAR-D2-06: the CDB instance-type peek runs before the #7132 context wrapper
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs:247` (`peek_top_level_class_name(&state)?`, before `let at = …`)
- **Status**: NEW (residual of #7132, closed)
- **Trigger Input**: an instance chunk whose type ref is outside the class table, or whose first u32 is cut.
- **Description**: these errors still carry no instance index. Separately, the peek clones a `String` for each of ~1.4 M instances; that is pre-existing, and returning `&str` would avoid it.
- **Suggested Fix**: wrap the peek error in `AtInstance { index: pos, class: "<unreadable>" }`.

#### PAR-D2-07: `bink2_container` maps any `io::ErrorKind::Unsupported` to `VideoError::Unsupported`
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs` `bink2_container`
- **Status**: NEW (introduced by #7135's signalling)
- **Description**: an OS-level `Unsupported` from `File::open` or `read` (special files, unusual filesystems) is reported as *skipped* rather than *unreadable*, because the #7135 refusal travels in an overloaded `ErrorKind`.
- **Suggested Fix**: a typed refusal (a `bink2::Error` variant or an inner marker type checked with `get_ref().downcast_ref`) instead of matching the kind.

#### PAR-D2-08: Bink Audio has no end-of-packet overrun check (PLAUSIBLE)
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/binkaudio/src/decoder.rs` `Decoder::decode`
- **Status**: NEW (sibling of #7127; not demonstrated)
- **Description**: the only guard is the lower bound `blocks * min_block_bits <= 8 * data.len()`. Nothing checks that the reader ends inside the packet, so a cut packet that still passes the bound may read missing bits as zeros. The truncation probe hit `BadSize` only.
- **Suggested Fix**: as #7127 did, measure first: run a census over the corpus for reads past `8 * len`. If none occur, refuse them with `Error::Overrun`.

#### PAR-D4-01: `the_provider_is_rebuilt_only_after_an_archive_replacement` is flaky
- **Severity**: LOW (a default-lane test that fails at random)
- **Dimension**: Corpus Gates
- **Location**: `byroredux/src/texture_streamer.rs` (the test, ~l.440); `refresh_provider` reads the process-global `archive_replacements()`
- **Status**: NEW (introduced by #7144)
- **Description**: `archive::tests::open_shared_rereads_*` bumps `ARCHIVE_REPLACEMENTS` from a parallel test thread. A bump between `TextureStreamer::new` and the first `refresh_provider`, or before the last assert, reddens the test.
- **Evidence**: 2 failures in 200 runs of `the_provider_is_rebuilt open_shared` (one panic at l.444, one at l.456). The race was confirmed by reading.
- **Suggested Fix**: let `refresh_provider` take the current generation as an argument; the frame loop passes `archive_replacements()` and the test passes literals.

#### PAR-D4-02: the #7140 ledger test can be satisfied by unrelated mentions of a field name
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `byroredux/src/asset_provider/tests/bgsm_merge.rs` `every_parsed_sidecar_field_is_merged_or_in_the_ledger`
- **Status**: NEW (introduced by #7140)
- **Description**: the needles `.field` and `` `field `` are searched across the whole production text of three files, and those needles match:
  - any receiver, including `ImportedMaterial` writes (`material.two_sided`, `material.specular_color`, `material.textures.base_color`);
  - fields of the other parsed structs (`BgemFile::emittance_color` is masked by `bgsm.emittance_color`);
  - mentions in comments.

  As a result, deleting the real read of `translucency_subsurface_color`, `translucency_transmissive_scale`, `model_space_normals`, `specular_color`, `alpha_test`, `two_sided` or BGEM `base_color`, `emittance_color` or `environment_mapping*` keeps the test green. Lines of the form `material.f = bgsm.f;` are genuinely guarded.
- **Evidence**: `/tmp/audit/parsers/misc/ledger_probe.py` (a re-implementation of the scan).
- **Suggested Fix**: key the needle by receiver per struct (`bgsm.`/`.base.`/`leaf.` for BGSM and the base, `bgem.` for BGEM), strip `//` comments before scanning, restrict ledger needles to the ledger block, and list accessor-consumed fields (`env_mapping_enabled`) explicitly.

#### PAR-D6-01: `no_production_code_prefixes_meshes_by_hand` only catches a name placeholder followed directly by `}"`
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `byroredux/src/workspace_hygiene_tests.rs`
- **Status**: NEW (residual of #7145, closed)
- **Description**: the scan misses all of these spellings:
  - `format!("meshes\\{stem}.nif")`
  - `format!("meshes\\{}.nif", x)`
  - `format!("meshes\\{dir}\\{file}")`
  - `format!("{}\\{}", "meshes", x)`
  - `push_str("meshes\\")`
  - `format!("meshes/{x}.nif")`

  No live offender exists today; the remaining `meshes\` formats in production are fixed paths.
- **Suggested Fix**: flag any string literal that starts with `meshes[\\/]` and then interpolates, or that ends at the separator, outside `core/src/paths.rs`.

#### PAR-D6-02: after an archive replacement the streamer keeps promoting textures that were floored from the old file
- **Severity**: LOW (silently mixed mips, but the trigger needs an archive replaced on disk while the engine runs)
- **Dimension**: I/O & Paths
- **Location**: `byroredux/src/texture_streamer.rs` `refresh_provider`; `byroredux/src/asset_provider/archive.rs` `open_shared` (the `ARCHIVE_REPLACEMENTS` bump)
- **Status**: NEW (residual of #7144, closed)
- **Description**: `refresh_provider` swaps the provider and nothing else. Resident textures floored from the old file are never re-validated, so their next promotion reads the new file, which is the two-file mixed-mip case #7144 set out to close. The counter also stays quiet in two cases: when the 1024-entry registry evicts an entry and the reopen lands in an empty slot, and when `ArchiveStamp::of` fails, which bypasses the registry. The read-before-build ordering is correct.
- **Suggested Fix**: record the generation on each stream source at floor time, and refuse (or re-floor) promotions for sources floored under an older generation. Bump the counter on an eviction-then-reopen as well, or compare each archive's own `ArchiveStamp`.

#### PAR-D6-03: `canonical_mesh_key` folds ASCII only; `normalize_path` folds Unicode
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/core/src/paths.rs` `canonical_mesh_key` (`to_ascii_lowercase`) vs `crates/bsa/src/ba2.rs` `normalize_path` (`to_lowercase`)
- **Status**: NEW (pre-existing: the renderer's old key was ASCII-only too; #7145 moved it into the one rule)
- **Description**: reads are correct, because archive and loose lookups re-normalise. But `Ä.nif` and `ä.nif` get different cache keys in the renderer mesh cache and `nif_import_registry`, so one asset is parsed and uploaded twice.
- **Suggested Fix**: have `canonical_mesh_key` fold through the same function as `normalize_path` (move the fold to `core::paths`, which bsa can depend on, or the reverse).

## Gate Matrix

| Crate | Size caps | Error policy | Version gate | Corpus sweep | CI lane |
|---|---|---|---|---|---|
| bsa | ✓ | BA2 ✓ / BSA+CSG context gap (D2-04) | ✓ | ✓ strict | manual-only `parsers` |
| bgsm | ✓ | ✓ | ✓ | ✓ strict | manual-only |
| sfmaterial | ✓ | missing-`Objects` gap (D2-01), peek context (D2-06) | ✓ | ✓ strict | manual-only |
| hkx | ✓ (nif ragdoll lookups linear, D1-02) | ✓ | ✓ (#7133) | ✓ strict | manual-only |
| facegen | ✓ | ✓ (#7130 warns) | ✓ | ✓ strict | manual-only |
| menuxml | ✓ | ✓ | n/a | ✓ strict | manual-only |
| game-detect | ✓ | ✓ | n/a | n/a | default |
| bink2 / binkaudio | ✓ | binkaudio overrun plausible (D2-08) | ✓ revisions + flags (#7134/#7135) | via video | default fixtures |
| video | Bink 1 audio record unbounded (D1-01) | cap cut unreported (D2-02), panic leak (D2-03) | ✓ (#7136/#7138) | ✓ census (#7137) | manual-only |
| asset_provider (consumer) | loose listing unbounded (D1-03) | ✓ | n/a | n/a | default (flaky D4-01) |

Suggested next step: `/audit-publish docs/audits/AUDIT_PARSERS_2026-11-21.md`. Labels: `import-pipeline` on every finding, `game:starfield` on D2-01 and D2-06, `game:fo4` on D1-02, and `test-gap` on D4-01, D4-02 and D6-01.
