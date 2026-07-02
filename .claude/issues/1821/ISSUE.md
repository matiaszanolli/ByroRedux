# SPT-NEW-06: format-notes.md's 14000-band tail-tags worked example doesn't byte-align with where the live walker bails

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1821
**Source report**: docs/audits/AUDIT_SPEEDTREE_2026-07-02.md
**Labels**: low, import-pipeline, documentation

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting (doc-precision)
- **Location**: `crates/spt/docs/format-notes.md:588-609` ("Open: 14000-band tail tags in the 4 outliers")
- **Status**: NEW (raised 2026-07-01, never filed; re-verified in `AUDIT_SPEEDTREE_2026-07-02.md` — `format-notes.md` unchanged, live corpus bail offsets confirmed 4507/5641/5946/6211 at tag `768`)

**Description**: The doc's worked example attributes the 4 Oblivion outliers' second bail to tag `14007` (out of `TAG_MAX = 13999`) at an eyeballed hex offset, dismissing the walker's actually-recorded value `768` as a mis-decode. But the walker's deterministic byte accounting genuinely lands the next tag read on the `00 03 00 00` (= 768) u32 — the prior entry `tag=13013` (`FixedBytes(7)`) consumes exactly 4 + 7 = 11 bytes, placing the cursor at the offset that reads `768`, not 3 bytes earlier where `14007` sits. `SptScene::unknown_tags == [(768, …)]` in the live corpus run matches the walker's real cursor, not the doc's hex table.

**Evidence**: The live corpus run (2026-07-02 audit) reports `tag=768 (0x0300)` at offsets 4507 / 5641 / 5946 / 6211 for the 4 outliers — exactly the walker's own byte math, which contradicts the doc's `14007`-at-a-different-offset narrative. `format-notes.md` is unchanged since the 2026-07-01 report that first raised this.

**Impact**: Documentation-only; parser behaviour is correct and the acceptance gate is unaffected. Forward-looking risk: the doc recommends a "re-run with TAG_MAX = 16000, extend `dispatch_tag`" follow-up anchored on this worked example. A contributor adding a `14007` arm from the doc's byte table would find the walker never reads `14007` there (it reads `768`), silently wasting the dictionary-expansion attempt until someone re-derives the alignment.

**Related**: SPT-D1-01 / #999 (the 13005 fix this doc section chronicles the follow-up to; the 13005 disambiguation itself is confirmed correct).

**Suggested Fix**: Re-run `spt_recon`/`spt_dissect` starting from the walker's actual `tail_offset` (not a manual hex-dump position) and correct the worked example's offset/value pairing before anyone acts on the "extend TAG_MAX" follow-up.

## Completeness Checks
- [ ] **SIBLING**: Check the rest of `format-notes.md`'s worked-example byte tables for the same eyeballed-vs-actual-cursor drift
- [ ] **TESTS**: The corrected offset/value pairing quoted in the doc is reproduced from a live `--ignored` corpus run before committing the doc edit
