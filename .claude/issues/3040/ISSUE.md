# FNV-2026-08-16-D4-01: the recipes counter tracks an empty group; FNV's real recipes are RCPE

**Issue**: #3040
**Severity**: LOW
**Dimension**: 4 — ESM index completeness
**Labels**: `low,import-pipeline,documentation`
**Source report**: `docs/audits/AUDIT_FNV_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 4 — ESM index completeness).

**Location**: `crates/plugin/src/esm/records/index.rs`:240-241 and :318-320

## Description

Two inverted documentation claims:

- `:240` documents `recipes: HashMap<u32, CobjRecord>` as *"`COBJ` constructible-object records — **FNV crafting recipes**"*
- `:319` documents `RCPE` as *"recipe — **superseded by COBJ**; FNV ships both"*

**Both are backwards.** `FalloutNV.esm` contains exactly one `COBJ` occurrence, at offset 13,130,513, and it is a `GRUP` header of size `0x18` = 24 bytes — an **empty group with zero records**. That is why the baseline reports `recipes=0`.

FNV's actual crafting data is `RCPE`, which is routed only into the `MinimalEsmRecord` (EDID + FULL) stub bucket.

## Evidence

Byte scan of `FalloutNV.esm` (245,650,747 B):
```
COBJ ×1   (empty GRUP header)
RCPE ×106   RCCT ×11   RCOD ×144   RCIL ×374   RCQY ×407
```
Test output at HEAD: `recipes=0`.

## Impact

No runtime cost today — no crafting subsystem consumes either field. But the `recipes=0` line in the ROADMAP-tracked baseline reads as *"FNV authors no recipes"*, and the inverted doc claim has **already propagated verbatim** into `docs/audits/AUDIT_FNV_2026-05-03_DIM2.md`:202.

A future crafting milestone that trusts the comment would build against the empty record type.

## Suggested Fix

Correct both comments: `RCPE` is FNV's live recipe record; `COBJ` is the FO4+ successor and is empty on FNV. Either point the `recipes` counter at `RCPE` or rename it so the zero is not read as an absence of authored data.

## Related

- #2990 (ESM-D4-01 — the same `index.rs` legibility class)
- `docs/audits/AUDIT_FNV_2026-05-03_DIM2.md`:202 (already carries the inverted claim)

## Completeness Checks
- [ ] **BOTH-COMMENTS**: `:240` and `:319` both corrected — they are inverses of each other
- [ ] **COUNTER-MEANING**: `recipes=0` either becomes non-zero or is renamed so it cannot read as "none authored"
- [ ] **PROPAGATION**: The prior audit report carrying the inverted claim is annotated
- [ ] **TESTS**: A regression test asserts the `RCPE` count on real FNV data

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3040 --json state` when live state is needed.*
