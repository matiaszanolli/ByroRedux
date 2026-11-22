# #7136: PAR-D3-2026-11-20-04: Bink 1 `Movie::open` revision / alpha / grayscale refusals are unpinned

**Labels**: low,import-pipeline,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7136

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D3-04 (`/audit-parsers`, HEAD f3713bf5e)

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
