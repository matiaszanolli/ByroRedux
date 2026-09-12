### SAVE-D4-2026-09-11-02: `validate_cinematic_entity_refs`'s doc comment still enumerates "four reference-class checks" and describes `validate_animation` as covering only `AnimationPlayer` — both stale

- **Severity**: LOW
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: none (doc rot — no functional impact)
- **Location**: `byroredux/src/save_io.rs:725-736`
- **Status**: NEW. Introduced at `90ae915c` (#2535) when `validate_world` genuinely had four checks; never updated across every subsequent addition, including this cycle's `validate_animation` extension (#3791).
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: `validate_world` runs seven core checks today (confirmed directly: `validate_hierarchy`, `validate_equipment`, `validate_saved_entity_references`, `validate_animation`, `validate_inventory_instances`, `validate_progression_state`, `validate_material_finiteness` — `crates/save/src/validate.rs:71-79`), not four; `validate_animation` covers `AnimationPlayer`, `AnimationStack`, and `Seated.animation_restore.clip_handle`, not "only `AnimationPlayer`" as the comment claims seven lines from the very fix (#3791) that changed it.

**Impact**: Documentation-only; risk is to a future auditor/contributor taking the enumeration at face value.

**Related**: None open.

**Suggested Fix**: Update the comment to name the two hazards this function adds without re-enumerating `validate_world`'s internals (the enumeration is what keeps going stale).

## Completeness Checks
- [ ] **TESTS**: No test change needed — comment-only fix
