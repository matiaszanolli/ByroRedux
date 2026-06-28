# FNV-D4-NEW-01: index.trees omitted from EsmIndex::categories()

**Severity**: LOW · **Source**: `docs/audits/AUDIT_FNV_2026-06-27.md` (FNV-D4-NEW-01)
**Location**: `crates/plugin/src/esm/records/index.rs` (`EsmIndex::categories()`); map populated at `crates/plugin/src/esm/records/mod.rs` (`index.trees.insert`)
**Status**: NEW

## Description
`categories()` (`index.rs:379`) is the documented single source of truth for `total()` and `category_breakdown()` (the #634/#817 invariant). 85 of the 86 typed maps are listed — including the FO4-architecture `EsmCellIndex` maps added by #817 *specifically* to stop a category-wipe passing CI silently. The `trees` (TREE) map is the lone populated map missing from the table: `TREE` is dispatched into `index.trees` (`mod.rs:349`, `parse_tree`) but no `("trees", |s| s.trees.len())` row exists, so those records never count toward `total()` and never print in the breakdown.

## Evidence
`grep '"trees"' index.rs` → no hit in `categories()` (only `self.trees.extend` at the merge site `:677`). A live FNV parse prints `trees=3` separately from `[FNV] total=77825` (the 3 are excluded). Byte-scan: FNV TREE=3, but FO3/Oblivion ship many more (SpeedTree-heavy masters), so the blind spot widens there.

## Impact
Cosmetic on the count itself (3 records on FNV). The real impact is the *guard gap*: a regression emptying `index.trees` (a TREE dispatch-arm break, or a `parse_tree` panic-to-empty) would not move `total()` below the `parse_rate_*` floors and would not surface in `category_breakdown()` — exactly the failure mode #817 added the `EsmCellIndex` rows to prevent. SpeedTree placement (REFRs → TREE bases) on FO3/Oblivion could silently stop resolving.

## Related
#634 (FNV-D2-06, closed — established the invariant), #817 (the `EsmCellIndex`-rows precedent), SpeedTree S1 (`crates/spt`).

## Suggested Fix
Add `("trees", |s| s.trees.len()),` to the `categories()` array. Optionally add a test asserting `categories().len()` tracks the typed-map count so the next added map can't slip the same way.

## Completeness Checks
- [ ] **SIBLING**: Scan the rest of the `EsmIndex` typed maps for any other populated-but-uncategorised field beyond `trees`
- [ ] **TESTS**: A test asserting `categories()` covers every typed map (count parity) so a future map omission fails CI
