# #3779: SF-D1-01: ba2.rs module doc cites a 108-vanilla-archive corpus that matches no installed game (Starfield 50, FO4 187, FO76 101)

**Labels**: documentation, import-pipeline, low, legacy-compat, game:starfield, doc-rot
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-30.md` — SF-D1-01 (LOW)
**Dimension**: 1 — BA2 v2/v3 LZ4 block decompression
**Location**: `crates/bsa/src/ba2.rs:14` (module doc) and `:220-222`

## Description

Both passages say the v2/v3 layout claim was *"observed across 108 vanilla archives"*. No installed corpus matches 108. Measured this run:

| Game | Archives |
|---|---:|
| Starfield | 129 total / 50 vanilla |
| FO76 | 101 |
| FO4 | 187 |

The number is unsourced against anything present on disk.

## Evidence

The substantive claim the count supports — *"no v3 GNRL observed"* — is independently **true**: 0 v3 GNRL archives across all 129 Starfield archives. So the conclusion holds; only the cited corpus size is wrong.

## Impact

Doc rot, not a correctness defect. It matters because the number is the stated evidence base for a format assumption in the reader: a future maintainer re-deriving it cannot reproduce 108 from any installed corpus, and cannot tell whether the assumption was measured over a corpus that no longer exists or was never measured at all.

## Suggested Fix

Replace "108 vanilla archives" at both sites with the corpus actually measurable today, stated per game (Starfield 50 vanilla of 129 installed; FO4 187; FO76 101), and pin the "0 v3 GNRL" result beside it with the date, the way the other measured pins in this crate are recorded.

## Completeness Checks
- [ ] **SIBLING**: Same "observed across N archives" phrasing checked elsewhere in `crates/bsa/`
- [ ] **TESTS**: No behaviour change; if the v3-GNRL assumption is turned into an assertion, a test pins it
