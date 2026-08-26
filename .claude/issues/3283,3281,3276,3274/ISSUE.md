# Issues 3283, 3281, 3276, 3274 — doc-rot batch fix

All four are documentation-only fixes flagged by the 2026-08-24 audit sweep.

## #3283 — audit-scripting/SKILL.md stale test name
- File: `.claude/commands/audit-scripting/SKILL.md:871`
- `declines_on_control_flow` → `declines_unmodeled_conditional_guard` (renamed in cee35507)
- Line 712 already has the correct name; only 871 is stale.

## #3281 — per-game-translation-survey.md 3-month stale
- File: `docs/engine/per-game-translation-survey.md`
- §2 worked example describes classify_pbr_keyword's pre-#1873-fix behavior (now fixed, collapses-to-matte bug gone)
- §4.3 RACE DATA bullet says "no Skyrim arm exists" — false, Skyrim arm exists at crates/plugin/src/esm/records/actor/mod.rs:1219
- Bump Status: date, fix both passages

## #3276 — SpeedTreeWind docstrings describe deleted CNAM wind model
- `crates/spt/src/import/mod.rs:70-77` (SptImportParams::wind)
- `byroredux/src/cell_loader/nif_import_registry.rs:156-157` (CachedNifImport::speedtree_wind)
- Both still claim wind is derived from TREE.CNAM's first two finite values; actual code (references/import.rs:332) is a hardcoded `(1.0, 0.0)` constant since #3190 fix (4e1afcbe)

## #3274 — REGN ambient music shipped but marked unbuilt in 3 docs
- `crates/audio/src/lib.rs:129-138` — "Future work" list still lists REGN ambient as unbuilt; split music (shipped) vs incidental/sounds (pending)
- `docs/feature-matrix.md:146` — "Region ambient (REGN)" row is `✗`, should be `~ Partial`
- `ROADMAP.md:705` — M44 row pending-clause stale; also test count drift: systems/audio.rs has 12 tests (says 11), asset_provider/audio.rs has 13 tests (uncounted), total is 60 not 46
