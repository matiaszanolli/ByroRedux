# D4-02: D4-02: AfflictionTable tie-break asymmetry between band_for (last-on-tie) and band_by_key (first-match) on duplicate thresholds

- **Labels**: low,character,bug
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4456

---

**Source**: The mechanism's own contract — affliction.rs:68-72 "Band order is not significant".

**Description**

`band_for` (`crates/core/src/character/affliction.rs:97-104`) selects the reached band via `max_by(total_cmp)`, which on a tie returns the LAST band with the maximal `min_pool`; `band_by_key` (:114-116) uses `find`, which returns the FIRST. If a table ever carries two bands with the same `min_pool` (an authoring error), `reevaluate_affliction` classifies via `band_for` but applies/reverses penalties via `band_by_key`, so the penalties applied can come from a different band than the classification selected.

**Evidence**

`Iterator::max_by` documented semantics (last element on ties) vs `Iterator::find` (first match); both apply and reverse paths in `reevaluate_affliction` (affliction.rs:190-207) route through `band_by_key`, so the diff-and-reapply remains self-consistent — penalties cannot compound or leak from this alone.

**Impact**

Effectively nil today: no shipped `AfflictionTable` exists (thresholds PENDING), and duplicate thresholds are malformed authored data. If real tables land with a duplicated cut point, the higher-listed band's penalties silently win — a wrong-penalty-set bug no current test can catch.

**Related**

#4103 (the min_pool keying this builds on).

**Suggested Fix**

Make the two agree — either dedupe/validate at table construction (reject duplicate `min_pool` with a logged warning) or give `band_for` find-compatible tie semantics. Add a one-line test pinning behavior on a duplicate-threshold table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D4-02, /audit-character 2026-09-19, HEAD `479163836`).*