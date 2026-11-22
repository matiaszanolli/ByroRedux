# #7123: PAR-D1-2026-11-20-01: Bink 1 pictures are bounded per side by a dependency but never by area in the engine; a 32767x32767 header reserves ~4.4 GiB before any frame decodes

**Labels**: high,import-pipeline,safety,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7123

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D1-01 (`/audit-parsers`, HEAD f3713bf5e)

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
