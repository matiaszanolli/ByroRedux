# #1594 — FO4-D9-MEDIUM-02: FO4 BSConnectPoint attach-point data lifted to ImportedScene but never consumed

**Severity**: MEDIUM · **Dimension**: Real-Data Validation + Forward Scope
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D9-MEDIUM-02)
**Location**: `crates/nif/src/import/mod.rs:212-242` (lift); `crates/core/src/ecs/components/attach_points.rs` (`AttachPoints` / `ChildAttachConnections` defined + unit-tested only)

## Description
BSConnectPoint::Parents (1,124) and ::Children (1,055) parse cleanly and are lifted into `ImportedScene.attach_points` / `child_attach_connections` (the #985 work). The ECS components exist. But a repo-wide grep finds NO call site that spawns these components onto entities or reads them to drive attachment — the chain dead-ends at the import boundary (parse → ImportedScene → nothing). The code comment at `mod.rs:212-216` itself notes the OMOD/material-swap subsystem (#973) "can't function without these reaching the ECS."

## Evidence
`grep -rn 'attach_points|child_attach_connections'` outside `crates/nif/src/import/*` → only `scene_import_cache.rs` initialising them to `None` + the component def/tests; no consumer. `AttachPoints`/`ChildAttachConnections` referenced only in their own def + tests. 2,179 ConnectPoint blocks in `Fallout4 - Meshes.ba2` alone produce data dropped after import.

## Impact
Record-parsed-but-unconsumed. Modular weapons and power-armor frames import as their base receiver/frame only; mod parts (barrels, stocks, armor plates) attached via connect points are not positioned/spawned. Functional gap, not a crash.

## Related
#985 (lift to ImportedScene — done), #973 (OMOD subsystem — downstream); #1359 (CONT, same parsed-but-unconsumed shape).

## Suggested Fix
Add a cell-loader / npc-spawn step that materializes `ImportedScene.attach_points` into the `AttachPoints` ECS component on the spawned root entity, plus a system that resolves `child_attach_connections.point_names` to attach child meshes at the named parent connect point. File as the consumer half of the #985/#973 arc.

## Completeness Checks
- [ ] **SIBLING**: Both `attach_points` and `child_attach_connections` materialized; the CONT/MOVS parsed-but-unconsumed siblings tracked alongside (categorised non-STAT spawn)
- [ ] **CANONICAL-BOUNDARY**: Consumer reads `ImportedScene` data at spawn; no per-game logic re-derived at render time
- [ ] **TESTS**: A regression test pins `AttachPoints` being spawned onto the root entity and child resolution by point name
