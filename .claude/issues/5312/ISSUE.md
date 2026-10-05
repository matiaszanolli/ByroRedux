# #5312: TD2-2026-10-05-02: The two `npc_spawn/resumable` state machines carry a 94%-identical Armor phase, and the copies already diverge in diagnostics

Labels: low,tech-debt,bug,gameplay
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD2-2026-10-05-02) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: `byroredux/src/npc_spawn/resumable/prebaked.rs:252-301` and `byroredux/src/npc_spawn/resumable/runtime.rs:794-845`.
  The Skeleton arms are at `prebaked.rs:151-193` and `runtime.rs:508-574` (56% similar).
- **Status**: NEW. The #5091 split (`37db35cca`) made the two copies siblings. They previously lived in one file.
- **Effort**: small
- **Description**:
  - Both Armor arms do the same work: extract the model, build the `hide_skin_partitions` pre-spawn closure,
    call `load_nif_bytes_with_skeleton` with the same nine arguments, `parent_equipment_part`, track
    `original_roots`, and count the armor.
  - `difflib` finds one difference. The runtime miss log prints `armor.resolved_fid` plus
    `(from CNTO {source_fid})`, while the prebaked one prints `armor.form_id` only, so the prebaked path loses
    the CNTO provenance.
  - The Skeleton arms share the extract → load → install-fallback-ragdoll core. Only the runtime one logs "no
    root entity", and the prebaked one carries a comment saying it mirrors the runtime arm ("Same no-bone-collider
    contract as the runtime Skeleton phase").
- **Suggested Fix**:
  - Move the Armor unit into `resumable/mod.rs` as `spawn_armor_unit(state_parts…, armor) -> UnitOutcome`, next to
    the helpers both arms already share (`hide_skin_partitions`, `parent_equipment_part`).
  - Extract the Skeleton phase's common core the same way.
- **Related**: LC-D3-02 / #5079 (the Child-flag translation is duplicated between `player_body.rs` and
  `resumable/runtime.rs`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
