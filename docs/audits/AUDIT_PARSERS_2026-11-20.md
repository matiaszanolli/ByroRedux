# Parser Discipline Audit: 2026-11-20

**HEAD**: f3713bf5e · **Baseline**: `AUDIT_PARSERS_2026-10-31.md` (HEAD 670f81917) · **Audited**: all six dimensions (every dimension's paths changed since the baseline; the Bink 2, Bink Audio and video crates are new and were audited by this skill for the first time) · **Unchanged since baseline (skimmed)**: none. The crates with no commits on these paths were still spot-checked through their named guards: facegen, game-detect, and bgsm's reader.

This audit ran six dimension agents, three at a time (Sonnet), with the coordinator verifying their findings. No tracked file changed; the probes live in the session scratchpad and built into the repo's `target/`.

## Commits since 670f81917 on this skill's paths

| Area | Commits | What landed |
|---|---|---|
| Bink (new) | 8e11d49a4, 3ebecb7ec, c678f1065, e13628ab4, 030c1747a, 538992417, 706f95884, cb6388788 (#6981), 5ff9deba6 (#6959), aa193bd9e, 118afd645, 8b131d9e3 (#7068), f2d209b1f (#6896-#6898) | These commits add the clean-room Bink 2 and Bink Audio decoders and decode Bink 1 in process through `mercs2_bink`. They add header and packet ceilings, the decoder-panic guard, and the Failed verdict when a movie stops decoding. |
| hkx / nif | 68d3a834e (#6954), 9a6dd51dd (#6593) | One Havok container decoder now serves both hkx and nif. The inline weight counts everything a resolve copies. |
| sfmaterial | 7687e25c3 (#6957), 7f4f32c72 (#6951) | One object-chunk opener now serves read, skip and the file-index stream. The test fixtures moved behind the `test-support` feature. |
| bsa | 36b55c728, f70d041de, f41103b53 | Adds `dx10_layout` and `extract_dx10_mips` (M39) and the `lod_family` path classifier. |
| menuxml | d783d176d (#6994), 0acdb5c04, ad7bde431, bb986991e, 250a5ba67 | Fixes the font glyph x/y order and adds M48 input, dialogue, message-box, sleep and level-up work. |
| gates | a2fb1bb12 (#6482), 64f8d41fd (#6993) | FO76 counts become floors. The parser lane now also runs `spt`, `ui`, `pex` and `video`. |
| consumers | da29e5837 (#6956), da70dafa4 (#6483), 3d67d963e / 2e30cdf10 (texture streaming), ba3ee4307 (PlayBink containment), 6afdb7919 / e064c6f97 (#6846 / #6848) | These are the asset-provider, video and BGSM-merge changes. |

Baseline issues: #6480, #6481, #6482, #6483 and #6593 are all **CLOSED**, and each fix was confirmed in place. #6481's vanilla inline-headroom pin is `corpus_tests.rs:196-197`. #6482's FO76 floors are green in the strict lane below. #6483's ancestor cut was verified by Dim 6, with a residual filed as PAR-D6-01. #6593's `graph_weight` destructures every field with no `..` (Dim 1).

## Test runs

**Default lane** (`cargo test -p <10 parser crates>`, rustc 1.100.0): all green.

| Crate | Unit | Integration (run / ignored) |
|---|---|---|
| bsa | 120 (+11 ignored) | `ba2_real` 1/12, `bsa_real` 1/10, `csg_real` 0/1 |
| bgsm | 35 | `parse_all` 1/3 |
| sfmaterial | 51 | `header_smoke` 6, `real_cdb` 0/5 |
| hkx | 75 (+10 ignored) | none |
| facegen | 31 | `parse_real_facegen` 0/5 |
| menuxml | 68 | `{fnv,fo3,vanilla}_corpus` 0/14 |
| game-detect | 60 | none |
| bink2 | 19 | `probe_stream` 1 |
| binkaudio | 3 | `fixture` 2 |
| video | 14 (+1 ignored) | none |

Dim 6 also ran `cargo test -p byroredux --bin byroredux asset_provider`: 273 passed, 16 ignored.

**Strict real-data lane** (`BYROREDUX_REQUIRE_GAME_DATA=1 cargo test --release -p <crate> -- --ignored`, run one crate at a time, `BYROREDUX_STEAM_COMMON` set): **all green, nothing skipped**.

| Crate | Passed | Wall | Peak RSS |
|---|---|---|---|
| bsa | 34 (11 lib + 12 `ba2_real` + 10 `bsa_real` + 1 `csg_real`) | 10 m 34 s | 1.0 GB |
| bgsm | 3 | 1 s | 0.2 GB |
| sfmaterial | 5 | 4 s | 1.1 GB |
| hkx | 10 (including both behavior-graph censuses, the FO4 clip census and the LE/SE parity test) | 19 s | 0.85 GB |
| facegen | 5 | 2 s | 0.17 GB |
| menuxml | 14 | 5 s | 0.28 GB |
| video | 1 (`every_installed_bink_movie_decodes_to_its_last_frame`) | 9 m 42 s | 0.44 GB |

The movie sweep covers 101 files: Oblivion 9, FO3 35, FNV 1 and Skyrim SE 1 (all Bink 1); FO4 1 Bink 1 + 14 Bink 2; FO76 4 and Starfield 36 (Bink 2). That matches ROADMAP's 101 of 101.

**Fuzz and probe coverage.** These probes exercise the public APIs only; the clean-room rules were respected and no external Bink source was read. Every run gave **0 panics**:

| Target | Inputs | Notes |
|---|---|---|
| `bink2::Decoder` | ~980 k packets | random and mutated; slices 2/4, rev g/j, threads 1/4, SIMD on and off, debug with overflow checks |
| `Bk2` container | 140 k files | random, mutated and truncated |
| binkaudio | 103 k packets | |
| Bink 1 `Movie::open` + `decode_to_end` | 3,400 | mutated copies of a real `.bik` |
| hkx container and decoders | 60 k | mutations of 33 real Skyrim SE / FO4 files |
| BA2 `open` / `extract` / `extract_dx10_mips` | ~8 k | mutated copies of a real FO4 DX10 BA2 |

The probes also produced these real-corpus checks:
- All 179,878 FO4 `bhkSystemBinary` blobs parse through the shared #6954 container. All are v11, 64-bit, three sections, `hk_2014.1.0-r1`.
- `extract_dx10_mips` equals a full `extract` byte for byte across about 4,400 textures from BA2 v1/2/3/7/8, including 38 v3 LZ4 textures with a raw chunk.
- Probe and decode agree on every installed movie: frame count, soundtrack length within 0.01 s, and rate and channels.

## Executive Summary

| Severity | NEW | Existing (open) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 21 | 0 |

- **HIGH, PAR-D1-01.** The Bink 1 path never bounds picture size. A ~100-byte mod `.bik` stating 32767x32767 passes `probe` and makes `Movie::open` reserve ~1 GiB planes on the frame loop. Under any memory limit below ~5 GB the process aborts on the failed allocation, and `catch_unwind` cannot stop that. Bink 2 has refused this since BIK-AUD-3 (`MAX_PIXELS`); the Bink 1 arm never got the same check.
- **MEDIUM, PAR-D2-01.** #6957 unified the sfmaterial read and skip paths but left `stream_list`, the DBFileIndex LIST streamer, as a third copy. It has no `expect_consumed` and keeps its own count clamp. A corrupt or modded CDB whose `Objects` list under-counts builds an `Ok` index with no keyed objects: every material misses, and nothing is logged. This is the #5320 class.
- **LOW, Bink.** The Bink path accounts for half the findings (D1-02, D2-02, D2-04, D3-02, D3-03, D3-04, D5-01, D5-02, D6-02, D6-03). The pattern repeats: the Bink 2 path is hardened, and the Bink 1 arm, written first on top of the third-party decoder, lacks the same check. The Bink 1 arm is missing the fps-zero refusal, the track-ID rule, warning on audio errors, and a revision test.
- **LOW, the rest.** The nif-side Havok view stayed quadratic after #6954 (D1-03), and the ragdoll decoder is not gated on 64-bit v11 (D3-01). Three LOW findings sit on the M39 streaming path: a per-frame retry with no backoff (D2-05), mixed-source mips plus no `ArchiveStamp` revalidation in the streamer (D6-04), and BA2 EOF errors with no context (D2-07). The rest are logged at debug level (D2-06), sfmaterial errors that name no instance (D2-08), the BGSM ledger (D5-03), the loose-walker multiplier (D6-01), a sixth `meshes/` prefixer (D6-05), and the non-strict movie sweep (D4-01).
- **Merged:** Dim 2's "Bink 1 soundtrack swallows a `split` error" is the second half of PAR-D5-02 and is folded into it.
- **Cross-audit:** `mercs2_bink` is a dependency outside this skill's source rules; D1-01 and D2-04 are about the engine's checks around it. D6-02 (`find_case_insensitive`) sits in `systems/script_requests.rs`, which `/audit-scripting` owns, and is filed here because it gates the movie path.

## Findings

### HIGH

#### PAR-D1-01: Bink 1 pictures are bounded per side by a dependency but never by area in the engine; a 32767x32767 header reserves ~4.4 GiB before any frame decodes
- **Severity**: HIGH (raised from the agent's MEDIUM by the coordinator: the skill rates an abort or OOM reachable from an archive or mod file HIGH, and `_audit-severity.md` rates an allocation sized from an on-disk field with no engine cap HIGH. The dependency's 0x7fff-per-side limit still admits one plane of ~1 GiB from a 100-byte header. Coordinator re-read `crates/video/src/lib.rs:455-458`: `Movie::parse` goes straight to `VideoDecoder::new` with no size check.)
- **Dimension**: Size Discipline
- **Location**: `crates/video/src/lib.rs:449-486` (`Movie::open`, Bink 1 arm), `:127-151` (`probe`); contrast `crates/bink2/src/container.rs:60-72,93-98`
- **Status**: NEW (TD7-2026-11-15-02 / #6981 re-exported only the frame and track ceilings, not the dimension ones)
- **Trigger Input**: a loose `Data\Video\*.bik` whose header (offsets 0x14 / 0x18) states width = height = 32767 (the dependency's own `1..=0x7fff` limit), one frame, no audio track; ~100 bytes.
- **Description**: Bink 2 refuses `width > MAX_DIMENSION` 8192, an area past `MAX_PIXELS` (4096x4096) and zero sides at `Bk2::open` because a header must not be able to size a frame buffer (BIK-AUD-3). The Bink 1 arm hands the file to `mercs2_bink::Movie::parse` + `VideoDecoder::new` with no engine-side dimension check; `probe` reports the dimensions as `Ok`. The only bound is the dependency's per-side 0x7fff, i.e. ~1.07 Gpx. `VideoDecoder::new` then reserves its plane buffers up front (calloc-style, so resident memory stays ~10 MB until a frame is written); with a stream that does decode, `mercs2_bink::rgb::to_rgba` adds a further 4 B/px (4.3 GB) per frame.
- **Evidence**: probe `bink_probe bik1 32767 32767 1` (`BIK1_PLANE=64`): `probe: Ok(MovieInfo { width: 32767, height: 32767, .. })`, `Movie::open` ok at `ulimit -v 5000000`; at `ulimit -v` 800 MB, 2.5 GB, 3.0 GB, 3.4 GB, 3.8 GB and 4.4 GB the process dies with `memory allocation of 1073741824 bytes failed` (exit 134, SIGABRT: Rust's allocation-failure abort, not catchable by `guard_decoder`'s `catch_unwind`). The same file at 4096x4096 opens under `ulimit -v` 600 MB. 65535 and larger are refused by the dependency (`a 65535x65535 movie (§1.1: 1..=0x7fff each way)`).
- **Impact**: a hostile or merely absurd mod movie aborts the whole engine on a memory-constrained or strictly-overcommitted system, and on any system a real ~10 MB Bink 1 bitstream of flat blocks at that size would touch ~1.6 GB of planes + 4.3 GB of RGBA. Reached from the movie player (`byroredux/src/video.rs:263`, `Movie::open` on the frame loop) for any file in `Data\Video`.
- **Related**: BIK-AUD-3 (Bink 2 side), TD7-2026-11-15-02 / #6981, PAR-D1-02. Not a clean-room question: the fix is engine-side and reads only header fields.
- **Suggested Fix**: in `probe` and `Movie::open`, after `Movie::parse`, refuse (as `VideoError::Unreadable`) a header whose sides exceed `bink2::MAX_DIMENSION` or whose area exceeds `MAX_PIXELS` (re-export those two as `MAX_AUDIO_TRACKS` / `MAX_FRAMES` already are) before `VideoDecoder::new`; add a test beside `formats_without_a_decoder_are_unsupported_and_damage_is_unreadable` using the 100-byte header above.


### MEDIUM

#### PAR-D2-01: `stream_list` (the DBFileIndex LIST streamer) has no trailing-bytes check and its own copy of the count clamp; a short count or trailing bytes build an Ok index, a zero-count `Objects` list builds an Ok index with no keyed objects
- **Severity**: MEDIUM
- **Dimension**: Error Semantics
- **Location**: crates/sfmaterial/src/index.rs:885 (`stream_list`), vs crates/sfmaterial/src/reader.rs:694 (`skip_list`, `expect_consumed`) and :713 (`checked_container_count`)
- **Status**: NEW (the coordinator confirmed this at `crates/sfmaterial/src/index.rs:885-900`: there is no `expect_consumed` after the loop, and the clamp is inline rather than through the shared `checked_container_count`) (#6957 consolidated consume_list/skip_list onto `expect_consumed` + `checked_container_count` but left `stream_list` as a third, divergent copy — the read/skip/stream drift #6957 set out to remove)
- **Description**: `consume_list` and `skip_list` end with `expect_consumed(&cur)` and take their count from `checked_container_count`. `stream_list` (used for the DBFileIndex `Components` / `Objects` / `Edges` side chunks) re-implements the count clamp inline and never calls `expect_consumed`. So a LIST chunk whose declared count is lower than the elements it holds, or that carries trailing bytes, is accepted silently, and the unread elements are dropped. `Components` is protected only indirectly by the row/instance join (`RowInstanceMismatch`); `Objects` (the PersistentID -> DBID key table) and `Edges` have no such backstop, and the `NoKeyedObjects` gate (#5899) only fires when `keyable_rows > 0`, so an `Objects` list that decodes to zero rows is not an error either. The result is the #5320 class: a degraded build that looks successful (every `.mat` lookup misses -> PBR fallback, no error, no warning).
- **Evidence**: probe (`sflist`, synthetic CDB from `index::test_support`, HEAD source): unmodified build `Ok(1 material)`. Appending 8 trailing bytes to each of the three DBFileIndex LIST chunks: `Ok(1)` for all three (a whole-value `consume_list`/`skip_list` would return `ObjectTrailingBytes`). Lowering the `Objects` LIST count 1 -> 0: `Ok(0)` — the index builds with zero keyed objects, no error. Lowering `Components` 9 -> 8: `Err(RowInstanceMismatch)` (the only backstop). Negative count: `Err(NegativeCount)`.
- **Impact**: A corrupt or mod-authored Starfield CDB whose DBFileIndex lists disagree with their payload loads with every material unresolved and nothing in the log; vanilla is unaffected (the five real-CDB tests pass, counts exact). Silent-degrade, not memory-unsafe.
- **Trigger Input**: Starfield `materialsbeta.cdb` whose DBFileIndex `Objects` LIST chunk has `count` < elements present (e.g. 0) or bytes after the last element.
- **Related**: #5320, #5899, #6957, #2623
- **Suggested Fix**: make `stream_list` call `checked_container_count` (export it) and finish with `expect_consumed(&cur)`; make a build whose `Objects` list yielded zero keyed objects an error regardless of `keyable_rows`; add a test beside `build_rejects_trailing_bytes_in_the_dbfile_index` per list chunk.


### LOW

#### PAR-D1-02: one zero-filled 13 MB audio packet reaches the whole one-hour soundtrack ceiling: ~15.7 s blocking decode and ~1.35 GB RSS (+1.38 GB temp WAV) inside `Movie::open`
- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `crates/video/src/lib.rs:191-195,295-321` (`pcm_cap`, `bink2_audio`), `crates/binkaudio/src/decoder.rs:91-123` (`Decoder::decode`, `min_block_bits`)
- **Status**: NEW (residual of the accepted BIK-AUD-1 design: 538992417 capped the ceiling at one hour rather than at what the file could justify)
- **Trigger Input**: a `.bk2` with `frames = 1_000_000`, `fps = 1/1` (duration clamps to 3600 s), one stereo 48 kHz DCT track, and in frame 0 one audio packet of ~13.3 MB of zero bytes stating `decoded_bytes = 1,382,400,000` (any value up to the cap in bytes; here exactly the 691.2 M-sample cap).
- **Description**: the `decode` bound (`blocks * min_block_bits <= 8 * data.len()`) is correct but lets a zero packet expand 104x (7,680 B of PCM per 74 B). The caller's only per-packet limit is the whole-movie `pcm_cap`, so a single packet may consume all of it, synchronously, on the caller's thread: `Movie::open` is called from `VideoPlayer::step` on the frame loop (`byroredux/src/video.rs:263`). Header fields can pick the cap up to `MAX_SOUNDTRACK_SAMPLES` 691.2 M samples.
- **Evidence**: probe `bink_probe abomb 180000` (release build, data = `vec![0; 13_275_008]`, size = 180000 blocks): `decode` returns Ok, 691,200,000 samples in 15.73 s, max RSS 1,350,716 kB, 104.1x the input; `write_wav` then writes 1.38 GB through a `BufWriter` of 2-byte `write_all`s to `temp_dir()` (tmpfs on many systems, i.e. more RAM). 20,000 blocks from 1.47 MB took 1.74 s.
- **Impact**: a hostile mod movie freezes the game for tens of seconds and spends ~1.4-2.8 GB of RAM / tmpfs; it is bounded (no abort on typical hardware), which is why it is LOW. Honest files are unaffected: all 236,690 corpus packets fit a per-frame bound by construction.
- **Related**: BIK-AUD-1 (INC3-1 in the decoder), PAR-D1-01.
- **Suggested Fix**: also bound each packet by the time one frame may hold (`rate * channels * (1/fps + slack)` samples, slack ~2 s) before `Decoder::decode`, so a packet cannot spend the whole-movie budget; decode and stream the WAV per frame instead of holding the PCM `Vec`.

#### PAR-D1-03: `HavokPackfile::objects()` / `class_name_at` is O(virtual fixups x class names); a 2.85 MB `bhkRagdollSystem` blob takes 4.5 s
- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `crates/nif/src/blocks/collision/havok_packfile.rs:215-243` (`class_name_at`, `objects`), reached from `crates/nif/src/blocks/collision/np_ragdoll.rs:319` (`decode_np_ragdoll`) and `crates/nif/src/import/collision/ragdoll.rs:371`
- **Status**: NEW (the same shape was fixed for the hkx reader in #4648/#4649; #6954 shared the *container* but the nif view kept its own lookup)
- **Trigger Input**: an FO4 NIF whose `bhkRagdollSystem` blob is a valid v11 packfile with N one-character class-name records (7 B each) in `__classnames__` and N virtual fixups in `__data__` (12 B each), every fixup naming the last record.
- **Description**: `class_name_at` does `class_name_offsets.iter().position(..)` per call and `objects()` calls it once per virtual fixup. Both tables are only file-bounded, so the work is quadratic in the blob size. `Fixups::pointer` / `class_at` in `np_ragdoll.rs` do the same linear `find` per joint, but those are capped by `MAX_ELEMENTS` 256.
- **Evidence**: probe `hk_probe <N>` (release; blob built by hand, parses `Ok`): N=20,000 (380 KB) objects() 80 ms; N=80,000 (1.5 MB) 1.28 s; N=150,000 (2.85 MB) 4.47 s; parse itself 6.7 ms.
- **Impact**: a modded ragdoll skeleton NIF stalls its importer for seconds per MB of blob; nothing allocates unboundedly. Vanilla blobs hold five objects.
- **Related**: #6954, #4648, #4649.
- **Suggested Fix**: build a `HashMap<u32, usize>` (or sort `class_name_offsets` and `binary_search`) once in `parse_havok_packfile` and use it in `class_name_at`; cap virtual-fixup / class-name counts the way `MAX_ELEMENTS` does for the ragdoll arrays.

#### PAR-D2-02: Bink 2 bit reader reads zeros past the end of a packet and nothing checks the over-read: a truncated video packet decodes `Ok` to a different picture, never `Failed`
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: crates/bink2/src/bits.rs:62 (`peek`, `bit`), crates/bink2/src/lib.rs:226 (`decode_into`) / :413 (`decode_slice`); crates/binkaudio/src/bits.rs (same, audio)
- **Status**: NEW (the zero-fill is a deliberate documented choice — "ours, not probed" — and binkaudio bounded the *size* side in 706f95884, but no end-of-slice `r.pos > 8*data.len()` verdict exists on the video side)
- **Description**: `Bits` returns 0 for every bit past `data`. `decode_into` never compares the final `r.pos` with the packet length, and `Bk2::video` accepts any packet length the (validated) frame index gives. A packet cut short by the encoder/mod therefore finishes decoding the remaining macroblocks from zero bits, returns `Ok(&Frame)`, and `Movie` reports a clean frame instead of `Failed(frame N does not decode)`; contract (c) wants a documented Ok+warn or an Err.
- **Evidence**: probe `short` on the 96x32 probe stream's frame 0 (24-byte packet): full packet -> Y-plane FNV `65961a2b50454325`; packet cut to 4 bytes (flags word only) -> `Ok`, Y-plane `bb2cbe5e5ab51325` (a different picture); cuts 12..23 -> `Ok` and identical to full (the tail is genuinely zero bits). Only cuts below 4 bytes give `Err(ShortPacket)`. Same at threads 1 and 4.
- **Impact**: corrupt/mod Bink 2 files show garbage frames and play to the end as Played. Not a safety issue (0 panics in 900k fuzz decodes).
- **Trigger Input**: KB2g/KB2j video packet whose bitstream is shorter than its macroblocks need (e.g. 4-byte packet for a 96x32 keyframe).
- **Related**: #6855, BIK-DEC-004
- **Suggested Fix**: do NOT simply error on any over-read: the probe shows valid streams may omit trailing zero bits. Measure the corpus first (max over-read bits per slice across the installed `.bk2` set); then warn or `Err(Error::Overrun)` above the measured maximum, and pin it with a test.

#### PAR-D2-04: the main-thread open path (`Movie::open` / `probe`: container parse, `VideoDecoder::new`, both soundtrack decodes) runs outside `guard_decoder`
- **Severity**: LOW (hardening; no panic reproduced)
- **Dimension**: Error Semantics
- **Location**: crates/video/src/lib.rs `Movie::open`, `open_bink2`, `extract_audio`, `bink2_audio`; call site byroredux/src/video.rs:263 (main thread, `step` -> `start`)
- **Status**: NEW (BIK-DEC-004 guards only the reader thread; SKILL notes unguarded main-thread callers generally)
- **Description**: `spawn_reader`/`guard_decoder` cover the frame loop. `Movie::open` itself — `mercs2_bink::Movie::parse` + `VideoDecoder::new` (third-party 0.1.0), `extract_audio` (`AudioDecoder::decode_record`), `bink2_audio` (`binkaudio::Decoder::decode`, `Bk2::audio`) — runs on the engine's main thread with no `catch_unwind` (grep: only texture_streamer, terrain_height, cdb, pre_parse, fog_volumes, video reader use one). A panic in any of them aborts the process instead of ending the movie `Failed`.
- **Evidence**: none reproduced: 3,400 mutated Bink 1 `open` calls, 100k binkaudio packets and 120k container files gave 0 panics. The exposure is structural: the soundtrack decode is the longest-running stream-driven code on the main thread, and `mercs2_bink` is outside the audit's source rules.
- **Impact**: latent; a future decoder change or a mercs2_bink panic on a hostile `.bik` kills the engine, not the movie.
- **Trigger Input**: any `.bik`/`.bk2` reaching `Movie::open` (mod-supplied Video folder).
- **Suggested Fix**: run `Movie::open`'s body under the same `catch_unwind` -> `VideoError::Decode` conversion (or decode the soundtrack inside the reader thread's guard); add a hygiene scan next to `spawn_reader_is_the_only_reader_thread`. (Also: if `spawn_reader(..)?` fails after `extract_audio` succeeded, the temp WAV is leaked — trivial.)

#### PAR-D2-05: texture-streamer promotion failure has no retry backoff, logs a misleading per-frame warning, and a short read is a debug line
- **Severity**: LOW (streaming is off by default until #6631)
- **Dimension**: Error Semantics
- **Location**: byroredux/src/texture_streamer.rs:171-198 (`step`), crates/renderer/src/texture_registry/upload.rs:506 (`apply_wanted`) / :1568 (`note_declined_changes`)
- **Status**: NEW
- **Description**: A pool read that returns `bytes: None` (an `Err` from `extract_dx10_mips`, a panic caught by #6810, or a true miss) hits `log::warn!("texture promotion read for handle N found no archive entry")` — wrong text for an extract error/panic, and `or_warn` has already warned once per entry. Nothing marks the handle failed: `retry_after_frame` is only set by `note_declined_changes` (GPU-side declines), so the next frame's `apply_wanted` re-requests the same promotion, `take_promotion_requests` re-spawns the read (a full chunk read + inflate on the pool) and `step` warns again — every frame while the texture stays wanted. When bytes arrive but `queue_promotion_bytes` returns false (payload shorter than the request: `payload_first_level` exact-length rule), only a `debug!` "was not queued (released or already there)" is logged, with the same re-request loop. The #6810 test documents "the handle can be requested again" but not at frame rate.
- **Evidence**: by reading (no GPU probe): `source.retry_after_frame` has exactly two writers (`note_declined_changes`, test fixtures); `apply_wanted` gates candidates only on `queued.contains_key` and `frame >= retry_after_frame`.
- **Impact**: a BA2 whose DX10 entry fails (short non-final chunk, corrupt deflate; vanilla measured zero, mods not) floods the log and burns pool CPU per frame once streaming is on by default.
- **Trigger Input**: streamed BA2 DX10 texture whose non-final chunk decodes short or whose zlib stream is corrupt.
- **Related**: #6810, #6631, #6737 (the floor-first sibling, closed)
- **Suggested Fix**: on a failed/short read call a registry method that sets `declined_target`/`retry_after_frame` (or a permanent `unreadable` mark), and word the warning by cause.

#### PAR-D2-06: a present-but-undecodable EGM or cart-idle HKX is logged at `debug`
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: byroredux/src/npc_spawn/resumable/runtime.rs:1191-1199 (`EgmFile::parse` Err), byroredux/src/asset_provider/animation.rs:105-113 (cart-idle `decode_animation` Err)
- **Status**: NEW (every other hkx/facegen consumer — actor_graph, archetype_idle, object_graph, the other animation.rs arms — warns)
- **Description**: The file was found in an archive (so the data is defective, not absent) but the failure is `log::debug!`, indistinguishable from "this NPC has no EGM". The NPC renders with the unmorphed head; the IDLE loop falls through to "no character animation HKX" (also debug). Contract (c): a degraded decode must not be silent.
- **Impact**: a corrupt/mod EGM silently loses facegen; vanilla census has zero undecodable sidecars.
- **Trigger Input**: `.egm` with size != `64 + morphs*(4+nv*6)`; `.hkx` failing `decode_animation` under an IDLE event candidate path.
- **Suggested Fix**: `warn!` once per path (an NPC loop would otherwise repeat it).

#### PAR-D2-07: BA2 chunk-read and inflate EOF errors carry no entry or chunk context
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: crates/bsa/src/ba2.rs:1038 (`read_chunk_payload`), `decompress_chunk` zlib arm
- **Status**: NEW (every other BA2 error names the entry, e.g. the #4662 short-chunk message and the zlib over-run message)
- **Description**: A record whose chunk offset/size runs past the file, or a truncated deflate stream, surfaces as the bare `UnexpectedEof: failed to fill whole buffer` / `incomplete deflate stream` — no path, chunk index, offset or archive. Open does not bound record offsets by file length (only the name table, #4670), so a truncated BA2 fails per entry at extract time with these. Consumers going through `or_warn` add the entry path, but not which chunk; direct callers (tools, `extract_dx10_mips` used by the streamer) get nothing.
- **Evidence**: probe `fuzzba2` (8,000 mutated/truncated FO4 DX10 BA2s) collected distinct `extract` / `extract_dx10_mips` errors: `UnexpectedEof: failed to fill whole buffer` and `UnexpectedEof: incomplete deflate stream`, versus named messages for every other failure.
- **Trigger Input**: truncated BA2 (record table intact, data cut) — the #4670 real case `cuwp - textures.ba2` shape.
- **Suggested Fix**: map `read_exact_at` / inflate EOF in `read_chunk_payload`/`decompress_chunk` to an error naming `path`, chunk index, `offset` and the file length.

#### PAR-D2-08: sfmaterial stream errors name no chunk index or class
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: crates/sfmaterial/src/reader.rs:1158 (`Cursor::read_bytes` -> `Error::UnexpectedEof { offset }` is chunk-relative), `ObjectTrailingBytes { leftover }`; call sites crates/sfmaterial/src/index.rs:259-266 (`build` loop)
- **Status**: NEW
- **Description**: `build` walks up to 1.44 M instances and propagates `consume_top_level_value`/`skip_top_level_value` errors with `?`. `UnexpectedEof` reports an offset relative to the chunk, and `ObjectTrailingBytes` a leftover count; neither names the stream position (`pos` is known in the loop) or class (`class_name` is known). A failure deep in the instance stream is not attributable.
- **Trigger Input**: any malformed instance chunk in a large CDB.
- **Suggested Fix**: wrap in `build`'s loop with `pos` and `class_name` (an `Error::AtInstance { index, class, source }`).

#### PAR-D3-01: `decode_np_ragdoll` hard-codes 64-bit Havok 2014 offsets but no longer (or ever) checks the file says so, and #6954 widened what reaches it
- **Severity**: LOW
- **Dimension**: Version Gating
- **Trigger Input**: a `bhkRagdollSystem` blob whose header says pointer width 4 (`byte 0x10 = 4`), or v8/`hk_2010` contents, but which still carries a virtual fixup naming `hknpRagdollData`.
- **Location**: `crates/nif/src/blocks/collision/np_ragdoll.rs:303-345` (`decode_np_ragdoll`), `crates/hkx/src/container.rs:70-78` (the `4 | 8` and `8 | 11` allowance)
- **Status**: NEW
- **Description**: the module doc states "little-endian, 64-bit pointers" and every row offset (`hkArray` count at `+8`, member stride 16, `+0` pointers) assumes it, but `decode_np_ragdoll` never consults `pack.header.layout_rules[0]` or `file_version`; its only selector is the class name `hknpRagdollData`. Before #6954 nif accepted any version; after it the container admits 32-bit and v8 packfiles for hkx's sake, so the "nif is now as strict as hkx" statement is true of the container but not of what this decoder needs (it needs strictly v11 + 64-bit, which is stricter than hkx's own set).
- **Evidence**: probe1 `np`: real `character\characterassets\skeleton.nif` ragdoll blob decodes to 18 bodies / 17 constraints; with only `blob[0x10] = 4` it still decodes to 18 / 17 (the 64-bit offsets are applied to a file that says 32-bit). Setting v8 + `hk_2010` fails only incidentally (section header size shifts: "global fixups precede local fixups"), not by a gate.
- **Impact**: none for shipped content (all 75 FO4 ragdoll blobs are v11 / 8-byte pointers). A hostile or corrupt blob decodes to a garbage skeleton instead of an `Err`; every read is bounds-checked, so no panic/OOM. Contract (c) "silent" only.
- **Related**: #6954 (container unification); `parks_authored_pose` consumer unaffected.
- **Suggested Fix**: in `decode_np_ragdoll`, after `parse_havok_packfile`, require `pack.header.file_version == 11 && pack.header.layout_rules[0] == 8` and add a test mutating those bytes of the synthetic blob.

#### PAR-D3-02: bink2 decodes any `KB2?` revision; the g/j gate lives only in `byroredux-video`
- **Severity**: LOW
- **Dimension**: Version Gating
- **Trigger Input**: magic `KB2` + any 4th byte other than `g`/`j` fed to `Bk2::open` + `Decoder::for_file`.
- **Location**: `crates/bink2/src/container.rs:84,100` (`&h[..3] != b"KB2"` only; `extra = if h[3] >= b'j'`), `crates/bink2/src/lib.rs:242,404` and `inter.rs:322` (`rev >= b'j'`), gate at `crates/video/src/lib.rs:27-28,84-87`
- **Status**: NEW
- **Description**: `Bk2::open` checks three magic bytes. A revision below `j` is treated as `g` layout, anything `>= j` (including `k`..`z`) as `j` layout with the 4 extra header bytes. The only refusal is `BINK2_REVISIONS` in the video crate. The in-engine consumer is gated, so nothing mis-decodes today; the library itself, and `binkaudio` (which reads packets through `Bk2`), would.
- **Evidence**: probe3: KB2j file with byte 3 set to `k` or `z` decodes 6 frames with the same hash as the genuine KB2j; KB2g with `f` or `i` decodes identically to genuine KB2g. Wolfenstein's real KB2i (`bink/outro/outro.bk2`) fails with "audio packet past the frame" only because its extra header word is mis-sized as `g`, i.e. by luck.
- **Impact**: defense-in-depth only; a second consumer of `bink2` would not get the contract the SKILL attributes to "any other revision is Unsupported".
- **Related**: #6981 (container ceilings re-exported for video); README already says other revisions are "Not decoded".
- **Suggested Fix**: move the revision check into `Bk2::open` (`bad("unsupported Bink 2 revision")` for anything but `g`/`j`) and keep the video crate's `Unsupported` classification by sorting on the magic before calling it.

#### PAR-D3-03: no Bink 2 header flag except video bit 1 is inspected, and the video crate's `Unsupported` doc says otherwise
- **Severity**: LOW
- **Dimension**: Version Gating
- **Trigger Input**: a KB2g/KB2j file whose `video_flags` (`+0x24`) carry a bit that changes the stream layout (a plane the decoder does not model), or whose audio track flags carry bits other than 12/13.
- **Location**: `crates/bink2/src/container.rs:153` (`slices()` reads bit 1 only), `crates/video/src/lib.rs:36-37` (doc: "an alpha or grayscale plane" is `Unsupported`)
- **Status**: NEW
- **Description**: `Bk2` stores `video_flags` and uses only bit 1. There is no refusal for any other bit. STATE.md lists alpha planes as unprobed and README says they are "Not decoded", but nothing makes that an `Unsupported`; the `VideoError::Unsupported` doc and the PRD wording imply the video crate refuses an alpha/grayscale plane, which is true only on the Bink 1 path (`mercs2_bink::VideoDecoder::new`, probe2: `video flags 0x100000: alpha planes are not supported`). CLEANROOM.md forbids taking the bit value from the NihAV exposure, so the fix needs its own probe.
- **Evidence**: probe3 toggles each of the 30 other bits on a real KB2g and a real KB2j file: all 6 sampled frames hash identically to the unmodified file for every bit (0, 2..31). Audio: `Decoder::new` receives only rate/stereo/dct.
- **Impact**: none for the 54 corpus files (all exact versus the DLL). A mod movie with a plane the decoder lacks would be decoded with the wrong stream model: most likely a desync `Err` (movie ends `Failed`), possibly wrong pixels.
- **Related**: #6907 (provenance), STATE.md open item 1.
- **Suggested Fix**: probe which video-flag bits real alpha files set against the oracle (not from the disclosed third-party source), then refuse them in `Bk2::open`; until then correct the `VideoError::Unsupported` doc to say the alpha/grayscale refusal is Bink 1 only.

#### PAR-D3-04: Bink 1 `Movie::open` revision / alpha / grayscale refusals are unpinned
- **Severity**: LOW
- **Dimension**: Version Gating (guard coverage)
- **Trigger Input**: `.bik` with a revision other than `i`, or flags 0x20000 / 0x100000, through `Movie::open`.
- **Location**: `crates/video/src/lib.rs:449-460` (no explicit gate; relies on `mercs2_bink::Movie::parse` / `VideoDecoder::new`), test at `lib.rs:797-818`
- **Status**: NEW
- **Description**: the SKILL/doc contract is "unknown revision is `Unsupported`, never mis-decoded". The only Bink 1 revision test (`formats_without_a_decoder_are_unsupported_and_damage_is_unreadable`) covers `probe()` with `BIKb`; `Movie::open` is covered for KB2a only. The open-path refusal depends on a third-party crate pinned `=0.1.0` and is not asserted anywhere.
- **Evidence**: probe2 shows it works today for revisions b/f/g/h/j/k/m and both flags; there is no test that would fail if a `mercs2_bink` bump started accepting them.
- **Impact**: a dependency bump could silently turn "Skipped" into a mis-decode. Test gap only.
- **Related**: #6907.
- **Suggested Fix**: add an explicit `BIKi` check in `Movie::open` (the same one `probe` does) plus a test that writes a `BIKb` / flag-0x100000 header through `Movie::open`.

#### PAR-D4-01: The Bink movie sweep skips a whole title silently and pins no count, so the "101 of 101" ROADMAP claim can shrink without the gate going red
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/video/src/lib.rs:866-917` (`every_installed_bink_movie_decodes_to_its_last_frame`)
- **Status**: NEW. #6993 put the sweep in the parsers lane (`real-data-gates.yml:211`, the `video` entry in the crate loop) but did not change the sweep itself.
- **Trigger Input**: a title's `Data/Video` folder that is absent or unreadable, for example a renamed install, a missing DLC, or a moved Steam library. Also a file whose 3-byte magic read fails.
- **Description**: The sweep walks seven `(game, folder)` pairs. A failed `read_dir` is a bare `continue`, and so is a failed magic read. The only coverage assertion is `decoded.iter().sum() > 0`. The sweep ignores `BYROREDUX_REQUIRE_GAME_DATA` (it keys off `BYROREDUX_STEAM_COMMON` alone), unlike every other parser real-data suite (#3850, #4660). Neither per-title nor total counts are pinned, although ROADMAP quotes "101 of 101 movies" (§ Video column, 2026-11-13). This rule (contract d) "cannot go green vacuously" is the one #4660 closed for the other parser crates.
- **Evidence**: The strict-lane run in this audit counted 101 movies (9 / 35 / 1 / 1 / 15 / 4 / 36). If any six of the seven folders went missing, the test would still pass on the seventh. `let Ok(entries) = std::fs::read_dir(..) else { continue };` is at `:880`.
- **Impact**: A missing title's movies, or a decoder regression on the only files of one title (Oblivion's nine Bink 1 movies, say), cannot turn the lane red if that folder stops being read. The ROADMAP figure goes stale without anyone noticing. The test is green today and correct about what it does decode.
- **Related**: #6993, #4660, #3850, PAR-D3-04.
- **Suggested Fix**: Under `BYROREDUX_REQUIRE_GAME_DATA=1`, panic on an unreadable folder. Pin the per-title (Bink 1, Bink 2) counts measured here as exact, or as floors for FO76 the way #6482 did, and print a re-measure line when a count changes.

#### PAR-D5-01: A Bink 1 header with a zero fps denominator is accepted, then played at an invented 25 fps with a 3-second soundtrack cap
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Trigger Input**: Bink 1 (`BIKi`) header whose fps divider field is 0 (or whose dividend is 0). Not in any installed file; reachable from a mod's `.bik`.
- **Location**: `crates/video/src/lib.rs:72-81` (`describe`), `:600` (`Movie::advance`), `:221`/`:225` (`pcm_cap` input); contrast `:89` (`bink2_container`).
- **Status**: NEW
- **Description**: `describe` turns a non-positive fps divider into `fps = 0.0` and `duration_secs = 0.0` instead of an error, and `Movie::open` accepts it. The Bink 2 path refuses the same condition (`file.fps_num == 0 || file.fps_den == 0` -> `Unreadable`). Downstream, the consumer substitutes defaults for the missing decoded field: `advance` uses `25.0` when `info.fps <= 0.0`, `byroredux/src/video.rs:269` computes `frames = round(0.0 * 0.0) = 0` for `video.status`, and `extract_audio` computes `pcm_cap(duration_secs = 0, ..)` = (0 + 2 s) x 1.5 = 3 s of PCM, so a movie with a real soundtrack has its audio truncated at 3 s (a warn is logged, the outcome is still `Played`).
- **Evidence**: probe `fps0` over a handcrafted 16x64 Bink 1 header: `fps divider 0` -> `probe: Ok(MovieInfo { fps: 0.0, duration_secs: 0.0, .. })` and `Movie::open: ok, info { fps: 0.0, duration_secs: 0.0 }`. (Header offsets: width/height/fps at 0x14/0x18/0x1c/0x20 as the parser reads them; confirmed by the parsed values.) Source: `let fps = if self.info.fps > 0.0 { self.info.fps } else { 25.0 };`.
- **Impact**: A damaged or hostile Bink 1 file plays at a made-up rate with its frame total reported as 0 and (if it has sound) 3 s of audio; no Failed/Skipped is reported, so BIK-DEC-001 ("at the file's frame rate") is silently replaced by a consumer default. Not reachable from vanilla content.
- **Related**: BIK-DEC-001, BIK-RPT-001; the Bink 2 refusal at `bink2_container`.
- **Suggested Fix**: Refuse `fps.0 == 0 || fps.1 == 0` in `describe`'s callers (`probe` / `Movie::open`) as `VideoError::Unreadable`, the same condition `bink2_container` already refuses, and drop the `25.0` fallback.

#### PAR-D5-02: Bink 1 soundtrack path ignores the track-ID rule and ends the audio silently on a missing record or a split error; `video.status` reports Played either way
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Trigger Input**: (a) a Bink 1 file with several audio tracks whose first is not the ID-1 one (the installed Skyrim `BGS_Logo.bik`, 2 tracks, and a mod's `threeweekslater_withSound.bik`, 2 tracks, have more than one); (b) a Bink 1 frame whose `movie.split(frame)` errors or yields no audio record.
- **Location**: `crates/video/src/lib.rs:207-229` (`extract_audio`; `movie.tracks.first()` :210, `.ok().and_then(..)` :220, `Ok(false)` :221) vs `:260-265` (`original_track_index`) / `:269-` (`bink2_audio`); `byroredux/src/video.rs:256-285`, `commands/video.rs:43-45`.
- **Status**: NEW. This entry absorbs Dim 2's PAR-D2-03, whose truncation probe cut a real `BGS_Logo.bik` to 99/90/60/30 %. The soundtrack WAV shrank from 1,221,364 to 1,205,804, 852,524 and 506,924 bytes with no audio-side log line, while the video side warned that frame 146 runs past the file.
- **Description**: BIK-DEC-002 and `docs/engine/video-playback.md` §2 state one rule — play the track with ID 1, else the first — but only the Bink 2 path applies it (`original_track_index`); the Bink 1 path hard-codes `tracks.first()`. Second, the Bink 1 frame callback converts a `split` error into `None` (`.ok()`) and `Ok(false)` ("end quietly") with no log line, while the Bink 2 path logs "its audio stops at frame N" for a decode error and skips (does not stop at) a frame with no packet. So a Bink 1 soundtrack can end early with no diagnostic. In both paths a soundtrack that never started or stopped early still yields `VideoOutcome::Played { ended_early: false }`; the only trace is the crate's `log::warn!` and `soundtrack=no` in the `playing` line, not in `video.status`.
- **Evidence**: `let track = movie.tracks.first()?;` and `let record = movie.split(frame as _).ok().and_then(|(records, _)| records.first().copied()); let Some(record) = record else { return Ok(false) };` vs Bink 2 `let Some(packet) = packet else { return Ok(true) };`. Probe `dim5probe`: every installed Bink 1 movie with audio, including the Skyrim/FO4 two-track files, decode a soundtrack as long as the picture (no silent truncation in the corpus), and the doc states Skyrim's two tracks are sample-identical, so there is no observed misbehaviour today.
- **Impact**: Latent. A mod's multi-track Bink 1 file whose tracks differ plays track 0 whatever its ID, and a corrupt record mid-file silently shortens the sound (the clock then falls back to the wall clock, `movie_clock`). Corpus unaffected.
- **Related**: BIK-DEC-002 (#6896), BIK-DEC-004 (frames are reported Failed, audio is not).
- **Suggested Fix**: Take the track's ID through the same `original_track_index` once the `mercs2_bink` track exposes it (or document that Bink 1 plays track 0), warn on a `split` error / missing record the way `decode_soundtrack` warns for an `Err`, and add a "soundtrack ended early / absent" note to `video.status` (or the `Played` outcome) so a silent play is visible there.

#### PAR-D5-03: The #6848 "remaining parsed-but-unmerged groups" ledger still omits seven parsed BGSM/BGEM groups
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Trigger Input**: parsed BGSM/BGEM fields with no `ImportedMaterial` sink and no ledger entry (measured in vanilla `Fallout4 - Materials.ba2` and `SeventySix - Materials.ba2`).
- **Location**: ledger at `byroredux/src/asset_provider/material/merge.rs:1473-1529`; fields in `crates/bgsm/src/base.rs`, `bgsm.rs`.
- **Status**: NEW (residue of FO4-2026-11-10-D2-04; #6848 closed with the comment claiming "the remaining parsed-but-unmerged groups")
- **Description**: Walking every `BaseMaterial` / `BgsmFile` / `BgemFile` pub field against the non-comment code of `material/{merge,provider,mod}.rs` and the rest of the binary leaves these fields with zero consumers and zero ledger mentions: `base.refraction` + `base.refraction_power` (BGSM and BGEM), `environment_mapping_window` / `environment_mapping_eye`, `hide_secret`, `displacement_texture_bias` / `displacement_texture_scale` and the `tessellation_pn_scale` / `tessellation_base_factor` / `tessellation_fade_distance` triple (only the `tessellate` flag is listed), and the FO76 `terrain` block (`terrain`, `terrain_unk_int1`, `terrain_threshold_falloff`, `terrain_tiling_distance`, `terrain_rotation_angle`). (`adaptive_emissive_final_exposure_min` is covered by the ledger's `adaptive_emissive_*` wording; `receive_shadows` is deliberately declined there; `distance_field_alpha_texture` is documented on the field, #2642.) `base.refraction` is the only renderer-relevant one: `material/mod.rs:251-262` explains why it must not mean glass but never says it has no sink.
- **Evidence**: probe `bgsmcount` (authored non-default counts): FO4 `refraction_power != 0`: 10 BGSM / 6 BGEM; `environment_mapping_window` 13, `environment_mapping_eye` 8, displacement texture slot 58 (that slot is bound at `merge.rs:930`); FO76 `base.refraction` 53 BGSM / 44 BGEM, `refraction_power != 0` 69 / 98, `terrain` 150 BGSM, displacement scale/bias and tessellation scalars non-zero on all 25,948 (parser default, not authored intent). Consumer greps: no `.refraction*`, `.terrain*`, `.tessellation_*`, `.displacement_texture_{bias,scale}`, `.environment_mapping_{window,eye}`, `.hide_secret` read anywhere in `byroredux/src` outside tests.
- **Impact**: Documentation only (no visual claim, as with #6848). The ledger's stated purpose is to let the next completeness sweep tell "not yet wired" from "overlooked"; refraction distortion (heat shimmer, force fields; FO76 69 + 98 files) is exactly such a candidate and is not recorded.
- **Related**: #6848, #2704, #4667, #2626 (refraction is not glass).
- **Suggested Fix**: Add the groups and counts above to the ledger comment (or `docs/engine/nifal.md`), and extend `bgsm_merge.rs`'s ledger pin, if it has one, so a new parser field cannot land unlisted.

#### PAR-D6-01: The loose walker's directory budget does not bound indexed entries, so a link fan-out multiplies keys and stat calls
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: a `Data\textures` (or `meshes`/`materials`/`music`) tree where a chain of directories each carry F files and link twice to the next (the `loose_tier_bounds_a_chain_of_doubled_links` shape, depth 18, F >> 1)
- **Location**: `byroredux/src/asset_provider/archive.rs:59-66` (`MAX_LOOSE_DIRS`), `:164-215` (`walk`)
- **Status**: NEW (#6483 closed; this is the residual of its own bound)
- **Description**: After #6483 each directory is walked once per distinct path to it, not once per canonical path, and only the count of directories (262,144) is capped. Each walked directory then inserts all its files under a distinct key and does two stats per entry (`is_dir`, `is_file`). Keys and work are therefore bounded by 262,144 x F, not by 262,144. The test pins the directory bound with one file per level, which hides the multiplier.
- **Evidence**: `state.walked >= state.budget` is the only work gate; `files.entry(child)` inserts per file with a path-qualified key (`textures\root\l0\l1\...\f.dds`, all distinct). At F = 100 the budget admits ~26 M entries (several GB of `String` + `PathBuf`) and ~50 M stat calls at boot, before the one warn line. A real mod-manager layout (one mod directory linked into a handful of places) is nowhere near this.
- **Impact**: boot stall or OOM on a hostile or accidentally cyclic-by-fan-out link tree; needs local filesystem control (a mod archive extracted with symlinks), so hardening only. When the budget does trip, the surviving subset is whichever branches `read_dir` yielded first, so the partial index differs run to run.
- **Related**: #6243, #6483
- **Suggested Fix**: also cap total indexed entries (or cap re-walks per canonical directory, e.g. 8) and warn once on either; sort `read_dir` entries so a truncated index is deterministic.

#### PAR-D6-02: `find_case_insensitive` refuses `..` but not a Windows drive-prefix or `:` component, so a script's `PlayBink` path can leave the data folder on Windows
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: `PlayBink "C:\somewhere\x.bik"` (or `D:x.bik`, or `x.bik:stream`) from a mod script, on a Windows build
- **Location**: `byroredux/src/systems/script_requests.rs:316-330`, called from `byroredux/src/video.rs:384-396`
- **Status**: NEW
- **Description**: The INC4-3 fix (ba3ee4307) splits on `\`/`/`, drops empty parts, and rejects `.`/`..`. It does not reject a component containing `:`. On Windows `PathBuf::join("C:")` replaces the accumulated path (std: a path with a prefix and no root replaces `self`), so the walk re-roots at the drive's current directory and the later components resolve relative to it, outside `Data`. Linux is unaffected (`:` is an ordinary name character), so no test can show it there. The project already treats this exact class as a hard rule for Steam `installdir` (`is_bare_install_dir` rejects `:` for the same reason, #5016).
- **Evidence**: `find_case_insensitive` only compares `component == ".." || component == "."`; `video.rs::resolve_under` tests cover `..\..\secret.bik` and `Mymod\..\..\secret.bik`, nothing with a prefix. Reasoned from std's documented `Path::join` behaviour; not run on Windows.
- **Impact**: a script can feed any `.bik`-shaped file under the engine's working directory on any drive to the Bink decoder (read only; the decoder rejects non-Bink bytes). It is the same threat INC4-3 closed, narrower and Windows-only.
- **Related**: INC4-3 (ba3ee4307), #5016
- **Suggested Fix**: reject any component whose `Path::new(component).components()` is not a single `Normal` (or simply one containing `:`), and add the Windows-shaped inputs (`C:\x`, `D:x`, `x:y`) to the `resolve_under` test, which is host-independent as a string predicate.

#### PAR-D6-03: The movie soundtrack's temp WAV is created non-exclusively under a predictable name
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: another local user pre-creates `<tmp>/byroredux-movie-<pid>-<n>.wav` as a symlink to a file the player can write
- **Location**: `crates/video/src/lib.rs:153-157` (`temp_wav_path`), `:256-277` (`write_wav`)
- **Status**: NEW. This is a sibling of the pattern #6307 fixed: the movie writer is new code that never got the fix, so it is not a regression of #6307 itself. The #6307 fix is (`crates/audio/src/fuz.rs::write_temp_xwma`, `create_new` + mode 0600 + pid/clock/counter name)
- **Description**: `write_wav` uses `std::fs::File::create(&wav)`, which follows a planted symlink and truncates its target. The name is `byroredux-movie-{pid}-{counter}.wav`, guessable. The decoded PCM (up to 691 MB) is written with default permissions. The `.fuz` writer was fixed for exactly this; the movie path, added later, did not reuse the fix.
- **Evidence**: no `OpenOptions`/`create_new` in `crates/video/src/lib.rs`; `Drop for Movie` only removes the file on a clean drop, so a crash leaves up to 691 MB in the temp dir.
- **Impact**: hardening on a shared-tmp multi-user host (kernel `protected_symlinks` blunts it on modern Linux); on a single-user desktop only the stale-file leak matters.
- **Related**: #6307
- **Suggested Fix**: share the `create_new` + retry + 0600 helper between `crates/audio/src/fuz.rs` and `crates/video` (or use `tempfile`), so there is one temp-file writer.

#### PAR-D6-04: A promotion read is not pinned to the winner the floor read came from, and the streamer's provider never re-validates its archives
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: (a) a BA2 DX10 record in the winning archive whose last chunk is corrupt but whose other chunks inflate (or the reverse); (b) an archive replaced on disk after streaming was first enabled
- **Location**: `byroredux/src/asset_provider/texture.rs:185-208` (`extract_for_registry`, `extract_texture_from`), `byroredux/src/asset_provider/archive.rs:500-525`, `byroredux/src/app_frame.rs:85-88`, `byroredux/src/texture_streamer.rs:131-141`
- **Status**: NEW
- **Description**: Each read is an independent `find_map` over `texture_archives.iter().rev()`, and `or_warn` turns any non-`NotFound` error into `None`, so a failed read in the winner falls through to the next archive. The floor read needs only the last chunk and the promotion read needs the chunks from the target level, so they can fail independently: the floor is served from winner W and the promotion from a lower-precedence L (or the reverse), giving one resident texture whose low mips come from one file and high mips from another (a mod override against vanilla). Separately, the streamer's `TextureProvider` is built once, on the first frame streaming is on (`streamer.is_none()`), from `Archive` handles taken then. Cell-load providers are rebuilt per transition and so re-check `ArchiveStamp` through `open_shared`; the streamer's handles do not. After an archive is replaced mid-session, floors come from the new file and promotions from the old fd and table, which is the #5663 same-length-rewrite hazard on the one holder that never re-opens.
- **Evidence**: `extract_texture_from` and `extract_for_registry` both end in `.or_else(|| self.extract(path))`; `or_warn` warns once per path per archive and then stays silent; no code compares the archive that served the floor with the one that serves the promotion. In the healthy case the order is identical, so this needs corruption or an on-disk change.
- **Impact**: visual (mixed-source mips) only on a damaged or replaced archive; bounds checks still fail rather than return garbage for a shorter replacement, but a same-length replacement is served from stale offsets.
- **Related**: #5663, #4658
- **Suggested Fix**: record which archive index served the floor in the registry's promotion request and have the promotion read only that archive (a failure then leaves the floor, with one warning); build the streamer's provider through the same `open_shared` revalidation a cell transition gets, or drop and rebuild it when a cell-load provider reports a non-reused archive.

#### PAR-D6-05: `mesh_cache_key` is a sixth "prepend `meshes/` unless present", outside the scan that guards the other five
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: any model path reaching `MeshRegistry` (e.g. Starfield `geometries\<hash>.mesh`)
- **Location**: `crates/renderer/src/mesh.rs:84-92`; scan scope `byroredux/src/workspace_hygiene_tests.rs:1262-1284`
- **Status**: NEW (hygiene; no wrong lookup demonstrated)
- **Description**: #6956 made `normalize_mesh_path` the one home of the prefix rule, including the #1292 `geometries\` exemption. `mesh_cache_key` re-implements it (lower-case, `\`->`/`, prefix `meshes/`) without the exemption, so a `geometries\x.mesh` path is keyed `meshes/geometries/x.mesh`. The renderer cannot call the binary's function, and the key is internal to the cache, so every producer of one input gets one key and nothing breaks today. The scan only walks `byroredux/src` and only matches the `"meshes\\{ident}"` spelling, so it could not have caught this copy and would not catch `format!("meshes\\{}", x)`.
- **Impact**: none now; a second producer that keys the same mesh from `canonical_mesh_key` output would split the cache entry for exactly the paths #1292 exempted.
- **Related**: #6956, #1292
- **Suggested Fix**: move the prefix rule (or just the `meshes`/`geometries` predicate) to a crate both can reach (`byroredux_bsa` already hosts `normalize_path`), call it from `mesh_cache_key`, and widen the scan to `crates/*/src` and the positional spelling.


## Checked and not filed

- **#6954 regression check.** Unifying the container did not change which FO4 `bhkSystemBinary` blobs nif accepts: 179,878 of 179,878 parse. On the nif side, a container error costs only that skeleton's ragdoll, with a warning naming the block.
- **BA2 `extract_dx10_mips`.** A `first_mip` past the chunk table returns the last chunk, as documented. An empty table gives a header-only DDS. A short final floor chunk is rejected by the exact-length `payload_first_level`. The renderer's `floor_first = Some(payload_first)` rules out the `level_byte_range_in` underflow.
- **Bink 2 and Bink Audio.** The bit readers, MV arithmetic and AVX2 `unsafe` blocks are clean: each load or store goes through a bounds-checked slice, and the i32 IDCT L1 gain is 57.2 < 70. Failed vs Played is signalled on every reader-thread error path.
- **Other consumers.** `.extract(..).ok()` has not reappeared in production code. The facegen exact-size precondition is unconditional. No CDB parked field leaks into a live slot, and the Additive and Multiply factors match the BGSM arm. Every partial result (`dropped_bones`, `reference_frame_dropped`, `additive`) warns with the clip path.
- **Bink 1 revisions.** Bink 1 `Movie::open` refuses revisions b/f/g/h/j/k/m and the alpha and grayscale flags today, through `mercs2_bink`. Only the test is missing (PAR-D3-04).
- **Size discipline and loose tier.** `lz4_flex` 0.14.0 keeps `safe-decode`, and the size ceilings live once in `crates/bsa/src/safety.rs`. `lod_family.rs` is pure. The #6956 `meshes\` scan reads production text, so it is not vacuous. Every guard test named in the skill exists, and only the real-data ones are `#[ignore]`d.

## Gate Matrix

| Crate | Size caps | Error policy | Version gate | Corpus sweep (strict lane, this run) | CI lane |
|---|---|---|---|---|---|
| bsa (BSA/BA2/CSG) | ✅ ceilings once in `safety.rs`; `capacity_hint` | ✅ named errors; ⚠ BA2 chunk EOF has no context (D2-07) | ✅ {103,104,105} / {1,2,3,7,8} | ✅ 34 green (FO3/FNV/SE/LE/FO4/FO76 full extract) | manual-only `parsers` job |
| bgsm | ✅ depth 16 + visited | ✅ `parse_*_diag` everywhere | ✅ per-field, over-ceiling diag | ✅ 3 green (FO4 exact, FO76 floors) | manual-only |
| sfmaterial (CDB) | ✅ `MAX_NESTING_DEPTH`, streaming index | ⚠ `stream_list` no trailing check (**D2-01**); errors name no instance (D2-08) | ✅ header errors, pinned vocabularies, `read_order` | ✅ 5 green (exact counts) | manual-only |
| hkx (+ nif view) | ✅ hkx tables and step budget; ⚠ nif `class_name_at` quadratic (D1-03) | ✅ labels; nif container errors cost the block | ✅ v8/v11 from the file; ⚠ np_ragdoll not 64-bit-gated (D3-01) | ✅ 10 green (FO4 + SE censuses, LE parity) | manual-only (LE parity `--skip` without `BYROREDUX_SKYRIMLE_DATA`) |
| facegen | ✅ morph cap, exact size | ⚠ consumer logs bad EGM at debug (D2-06) | ✅ magics | ✅ 5 green (exact) | manual-only |
| menuxml | ✅ include, byte, tile and texture budgets | ✅ | n/a (lenient scanner) | ✅ 14 green | manual-only |
| game-detect | ✅ `MAX_VDF_DEPTH` | ✅ malformed rejected | n/a | n/a (no data suite) | default lane only |
| bink2 | ✅ `MAX_FRAMES` / `MAX_PIXELS` / tracks / index | ⚠ over-read decodes Ok (D2-02) | ⚠ any `KB2?` accepted (D3-02); flags unread (D3-03) | via video sweep | default-lane fixture |
| binkaudio | ✅ block bound; ⚠ one packet can spend the whole hour (D1-02) | ✅ `BadSize` | ✅ 48 kHz stereo DCT else Unsupported | via video sweep | default-lane fixture |
| video (Bink 1 + glue) | ❌ **no Bink 1 area cap (D1-01)** | ⚠ open path outside `guard_decoder` (D2-04); silent Bink 1 audio end (D5-02); fps 0 accepted (D5-01) | ⚠ Bink 1 refusal untested (D3-04) | ✅ 1 green, 101 movies; ⚠ not strict, no count pin (D4-01) | manual-only (since #6993) |

The `real-data-gates.yml` `parsers` job is still **manual-only** (`workflow_dispatch`, runner not registered), so this run's strict lane is the only recent automated-equivalent result.

## Skill drift (for `/audit-sync`, not findings)

1. **Dims 1, 2, 5 and 6 `Paths:`** omit the Bink crates (`crates/bink2/src/`, `crates/binkaudio/src/`, `crates/video/src/`, `byroredux/src/video.rs`) and the nif container view `crates/nif/src/blocks/collision/havok_packfile.rs`. Dim 2 and Dim 6 also omit the M39 surface (`byroredux/src/texture_streamer.rs`, `extract_dx10_mips` / `dx10_layout`, the floor-read functions) and `byroredux/src/systems/script_requests.rs` (`find_case_insensitive`). Dim 6 omits `crates/renderer/src/mesh.rs` (`mesh_cache_key`). Dim 5 has no Bink decode-to-consumer bullet.
2. **Dim 1:**
   - The phrase "`probe` clamps the Bink 1 header to the same two ceilings" implies Bink 1 is bounded like Bink 2, and it is not for picture size (D1-01).
   - The section-count bound now lives in `crates/hkx/src/container.rs`.
   - Name `class_name_at` beside the borrowed-class-name rule.
3. **Dim 3:**
   - "nif is as strict as hkx" holds for the container, not for `decode_np_ragdoll` (D3-01).
   - The `crates/video/src/` row should say the g/j gate belongs to video alone; `Bk2::open` accepts any `KB2?`.
   - The `ba2.rs` comment cites `archive/open.rs:40-48`, which is now `:34-40`.
4. **Dim 4:** The Bink bullet says the movie sweep is "absent from `real-data-gates.yml`". That is stale: #6993 (64f8d41fd) added `video` to the parsers loop. `material_index_resolves_inherited_texture_slots` is an `#[ignore]`d real-data test, not a default-lane pin.
5. **Dim 2:** "a first audio track that cannot be decoded plays the picture silent" no longer holds: `bink2_audio` picks the track with ID 1, else the first (#6896), and its warning text still says "first". The guard bullet should say `guard_decoder` stops at the reader thread (D2-04). `container.rs` has no test module of its own.
6. **Dim 6:** The skill never mentions the #6483 semantics: the ancestor-stack cut, `MAX_LOOSE_DIRS` 262,144, and the three `loose_tier_*` tests. The sound provider's music-root loose tier is unlisted.
7. **`_audit-owners.md`** has no row for `crates/nif/src/blocks/collision/havok_packfile.rs`.
8. **Code and doc comments (not skill text):**
   - `extract_dx10_mips` says chunk order is "validated at open", but open only warns.
   - `VideoError::Unsupported` and the `video.rs` module and `VideoOutcome::Skipped` docs predate Bink 2 decoding. They claim alpha/grayscale refusal for both codecs, which only Bink 1 does.
   - `docs/engine/video-playback.md` §2 does not say the track-ID rule is Bink 2 only.
   - `docs/engine/archives.md` still says the loose tier walks each symlinked directory "once by canonical path" and lists four roots; there are five now, with `music` added.

## Note for the owner (not a finding)

Commit f2d209b1f (#6896-#6898) records the Bink track-ID mapping (`ENGLISH_TRACK_ID` in `video.rs`) as read from `Fallout4.exe`, `Oblivion.exe` and `Fallout3.exe` by disassembly. `docs/clean-room-decoders.md` allows only the Bink DLL as a black box, and the standing "no exe-decode shortcuts" preference rules out disassembly as a source for rules. Whether to keep the mapping, re-source it, or relabel it as a declared gap is the owner's decision. This audit does not file it.

## Next step

`/audit-publish docs/audits/AUDIT_PARSERS_2026-11-20.md`. Suggested labels:
- **Bink** (D1-01, D1-02, D2-02, D2-04, D3-02, D3-03, D3-04, D5-01, D5-02, D6-02, D6-03): `import-pipeline`. There is no `video` label, so flag that in the publish summary.
- **D1-03, D3-01:** `nif`.
- **D2-01, D2-08:** `nifal` + `game:starfield`.
- **D2-05, D6-04:** `renderer` + `memory`.
- **D2-06:** `import-pipeline`.
- **D2-07:** `import-pipeline`.
- **D4-01:** `test-gap`.
- **D5-03:** `nifal` + `doc-rot` + `game:fo4`.
- **D6-01, D6-05:** `tech-debt`.
