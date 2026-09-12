# SPT-2026-09-11-D1-02: the #3808-measured 46%-of-corpus tail_offset desync remains unfixed and untracked by any open issue

URL: https://github.com/matiaszanolli/ByroRedux/issues/4122
Labels: bug, medium, terrain-exterior, speedtree

- **Severity**: MEDIUM
- **Dimension**: Walker Byte-Accounting (audit-speedtree)
- **Location**: `crates/spt/src/parser.rs:35-38` (`TAG_MIN`/`TAG_MAX`), `:66-76` (tail-detection loop); `crates/spt/docs/format-notes.md:763-778` ("`tail_offset` is a desync point, not a section boundary")
- **Status**: NEW (the underlying measurement is not new — it is `#3808`'s own finding, recorded in `format-notes.md` on 2026-09-07 and repeated in the `/audit-speedtree` skill's own text as an explicit "do not report as closed" open item — but no GitHub issue tracks the fix, and `#3808`, the issue that produced the measurement, is itself CLOSED)

**Description**

`#3808`'s corpus dissection measured, for each of 159 `.spt` files across FNV/FO3/Oblivion+Shivering Isles, the byte shift from `tail_offset` that maximises known-tag hits against the existing dictionary: 86 files need 0, 36 need 1, 33 need 2, 4 need 3. That means in 73 of 159 files (46%) the walker's stopping point is not a real boundary — it stopped *inside* a payload it had mis-sized, and one or more tags immediately before `tail_offset` consumed the wrong number of bytes. This is squarely Dimension 1's own core risk ("one mis-sized payload desyncs the whole stream"), now confirmed to actually be happening on nearly half the corpus, not hypothetical. `#3808` — the issue this measurement is filed under — is CLOSED, with its own body's "Status" line describing it as a completed research spike; no separate follow-up issue exists for the fix itself.

**Evidence**

`format-notes.md:763-778`:
```
| shift | files |
|---:|---:|
| 0 | 86 |
| 1 | 36 |
| 2 | 33 |
| 3 | 4 |
```

**Impact**

Bounded today for the identical reason the `#3808` entry itself gives: nothing consumes bytes past `tail_offset`, so a walker that stops 1-3 bytes into the wrong place is invisible to every current consumer (the placeholder importer, the acceptance-gate harness, the tag dictionary). The impact is entirely on the next piece of work the `/audit-speedtree` skill flags as "routine" — raising `TAG_MAX` and dictionarying the 14000-22000 tag bands `#3808` found recurring in ~151/159 files. That work cannot safely start from the current `tail_offset` on 46% of the corpus: extending the walker past a stop it doesn't know is wrong would silently misparse the newly-dictionaried tags in nearly half of all vanilla content.

**Related**

#3808 (closed — the measurement, not the fix); companion finding SPT-2026-09-11-D1-01 (the stale "geometry tail" prose describing the same field, filed separately).

**Suggested Fix**

Scoped to: (1) identify which dictionary entry near each file's `tail_offset` is mis-sized — the recon tooling (`spt_tail` example) already computes the resync shift per file, so pairing that with the *last* decoded tag before `tail_offset` should localise the culprit tag(s); (2) fix the size in `tag.rs`'s dictionary; (3) re-run the acceptance gate to confirm the 0-shift count rises from 86/159 toward 159/159 before attempting to raise `TAG_MAX`.

## Completeness Checks
- [ ] **SIBLING**: The `spt_tail` recon example's per-file shift computation is reused, not reimplemented, when localising the mis-sized tag(s)
- [ ] **TESTS**: The acceptance gate (0-shift file count) is re-run and its new count pinned by a regression assertion

Source: `docs/audits/AUDIT_SPEEDTREE_2026-09-11.md`
