# Parser Discipline Audit: 2026-11-21 (second pass)

**HEAD**: 972c023b8 · **Baseline**: `AUDIT_PARSERS_2026-11-21.md` (HEAD 0878f5b54, this morning) · **Mode**: delta. The only commits on this skill's paths since the baseline are the 16 fixes for its findings (#7153–#7168), so this pass reviews those fixes. For each fix it checks three things: whether it closes its issue, whether it added a bug, and whether it missed a sibling site. Code no fix touched was not re-audited.

The same session wrote all 16 fixes. Three fresh review agents (Sonnet) were therefore given the commits and the issue text, not the author's conclusions. The coordinator re-read each kept finding against the code. No tracked file changed. Probes live under `/tmp/audit/parsers/{bink,arch,misc}/` and built in separate target dirs.

## Commits reviewed

| Group | Commits | Issues |
|---|---|---|
| Bink | 07e25694c, a87307cfe, 58f80b658, 520a8d4e9, 69655af6a, a84a4d174 | #7153, #7157, #7160, #7158, #7162, #7163 |
| Archives / paths / streaming | ff7b39443, b7b54ff1e, efad46259, 04b28882d, 8f28af5c6, 972c023b8 | #7156, #7159, #7164, #7167, #7166, #7168 |
| CDB / ragdoll / BGSM | 6f5035a9f, 193fb6e72, 27ea36216, 93093f879 | #7154, #7161, #7155, #7165 |

**Verified complete:**
- **#7153** — the frame-cap check on a record's stated size bounds the decode in every case probed through `Movie::open`:
  - records of 0 to 3 bytes;
  - a claim of 0;
  - claims at, under and over the cap;
  - stereo;
  - garbage track flags;
  - a 64 MiB record.
- **#7157, #7160** — complete; the second keeps one cosmetic note, the clamped counts in its message.
- **#7162, #7163** — complete. A truncation sweep of the fixture produced no silently different `Ok` output.
- **#7154, #7155, #7161** — complete. #7155's maps reproduce the old scans' answers in every listed case.
- **#7164** — complete.
- **#7168** — complete for mesh keys. 14 hard inputs agree with `normalize_path`, among them final sigma, `İ`, the Kelvin sign and `ẞ`.

**Partly complete:**
- **#7158** — the guard is tested, but not at the place where the soundtrack uses it (D2-01).
- **#7159** — BA2 now formats on every read (D1-02), and the open-time reads were missed (D2-05).
- **#7165** — residual blind spots in the test (D4-01).
- **#7166** — spelling 4 from the issue is still not flagged (D4-02).
- **#7167** — regression, D6-01.

## Test runs

**Default lane** (`cargo test -p <10 parser crates>`, rustc 1.100.0): all green.

| Crate | Unit | Integration (run / ignored) |
|---|---|---|
| bsa | 122 (+11 ignored) | `ba2_real` 1/12, `bsa_real` 1/10, `csg_real` 0/1 |
| bgsm | 35 | `parse_all` 1/3 |
| sfmaterial | 55 | `header_smoke` 6, `real_cdb` 0/5 |
| hkx | 75 (+10 ignored) | none |
| facegen | 31 | `parse_real_facegen` 0/5 |
| menuxml | 68 | `{fnv,fo3,vanilla}_corpus` 0/14 |
| game-detect | 60 | none |
| bink2 | 22 | `probe_stream` 2 |
| binkaudio | 4 | `fixture` 2 |
| video | 26 (+1 ignored) | none |

The reviewers also ran these:
- `cargo test -p byroredux --bin byroredux asset_provider`: 279 passed, 16 ignored.
- `cargo test -p byroredux-renderer --lib texture_registry`: 109 passed.
- `cargo test -p byroredux-nif --lib collision`: 195 passed.

**Strict real-data lane** (`BYROREDUX_REQUIRE_GAME_DATA=1`, release). Each crate was run once after the last commit that touched it:

