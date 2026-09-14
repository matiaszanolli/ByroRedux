# #4327 SCR-D7-2026-09-14-02: actor jobs and LIGH-only / fxlight placements ignore `ReferenceEnableState`, so a `Disable()`d NPC or light respawns with full content on the next load

**Labels**: medium,scripting,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references/mod.rs` `load_references_budgeted` actor arm (builds its own placement root, bypassing `spawn_placed_instances`); `byroredux/src/cell_loader/references/synth_child.rs` LIGH-only and fxlight branches
- **Status**: NEW (#3278's completeness checklist asked for a consumer test; the shipped tests exercise only the mesh path)
- **Description**: `NpcSpawnJob` never reaches the #3278 gate, so a disabled ACHR/ACRE spawns its body, armor, skeleton and collision, and gets identity plus scripts. The LIGH-only and fxlight branches build `LightSource` entities directly. #3278's comment says one gate "covers all three at once" (render/collide/light); that is true only for the mesh path.
- **Evidence**: `grep placement_is_disabled` finds one production call site. The orchestrator confirmed the actor arm `continue`s before `spawn_synth_child`.
- **Impact**: A Papyrus `Disable()` on an actor, common in quest fragments, is silently undone on the next cell load, leaving a visible, collidable, alias-fillable NPC the script removed. Disabled lights come back the same way.
- **Related**: #3278, #3789, SCR-D7-2026-09-14-01
- **Suggested Fix**: Hoist the enable check to the per-REFR level in `load_references_budgeted`, before the actor/synth dispatch, with a per-branch policy (identity-only spawn vs skip), and test the actor and LIGH-only arms.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
