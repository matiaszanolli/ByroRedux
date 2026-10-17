# PHYS-D4-2026-10-16-03: `FOOTSTEP_SURFACE_PROBE_REACH` is declared mis-indented at module scope between two structs

Labels: low,physics,tech-debt,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (path owned by Dim 4/6)
- **Location**: `crates/physics/src/world/queries.rs:30-38`
- **Status**: NEW
- **Description**: The #5743 constant and its doc comment are indented four spaces at module scope (as if inside an `impl`) and wedged between `CharacterMoveResult` and the doc comment of `CharacterMoveParams`. `cargo fmt` would re-indent it, so the file is not fmt-clean and the const reads as belonging to the neighbouring struct. Behaviour is unaffected (it is re-exported at `world/mod.rs:20` and used by `systems/audio.rs:281`; `footstep_probe_reach_clears_the_player_capsule_centre` pins the 64 BU vs 128 BU margin).
- **Impact**: Cosmetic / diff noise for the next `cargo fmt`.
- **Suggested Fix**: Re-indent and move it next to `PhysicsWorld::surface_material_under`, its only consumer-facing API.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
