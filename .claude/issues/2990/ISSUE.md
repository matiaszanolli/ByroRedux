# ESM-2026-08-16-D4-01: object_mod_loose_items is absent from EsmIndex::categories() with no recorded rationale

**Issue**: #2990
**Severity**: LOW
**Dimension**: 4 — Index Completeness
**Labels**: `low,import-pipeline,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_ESM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ESM_2026-08-16.md` (Dimension 4 — Index Completeness).

**Record / Sub-record**: `OMOD`
**Location**: `crates/plugin/src/esm/records/index.rs`:43-47, :445-472

## Description

`object_mod_loose_items` (added 2026-08-16, 2,409 entries on `Fallout4.esm`) is **absent from `EsmIndex::categories()`** — the table whose own docstring says it exists to keep `total()` and the end-of-parse census line in lockstep.

No exclusion rationale is recorded on either the field or `categories()`, so the next reader cannot tell *"deliberately not a record count"* from *"forgotten"* — the exact ambiguity that let #2907 (ESM-D4-01, 41 maps missing from `merge_from`) survive so long.

The field **was** correctly added to `merge_from` (:767-768), which is what makes the `categories()` omission read as an oversight rather than a decision.

## Evidence

```
$ grep -n "object_mod_loose_items" crates/plugin/src/esm/records/index.rs
47:    pub object_mod_loose_items: HashMap<u32, u32>,
388:        for &loose_item in self.object_mod_loose_items.values() {
767:        self.object_mod_loose_items
768:            .extend(other.object_mod_loose_items);
```

Re-verified 2026-08-17: `sed -n '445,472p' … | grep -c object_mod_loose_items` → **0**. Present in `merge_from`, absent from `categories()`.

## Impact

`total()` and the end-of-parse census under-report by the size of this map. Low direct cost — but the *reason* this matters is precedent: #2907 was a 41-map version of exactly this, and it survived because nobody could tell omission from intent.

## Suggested Fix

Either add the field to `categories()`, or record a one-line rationale on the field explaining why it is not a record count (it maps OMOD → loose item, so "not a record category" may well be correct — the point is that the decision must be legible).

## Related

- #2907 (ESM-D4-01 — the 41-map instance of this same ambiguity)
- ESM-2026-08-16-D4-02 (#2990) — sibling `index.rs` finding from the same dimension

## Completeness Checks
- [ ] **SIBLING**: Every `EsmIndex` field diffed against both `categories()` and `merge_from`, not just this one
- [ ] **LEGIBLE-INTENT**: Whichever way it goes, the decision is recorded so the next reader cannot mistake it for an oversight
- [ ] **PARITY-GUARD**: Consider a test asserting `categories()` covers every countable field, so the next addition cannot silently drift
- [ ] **TESTS**: A regression test pins this specific fix

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2990 --json state` when live state is needed.*
