# TD8-2026-10-27-01: `MeshRegistry::actor_geometry_admission_open` (22d6e1a80) has no production reader

Labels: low, renderer, tech-debt, bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-27.md

# TD8-2026-10-27-01: `MeshRegistry::actor_geometry_admission_open` (22d6e1a80) has no production reader

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` finding `TD8-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `crates/renderer/src/mesh.rs:997-1001` (`actor_geometry_admission_open`); sibling readers `byroredux/src/app_step.rs` (~:143-146, `prop_geometry_admission_open`) and NPC/player spawn `spawn_nif_mesh` in `byroredux/src/scene/nif_loader.rs` (~:1226-1237).

## Description
The two sibling latches both have readers: `prop_geometry_admission_open` gates the tree, placement and object LOD pre-checks and the LOD re-arm edge; `scene_geometry_admission_open` gates terrain LOD. The actor latch is read only by tests. NPC and player spawn attempts the upload and lets it fail; nothing re-arms on a closed actor tier or reports it. Found by a sweep of the 130 `pub fn`s added this window; the other five with only test callers are documented test/tooling helpers kept public on purpose (#6398, #6372).

## Evidence
`grep -rn actor_geometry_admission_open crates byroredux` -> definition at `mesh.rs:999` and two test assertions (`mesh.rs:2399`, `:2404`) only.

## Impact
Dead accessor on a tier that exists precisely to protect actor uploads; a closed actor tier fails silently at spawn.

## Related
SPT-2026-10-27-D4-01, TD3-2026-10-27-02.

## Suggested Fix
Either add an NPC-spawn pre-check plus a reopen log edge, matching the LOD one, or remove the public accessor until it has a consumer. Do it together with SPT-D4-01.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

