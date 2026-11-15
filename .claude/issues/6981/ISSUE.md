# #6981: TD7-2026-11-15-02: the Bink container ceilings are typed twice and linked only by a doc comment; the Bink 1 header offsets are bare

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6981
- **Labels**: low,import-pipeline,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 7 (magic numbers)
- **Location**: `crates/video/src/lib.rs:133` (`word(0x28).min(0x100)`, `word(0x10).min(1_000_000)`, `0x2c + 12 * … + 4 * …`); `crates/bink2/src/container.rs:60-72` (`MAX_AUDIO_TRACKS = 0x100`, `MAX_FRAMES = 1_000_000`)
- **Age**: `e13628ab4` 2026-11-12 (video probe); bink2 constants earlier
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW
- **Description**: `bink2/container.rs:62-63` says "the track and frame limits are the ones `byroredux-video`'s Bink 1 probe clamps to", so a doc comment is the only link between two numeric copies. The Bink 2 copies are `pub const` but `mod container` is private (`bink2/src/lib.rs:14`; only `AudioPacket`, `AudioTrack` and `Bk2` are re-exported), so `video` cannot import them. Beside them, the Bink 1 header layout is bare in the same expression: `0x28` (track count), `0x10` (frame count), `0x2c` (header end), `12` (track record size), `4` (offset entry size), and a `[0u8; 0x2c]` buffer.
- **Evidence**: `let table_end = 0x2c + 12 * word(0x28).min(0x100) + 4 * (word(0x10).min(1_000_000) + 1);`
- **Impact**: This bounds an allocation from an untrusted header (`bytes.resize(table_end, 0)`, at most about 4 MB). If one copy is tightened (BIK-AUD-3 already moved bink2's limits once) and the other is not, the Bink 1 probe (which clamps) and the Bink 2 container (which refuses) stop agreeing about what an over-sized header is. This is bounded and cannot overflow today.
- **Related**: BIK-AUD-1, BIK-AUD-3 (cited in `video/src/lib.rs` and `bink2/src/container.rs`); `docs/clean-room-decoders.md`.
- **Suggested Fix**: re-export `MAX_AUDIO_TRACKS` and `MAX_FRAMES` from `byroredux-bink2` (`pub use container::{…, MAX_AUDIO_TRACKS, MAX_FRAMES}`), use them in `probe`, and name the five Bink 1 header offsets as `const BIK1_*` next to `probe`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