| Crate | Passed | Wall | Peak RSS | Run after |
|---|---|---|---|---|
| bsa | 34 (11 lib, 12 `ba2_real`, 10 `bsa_real`, 1 `csg_real`) | 12 m 24 s | 1.0 GB | #7159 (this pass) |
| sfmaterial `real_cdb` | 5 | 8 s | — | #7154 and #7161 (this pass) |
| nif `np_ragdoll_corpus` | 5 (75 blobs, 850 joints, 0 skipped) | 2 s | — | #7155 |
| video `every_installed_bink_movie_decodes_to_its_last_frame` | 1 (101 movies, whole soundtracks) | 9 m 02 s | — | #7163, which follows every video and binkaudio commit |

bgsm, hkx, facegen and menuxml had no code changes; their last strict run was green on 2026-11-20.

## Executive Summary

| Severity | NEW | Existing (open) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 11 | 0 |

**MEDIUM, PAR-D6-01: #7167 traded one streaming defect for another.** Cell loads build their texture provider through `Archive::open_shared`, and that call is what notices a replaced archive. The streamer retires its sources only on a later step, inside the frame loop. So the first cell loaded after a replacement queues its floors under the old epoch, although they come from the new file. They are then retired and never promoted, so those textures stay at floor resolution until released. The fix must key the gate on the archive generation at floor time, not on a counter advanced after the fact.

**LOW.** Every LOW finding is a residual or sibling of a fix that just landed:
- the soundtrack WAV is guarded only until `keep()` (D2-01);
- binkaudio decoder state after `Overrun` (D2-02);
- surplus packet data is accepted (D2-08, PLAUSIBLE);
- degenerate CDB indexes still build `Ok` (D2-03);
- unattributed DBFileIndex errors (D2-04);
- BSA, BA2 and CSG open-time reads have no context, and the BA2 chunk label now allocates on every read (D2-05, D1-02);
- the #7156 bound is loose (D1-03);
- the ledger test and meshes scan still have blind spots (D4-01, D4-02);
- four other caches still fold ASCII-only (D6-02).

**Not filed (INFO):**
- #7160's message prints the clamped counts rather than the header's claim.
- `deficits` telemetry counts retired sources, so `deficits > 0, settled = 1` can persist; settling itself is not blocked.
- Bink 1 `Movie::open` still reads the whole file uncapped. This is known and recorded in the skill.

## Findings

### MEDIUM

#### PAR-D6-01: the first cell loaded after an archive replacement streams no sharper mips: its floors from the new file are retired with the old ones
- **Severity**: MEDIUM. A regression #7167 introduced: textures from the right file stay at floor mips, with no error.
- **Dimension**: I/O & Paths
- **Location**:
  - `crates/renderer/src/texture_registry/upload.rs`: `enqueue` stamps `epoch: self.stream_epoch`; `retire_stream_sources`; the `source.epoch == self.stream_epoch` gate in `apply_wanted`.
  - `byroredux/src/app_frame.rs`: the retire call after `refresh_provider`.
  - `byroredux/src/app_step.rs`: the cell-load `build_texture_provider` calls.
