**Severity**: LOW · **Dimension**: CDB Material Correctness
**Location**: `byroredux/src/asset_provider.rs:504`
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D3-03)

## Description
`build_material_provider` extracts only the hardcoded `materials\materialsbeta.cdb`. Each DLC/Creation ships its own CDB at a namespaced path (`materials\creations\shatteredspace\materialsbeta.cdb`, `…\sfbgs003\…`, `…\sfbgs00d\…`) inside its `* - Main.ba2` (passed via `--bsa`, not `--materials-ba2`). Neither the path nor the archive class is reached.

## Evidence
`a.extract("materials\\materialsbeta.cdb")` (`asset_provider.rs:504`) is the only extraction call (confirmed; the other `materialsbeta.cdb` hits at `:495/:668/:716/:1001` are comments/doc). Archive enumeration this audit: `ShatteredSpace - Main01.ba2`, `SFBGS003 - Main.ba2`, `SFBGS00D - Main.ba2` each contain one `.cdb` under `materials\creations\<plugin>\materialsbeta.cdb`.

## Impact
None observable today (Phase 1 only flips a global boolean; once the base CDB loads, `has_starfield_cdb()` returns true for DLC `.mat` meshes too). The moment SF-D3-01's Phase-2 lookup lands, DLC materials will be absent from the index and silently fall back to keyword-guessed values — a regression that would hide inside the Phase-2 change.

## Related
SF-D3-01 (#1289 Phase 2 — implement this in the same change).

## Suggested Fix
When implementing Phase 2, scan every loaded archive (`--materials-ba2` and `--bsa`) for `materials\**\materialsbeta.cdb` and merge in load order rather than extracting one fixed path.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: DLC CDBs merge into the same `material_path → {…}` index that feeds the single `.mat`→`Material` lookup; no second per-game material path
- [ ] **TESTS**: A test asserts a DLC-namespaced `materials\creations\<plugin>\materialsbeta.cdb` path is discovered and merged
