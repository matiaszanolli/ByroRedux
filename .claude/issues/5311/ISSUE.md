# #5311: TD1-2026-10-05-01: `crates/physics/src/world.rs` crossed 2000 production LOC (1659 → 2101 in five commits)

Labels: low,tech-debt,bug,physics
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD1-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/physics/src/world.rs` (4495 total lines, 2101 production)
- **Status**: NEW (first crossing)
- **Age**: `ea04f2689` (09-30, #5126/#5127) → `483776fe5` (#5160) → `44f7bab55` / `5ae7f8ad4` (#5161) →
  `e8de9f8c8` (10-04, #5246). +760 −50 lines.
- **Effort**: medium
- **Description**: the file already has two internal seams, and the window's growth sits in a third:
  - **Lifecycle and step** (`impl PhysicsWorld` at 454-1394): `new`, body add/remove, forces, motion type,
    `step` (1144-1388, **244 lines**).
  - **Queries and character motion** (`impl PhysicsWorld` at 1469-2076): `cast_ray*`, `cast_ray_corridor`,
    `line_of_sight_blocked`, the capsule probes, `colliders_near_xz`, `static_colliders_aabb`,
    `move_character`, plus the `CharacterMove*` types and the group/filter helpers at 129-205.
  - **Explosion recovery and containment** (the growth): `DynamicBodySnapshot` / `body_needs_recovery` /
    `restore_invalid_dynamic_bodies` (365-454), `refresh_query_geometry_after_restore`,
    `recover_pre_broken_bodies`, `accept_keyframe_target`, the label/refusal counters and
    `clamp_explosive_velocities` (822-1114, 145 lines), plus the sanity-cap constants at 59-110.
- **Suggested Fix**:
  - Turn `world.rs` into `world/mod.rs` holding the struct, lifecycle and `step`. Move the recovery cluster to
    `world/recovery.rs` and the query/KCC impl block to `world/queries.rs`.
  - Pull `step`'s per-substep body (snapshot → pipeline.step → clamp → restore) into a `run_substep` helper.
  - Mind *feedback_file_split_include_str*: grep for `include_str!("world.rs")` before moving anything. The
    `wake_contract_tests` module (4382+) reads the file's own docs.
- **Related**: SAFE-D3-2026-10-05-01 (the recovery maps that `remove_body` does not prune live in this
  cluster); PHYSICS-2026-10-05 findings at `world.rs:91-99`, `:949-1048`, `:1295-1314`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
