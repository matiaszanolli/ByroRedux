# 3253: ECS-2026-08-24-05: physics_sync_system reads ActorBoneCollider and Parent undeclared

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-05)

## Description

Two component read-guards on `physics_sync_system`'s production path are missing from its `Access`, adjacent to — but not covered by — the now-CLOSED `#3121` fix (which added `TotalTime`/`WindField`/`WaterCurrentVolume` for the buoyancy phase only).

## Location

`byroredux/src/boot.rs:1257-1297` (declaration) vs `crates/physics/src/sync.rs:784` (`ActorBoneCollider`, `collect_newcomers`) and `crates/physics/src/sync.rs:1100` (`Parent`, `pull_dynamic`)

## Evidence

```rust
// collect_newcomers
let bone_q = world.query::<ActorBoneCollider>();
// pull_dynamic
let parent_q = world.query::<Parent>();
let global_q = world.query::<GlobalTransform>();  // declared
let transform_q = world.query::<Transform>();     // declared
```

## Impact

No live conflict today (Stage::Physics's parallel batch is also a singleton), but `Parent` is cross-stage-contended: `make_transform_propagation_system()` (PostUpdate) and `make_world_bound_propagation_system()` both declare `.reads::<Parent>()`. `sync.rs:1095-1099`'s own comment reasons in prose about this exact lock-order edge (#2135) — the invariant lives only in a comment, not in the machine-readable declaration.

## Related

Adjacent to closed `#3121`. Cross-referenced (not re-filed) by `AUDIT_CONCURRENCY_2026-08-24.md` Dimension 5 and `AUDIT_PHYSICS_2026-08-24.md` — both confirm this is live without presenting it as their own new finding.

## Suggested Fix

Add `.reads::<byroredux_physics::ActorBoneCollider>()` and `.reads::<byroredux_core::ecs::Parent>()` to the `physics_sync_system` block in `boot.rs`. Extend `scheduler_access_tests::physics_sync_declaration_reads_contact_config_and_faller_dump_types` with both needles.

## Completeness Checks
- [ ] **LOCK_ORDER**: `Parent`/`Transform` lock-order edge (#2135) preserved and now machine-checked
- [ ] **TESTS**: Extend the physics_sync declaration test with both needles
