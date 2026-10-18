# SCR-D3-2026-10-17-08: Marker-contract docs and pins drifted (cleanup table, catalog doc, stale count comment)

Issue: #5996
Labels: low, scripting, documentation, doc-rot

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-08: Marker-contract docs and pins drifted (cleanup table, catalog doc, stale count comment)

- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `cleanup.rs:36-46,163-232`; `condition/catalog.rs:78`; `condition/tests.rs:1183`; `condition/catalog.rs:19-21`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: (a) The Pattern-B table and `every_documented_pattern_b_marker_drains_in_its_named_consumer` omit `OnDeathEvent` (drained at
  `reference.rs:588`) and do not row-check the `HavokImpulseRequest`/`MotionTypeChangeRequest` drains the table does list; the pin also scans whole
  files via `include_str!` instead of `production_text`. (b) `catalog.rs:78` says "FO3 / FNV author no CTDA at 71"; the masters carry 2 450 (FO3) and
  3 535 (FNV) fn-71 uses (71 is `GetInFaction` on TES4/FO3/FNV too). (c) `tests.rs:1183` "24 catalog entries" (live: 48 + HasPerk's second index);
  `catalog.rs:19-21` still says new functions land as "a variant + a match arm in evaluate_function" (now a ROWS row + arm).
- **Impact**: comment/pin rot only.
- **Suggested Fix**: add the three rows to the table and the pin (scan `production_text`), correct the 71 sentence, refresh the two comments.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
