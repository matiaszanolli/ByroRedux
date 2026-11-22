# #7283: PHYS-D1-01: `physics.md` still says the architecture stand-in covers every collision-less model

- **Severity**: LOW
- **Dimension**: Shape Translation (doc rot)
- **Location**: `docs/engine/physics.md:563-565`
- **Status**: NEW
- **Description**: The doc says the `ArchitectureTriMesh` path "remains available for collision-less architecture even when the NIF does not contain a packed object". Since #6549 (`0c35505ba`), `missing_collision_fallback` returns `None` for a classic-dialect model that authored no body (`authoring.walk_through_by_authoring()`). Those models get no stand-in: FNV sky planes and cave fungus stalks, Skyrim water planes. The stand-in remains only for authored-but-declined collision and for FO4+ / unknown provenance.
- **Evidence**: `byroredux/src/cell_loader/spawn.rs` `missing_collision_fallback`: `if authoring.walk_through_by_authoring() { return MissingCollisionFallback::None; }`.
- **Trigger Conditions**: n/a (documentation)
- **Impact**: A reader expects a solid stand-in under every collision-less architecture mesh and would misread the #6549 walk-through as a regression.
- **Related**: #6549, #6555
- **Suggested Fix**: Restate the stand-in's scope as the `missing_collision_fallback` rule (classic dialect with nothing authored → none).

**Found by**: `docs/audits/AUDIT_PHYSICS_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: physal.md's stand-in prose checked too
