# NIFAL-D2-2026-11-08-02: The placement-root replacement rule is absent from `nifal.md` and the NIFAL skill

GitHub: #6820 (snapshot as filed; GitHub is authoritative for state)
Labels: low,nifal,nif-parser,documentation,doc-rot,game:fo3,game:fnv,game:fo4

- **Severity**: LOW (doc drift)
- **Dimension**: Geometry/Transform
- **Tier Violated**: harness-gap
- **Game Affected**: FO3 / FNV / FO4 (replace) vs Oblivion / Skyrim / FO76 / Starfield (compose)
- **Location**: `docs/engine/nifal.md` §2 Geometry/Transform; `.claude/commands/audit-nifal/SKILL.md` Dim 2. The rule is in `byroredux/src/cell_loader/references/import.rs:25-47`, `crates/nif/src/scene.rs:210` (`clear_root_transform`), and the audit-fnv / audit-fo4 skills.
- **Status**: NEW
- **Description**: 62782dd3b and 7f2d98944 add one mutation site on the raw scene, applied before every importer. The cell loader decides per game through `placement_replaces_model_root` (FO3/FNV and FO4). The Dim 2 audit traced every REFR cache-fill path (sync, SCOL/PKIN children, streaming worker, precombined) and found each routes through it. The deliberate exemptions are ground cover, LOD and `_physics.nif`. The code and its pins (`placement_replaces_model_root_only_for_verified_games`, two `clear_root_transform_*` tests) are consistent. Only the spec and the skill lack a checklist line.
- **Failure scenario**: A new REFR cache-fill path skips `prepare_placed_model_scene` and nothing in the NIFAL skill says to check.
- **Suggested fix**: Add a "Placement-root replacement" paragraph to `nifal.md` §2 and one Dim 2 checklist line naming the callers and the exemptions.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test or guard pins this specific fix


_Source: `docs/audits/AUDIT_NIFAL_2026-11-08.md` (HEAD 51ce1c2f9)._
