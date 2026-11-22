# #7134: PAR-D3-2026-11-20-02: bink2 decodes any `KB2?` revision; the g/j gate lives only in `byroredux-video`

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7134

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D3-02 (`/audit-parsers`, HEAD f3713bf5e)

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