- **Status**: NEW (regression of #7167, closed)
- **Trigger Input**: a texture archive replaced on disk while the engine runs, then a cell load.
- **Description**: the replacement counter (`ARCHIVE_REPLACEMENTS`) moves only inside `Archive::open_shared`, which a cell load calls through `build_texture_provider` (`app_step.rs`) before it resolves and floors that cell's textures. Those floors are read from the new file but queued under the current `stream_epoch` N. On the next frame-loop pass, `refresh_provider` sees the moved counter, and `retire_stream_sources` advances the epoch to N + 1. Every floor queued since the counter moved therefore carries a stale epoch: it is never promoted and stays blurry until it is released. Before #7167 these textures promoted correctly; the commit closed the mixed-mip case for floors from the old file at the cost of floors from the new file.
- **Evidence**: the call order was traced by the reviewer and re-read by the coordinator (cell-load provider builds at `app_step.rs:1484` and the sites near it). The epoch is stamped at enqueue and compared only at promotion. The #7167 test inserts `StreamSource { epoch: reg.stream_epoch, .. }` by hand, so it never queues a floor between the counter moving and the retire. Not run end to end.
- **Impact**: after a mid-session archive swap (a mod manager redeploying, a user editing a BA2), the next cell's textures render at low resolution for as long as they stay loaded.
- **Related**: #7167, #7144
- **Suggested Fix**: stamp each queued floor with the archive generation (`archive_replacements()` read when the provider that reads it was built, passed through the enqueue). Refuse promotion only when a source's generation differs from the streamer provider's. Add a test that queues a floor after the counter moves and before the retire, and expects it to promote.
- **Also (INFO)**: `stats.deficits` counts retired sources wanted finer, so `deficits > 0, settled = 1` can persist after a replacement; `promotions_idle` does not block settling.

### LOW

#### PAR-D1-02: #7159 formats a `String` before every BA2 chunk read, not only on failure
- **Severity**: LOW (an allocation per DX10 chunk read, on the streaming path)
- **Dimension**: Size Discipline
- **Location**: `crates/bsa/src/ba2.rs` `read_chunk_payload` (`let chunk = site.chunk.map_or(String::new(), |index| format!(" chunk {index}"));`)
- **Status**: NEW (introduced by #7159; the commit says a successful read formats nothing)
- **Description**: the label suffix used to be built inside `map_err`. It is now built before the read. The BSA and CSG sites pass `format_args!` and allocate nothing on success.
- **Suggested Fix**: a `Display` adapter for the optional chunk index, passed through `format_args!`.

#### PAR-D1-03: #7156's loose bound is loose, and an exhausted budget still enumerates a whole directory
- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `byroredux/src/asset_provider/archive.rs` `sorted_entries`, `walk`
- **Status**: NEW (residual of #7156, closed)
- **Description**: memory is bounded at `max(2 * keep, 64)` entries, which at the default 2^21 budget is up to ~4 M `(OsString, DirEntry)` pairs, a few hundred MB transient (not measured). With `keep == 0` the whole listing is still walked to set `dropped`, holding nothing; stopping at the first entry would do. The test does not assert the 2x holding bound.
- **Suggested Fix**: return early for `keep == 0` after one entry, and consider a lower budget for the per-directory hold.

#### PAR-D2-01: the soundtrack WAV's guard ends at `keep()`, and the leak fix is untested where it is used
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs`: `Audio.wav: Option<PathBuf>`, `discard_wav` (the only manual `remove_file` left), `open_unguarded` / `open_bink2` (`.inspect_err(|_| discard_wav(&audio))?`), and `decode_soundtrack`'s tests
- **Status**: NEW (residual of #7158, closed)
- **Description**: `decode_soundtrack` hands back a bare `PathBuf` once the samples are kept. A panic between that point and `Movie::streaming` taking the audio unwinds through `guard_open` and leaves the WAV; `discard_wav` covers only the explicit `Err` arm. No panic trigger is known. Separately, no video test checks that a panicking frame closure, an `Err`, or a zero-sample track leaves no `byroredux-movie-*` file; only the core `TempPath` test exists.
- **Suggested Fix**: carry the `TempPath` in `Audio` and in the `Movie` until playback ends, and drop `discard_wav`. Add a video-level leak test over the three exits (matching this process's file-name prefix).

#### PAR-D2-02: a binkaudio `Decoder` keeps the refused block's overlap after `Error::Overrun`
- **Severity**: LOW (latent: the only caller ends the track on any error)
- **Dimension**: Error Semantics
- **Location**: `crates/binkaudio/src/decoder.rs` `Decoder::decode`, `block` (`self.tail = …` runs before `decode`'s check)
- **Status**: NEW (introduced by #7163; documented in its doc comment)
- **Description**: `out` is rolled back, but the overlap carried into the next block is the refused block's. `BadSize` leaves the state untouched, so the two errors leave the decoder in different states.
- **Suggested Fix**: snapshot `tail` before the loop and restore it on `Overrun`, or document the decoder as unusable after any `Err`.

#### PAR-D2-03: a CDB index that keys or describes almost nothing still builds `Ok`
- **Severity**: LOW (PLAUSIBLE; vanilla is unaffected)
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs`: `stream_db_file_index` (`objects_by_key.insert(key, dbid)` and the #7154 `is_empty()` check), and `MaterialIndex::build`'s `idx.rows.len() != instances` check
- **Status**: NEW (siblings of #7154, closed)
- **Trigger Input**:
  - (a) a mod CDB whose `Objects` rows all carry one `(Dir, Ext)` PersistentID, for example a drift that zeroes both;
  - (b) a `DBFileIndex` with keyed `Objects`, no or empty `Components`, and no instances after it.
- **Description**:
  - (a) key collisions are last-wins with no count, so one surviving key passes the `is_empty()` gate.
  - (b) `0 == 0` passes the row/instance join, so every lookup resolves to an object with no component rows.
  - Both are the #5320 / #7154 silent-degrade class at its edges.
- **Suggested Fix**: count `insert(..).is_some()` into the existing "Objects table decode degraded" warning, and refuse an index with no component rows the way `NoKeyedObjects` does.

#### PAR-D2-04: DBFileIndex read errors and `DuplicateDbFileIndex` are still not attributed to a stream instance
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs` `MaterialIndex::build` (`stream_db_file_index(&mut state, &mut idx)?`, `Err(Error::DuplicateDbFileIndex)`)
- **Status**: NEW (residual of #7161, closed)
- **Description**: a reader EOF inside the index's own LIST (a cut `Objects` or `Components` row) surfaces bare, although `error.rs` says the index names itself.
- **Suggested Fix**: wrap with `AtInstance { index: pos, class: "BSComponentDB2::DBFileIndex" }`, leaving the already-typed variants alone.

#### PAR-D2-05: BSA, BA2 and CSG open-time table reads still fail with a bare "failed to fill whole buffer"
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**:
  - `crates/bsa/src/archive/open.rs`: the header, folder records, folder names, file records and file names reads;
  - `crates/bsa/src/ba2.rs`: the header and name-table reads;
  - `crates/bsa/src/csg.rs`: the header and chunk-table reads.
- **Status**: NEW (sibling of #7159, closed: it covered the extract-time reads only)
- **Trigger Input**: a truncated archive whose declared counts run past the file.
- **Description**: these reads name no table or index. #7159's issue also asked to drop the doubled outer labels: the plugin's "Compressed record X: ESM record X", and the provider's "'p' failed to extract: BA2 … of 'p'". The commit kept them, giving reasons.
- **Suggested Fix**: route the open-time reads through `read_exact_at_for` (or a cursor-side equivalent) with the table and index in the label.

#### PAR-D2-08: surplus data after a Bink Audio packet's blocks is accepted silently (PLAUSIBLE)
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/binkaudio/src/decoder.rs` `Decoder::decode`
- **Status**: NEW (the other side of #7163's check)
- **Description**: the overrun check refuses data that is too short. Data longer than the blocks it states is never compared with where decoding ended. In the fixture probe, appending 64 junk bytes to every packet decoded `Ok` with identical output (5 of 5). This only matters for a degraded soundtrack the sweep would not flag; it is not a safety issue.
- **Suggested Fix**: measure as A10 did (any installed packet whose aligned end is not `8 * len`?). If none, refuse or warn on surplus.

#### PAR-D4-01: the ledger test's remaining blind spots
- **Severity**: LOW (all latent; no field is falsely credited at HEAD)
- **Dimension**: Corpus Gates
- **Location**: `byroredux/src/asset_provider/tests/bgsm_merge.rs` `every_parsed_sidecar_field_is_merged_or_in_the_ledger`
- **Status**: NEW (residual of #7165, closed)
- **Description**:
  - **Ledger credit ignores which struct a field belongs to.** `glowmap`, `emittance_color` and the `adaptive_emissive_*` fields exist in both BGSM and BGEM, so one mention credits both.
  - **A sentence that excludes a field still credits it.** `receive_shadows` is credited by a line that says it is *not* in the ledger.
  - **The template-read check counts comments.** It searches `template.rs` with comments still in it.
  - **The comment stripper is hand-rolled.** It tracks only `"` per line, so `'"'` or `"\\"` desynchronise it; it ignores `/* */` and raw strings.
  - **`.base.` is unanchored on the left.** It would credit `BaseMaterial` for any future struct with a `base` field.
- **Suggested Fix**: key ledger entries per struct (`BGEM glowmap`), strip comments from the template text too, and move a real comment stripper into `byroredux_core::source_scan`.

#### PAR-D4-02: the meshes-prefix scan still misses several spellings, including one the issue listed
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `byroredux/src/workspace_hygiene_tests.rs` `no_production_code_prefixes_meshes_by_hand`
- **Status**: NEW (residual of #7166, closed). The close comment said the scan was "mutation-checked against the six spellings"; the issue listed seven, and `format!("{}\\{}", "meshes", x)` is still not flagged.
- **Description**: re-implemented and fed snippets. Missed:
  - `format!("{}\\{}", "meshes", x)`;
  - `r#"meshes\{x}"#`;
  - a leading separator (`"\\meshes\\{x}"`, `"/meshes/{x}"`);
  - `["meshes", x].join("\\")`;
  - `Path::new("meshes").join(x)`;
  - a placeholder before the root (`"{base}meshes\\{x}"`).

  No live offender. The `mitsuba_export.rs` allow-listing is justified, but it exempts the whole file rather than the one line. The matcher has no unit test of its own.
- **Suggested Fix**: move the matcher into a pure function with positive and negative cases, and handle `r#"`, a leading separator, and a bare `"meshes"` used as a `format!` / `join` argument.

#### PAR-D6-02: four other caches still fold their keys ASCII-only while the archives fold Unicode
- **Severity**: LOW (duplicate parse and upload only; reads resolve)
- **Dimension**: I/O & Paths
- **Location**:
  - `crates/renderer/src/texture_registry/mod.rs` `normalize_path` (the texture registry key);
  - `byroredux/src/scene/nif_loader.rs` (the `SceneImportCache` key);
  - `byroredux/src/asset_provider/material/provider.rs` (`normalize_material_path(..).to_ascii_lowercase()` keys);
  - `byroredux/src/asset_provider/archetype_idle.rs` (`idle_key`).
- **Status**: NEW (siblings of #7168, closed)
- **Description**: `Ä.dds` and `ä.dds` are two cache entries for one archived file, the #7168 class. No persisted (save) key is involved.
- **Suggested Fix**: fold through the same Unicode lowercase, and extend #7168's binary-side agreement test to these keys.

## Gate Matrix

| Crate | Size caps | Error policy | Version gate | Corpus sweep | CI lane |
|---|---|---|---|---|---|
| bsa | ✓ (BA2 label alloc, D1-02) | extract ✓ / open-time context gap (D2-05) | ✓ | ✓ strict (this pass) | manual-only `parsers` |
| bgsm | ✓ | ✓ | ✓ | ✓ strict | manual-only |
| sfmaterial | ✓ | degenerate indexes (D2-03), index errors (D2-04) | ✓ | ✓ strict (this pass) | manual-only |
| hkx / nif Havok | ✓ (#7155) | ✓ | ✓ | ✓ strict | manual-only |
| facegen | ✓ | ✓ | ✓ | ✓ strict | manual-only |
| menuxml | ✓ | ✓ | n/a | ✓ strict | manual-only |
| game-detect | ✓ | ✓ | n/a | n/a | default |
| bink2 / binkaudio | ✓ | overrun ✓, surplus plausible (D2-08), state after Err (D2-02) | ✓ | via video | default fixtures |
| video | ✓ (#7153) | WAV guard window (D2-01) | ✓ | ✓ census, whole soundtracks | manual-only |
| asset_provider / streamer (consumer) | loose bound loose (D1-03) | ✓ | n/a | n/a | default; streaming regression D6-01 |

Suggested next step: `/audit-publish docs/audits/AUDIT_PARSERS_2026-11-21b.md`. Labels: `import-pipeline` on every finding; `renderer` on D6-01; `game:starfield` on D2-03 and D2-04; `test-gap` on D2-01, D4-01 and D4-02.
