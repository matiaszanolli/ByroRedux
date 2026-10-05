# #5361 — SPT-2026-10-05-D1-01: The #5138 doc sweep left two contradictions — parse_spt says all five fatal errors are InvalidData (underflow is UnexpectedEof), and the boundary gate's docstring names two #4122 culprits instead of three

- **Labels**: low,speedtree,doc-rot,documentation
- **Filed from**: `docs/audits/AUDIT_SPEEDTREE_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5361

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting (also Tag Dictionary)
- **Location**: `crates/spt/src/parser.rs:82-90`; `crates/spt/src/stream.rs:8-11`, `:52-63`; `crates/spt/tests/parse_real_spt.rs:263-266`
- **Status**: NEW (residue of #5138's sweep; the `parse_spt` sentence dates to `57fdcc577`, 2026-08-31)
- **Description**: There are two stale claims:
  - **Error kind.** `parse_spt`'s rustdoc reads "Returns `Err(io::Error)` (`InvalidData`) on five fatal conditions: magic-header mismatch, stream underflow during a partially-read payload, …". The underflow condition comes from `SptStream::read_bytes`, which returns `io::ErrorKind::UnexpectedEof` (`stream.rs:54-55`). `567d7e064` corrected `stream.rs`'s module doc to say exactly that, so the crate's two docs now disagree about the same error.
  - **Culprit count.** The `walker_stops_on_true_tlv_boundary` docstring says "The culprits were two dictionary entries — `10002` (stride 1, now 32) and `13013` (7 bytes, now 4)". The gate itself found a third, `10003` (stride 8 → 32). `format-notes.md` records it as "Culprit 3 (found by the stronger gate)", and `tag.rs` and the SKILL list all three.
- **Evidence**: `grep -rn ErrorKind` finds no consumer that branches on the kind; the only kind assertion is the magic-mismatch test at `parser.rs:377`. Both claims are therefore documentation-only.
- **Impact**: Doc rot only. A caller that matches `InvalidData` to detect "corrupt `.spt`" would miss truncated files. The two-culprit sentence hides the entry that only the stop-word check, not the shift, could catch. That is the property a future `TAG_MAX` raise depends on.
- **Related**: #5138, #4122, #3752 (the five fatal conditions).
- **Suggested Fix**: State the kinds separately: magic, string cap, array cap and context-sensitive kind are `InvalidData`, and underflow is `UnexpectedEof`. Change "two dictionary entries" to three and add `10003` (stride 8 → 32).


Report ID in `AUDIT_SPEEDTREE_2026-10-05.md`: **SPT-D1-01** (filed under a dated ID because the bare ID collides with closed #999).

_Source: `AUDIT_SPEEDTREE_2026-10-05.md` (SPT-D1-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
