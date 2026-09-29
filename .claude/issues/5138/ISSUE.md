# #5138: SPT-2026-09-29-D1-01: #4122 left the spt crate's docs asserting the desync it fixed — `SptScene::tail_offset` still says 46 % of files stop mid-payload, `tag.rs` says 13013 is 7 bytes, 12002/12003 still "unevidenced"

**Labels**: low, documentation, doc-rot, speedtree

**Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-09-29.md` (report ID `SPT-D1-01`)
**Severity**: LOW
**Dimension**: Walker Byte-Accounting (also Tag Dictionary)

## Location
- `crates/spt/src/scene.rs` (`SptScene::tail_offset` rustdoc)
- `crates/spt/src/tag.rs` (`SptTagKind::FixedBytes` doc; 12002/12003 arms)
- `crates/spt/docs/format-notes.md` (12002/12003 caveat)
- `crates/spt/examples/spt_tail.rs`
- `crates/spt/src/stream.rs` (module doc)

## Description
`9fcbee478` fixed the three mis-sized entries and updated the dispatch arms, but several claims were not updated:
- The public rustdoc on `SptScene::tail_offset` still lists "In 46 % of files the resync needs a 1-3 byte shift, meaning the walker stopped *inside* a payload it mis-sized" and closes with "Treat it as 'where parsing gave up'". The gate #4122 added now measures 0/159 shifted and 159/159 stopping on a 14 000-band tail tag, so `tail_offset` is the true TLV boundary at `TAG_MAX`.
- The `SptTagKind::FixedBytes` doc gives "tag `13013` = 7 bytes"; the value is 4.
- The 12002 and 12003 arms still say "Size only … no recorded corpus evidence"; `format-notes.md` repeats it, while the same file's 2026-09-24 entry records that both "decode cleanly to their next tags".
- `spt_tail.rs` describes the 46 % stop in the present tense.
- Separately (predating the baseline), the `stream.rs` module doc says errors surface as `Err(SptParseError::Truncated)`. No such type exists; the parser returns `io::Error`.

## Evidence
Corpus run this audit: `[SI] 159 files | 159 on boundary | 159 shift-0`. `grep -rn SptParseError crates/spt` has exactly one hit, the doc line.

## Impact
A consumer reading the public API doc would treat `tail_offset` as untrustworthy, although it is now the exact precondition point #3808 named for raising `TAG_MAX`. The false 7-byte and "unevidenced" claims invite a re-litigation like the 2026-07-04 "768" dispute.

## Related
#4122, #3535 (12002/12003 evidence), #4120 (prior "sweep missed a file" doc-rot pattern).

## Suggested Fix
- Rewrite the `tail_offset` bullets to state the post-#4122 measurement: the stop is the true boundary, and tail tags start at 14 000.
- Change the `FixedBytes` example to 4 bytes.
- Replace the 12002/12003 caveats in `tag.rs` and `format-notes.md` with a pointer to the 2026-09-24 side confirmation.
- Mark the `spt_tail.rs` sentence as pre-#4122.
- Replace `SptParseError::Truncated` with `io::Error` (`UnexpectedEof`).

Validated at HEAD 9fcfdc3fc: `scene.rs` still carries the "46 %" and "where parsing gave up" lines; `tag.rs` doc still says `13013` = 7 bytes while dispatch is `FixedBytes(4)`; `SptParseError` appears only in the `stream.rs` doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other docs in crates/spt referencing pre-#4122 behaviour)
