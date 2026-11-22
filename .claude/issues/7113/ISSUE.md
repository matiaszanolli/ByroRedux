# #7113 — ESM-D7-01: `scripted_hit_target` uses raw `World::find_by_form_id` (full `FormIdComponent` scan per scenery impact)

Labels: low,esm-plugin,performance,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D7-01)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D7-01**

- **Severity**: LOW (event-bounded, about 4 µs at 2 k entities and 65-131 µs at 30-60 k `FormIdComponent`s per non-actor projectile stop, per the pool's own measurements)
- **Dimension**: 7 (EsmIndex → ECS handoff, resolver discipline)
- **Record / Sub-record**: none (runtime FormID → entity)
- **Location**: `byroredux/src/systems/projectiles.rs:573-578` (`scripted_hit_target`, called at `:481`); the scan is `crates/core/src/ecs/world.rs:444-449`
- **Status**: NEW (the 11-08 audit recorded "zero production callers" of raw `find_by_form_id`; `5fbfdc3f7`, 2026-11-11, added this one)
- **Description**: when a projectile stops on a non-actor collider that names its placement by `PhysicsSourceForm` (a NIF root collision with no `Parent`), the code does `world.find_by_form_id(source.0)`, which iterates every `FormIdComponent` and returns the first match in storage order. Every other FormID → entity lookup in `byroredux/src` (46+ sites: package systems, barter, travel, door/MoveTo, snapshots, quest commands) goes through `byroredux_scripting::condition::resolve_entity_by_global_form_id`, which answers from the generation-keyed `FormIdPool::entity_index` (#6338). This site holds a runtime `FormId` handle, not a raw u32, so `FormIdPool::local_of(handle)` + `entity_with_local` (or an `entity_with_handle` on the pool) is the indexed equivalent; the duplicate-id rule is identical (first entity in storage order wins), so behaviour does not change.
- **Evidence**: `rg 'find_by_form_id' byroredux/src` outside tests and comments returns only `projectiles.rs:577`. `PhysicsSourceForm(pub FormId)` is a pool handle (`crates/core/src/ecs/components/physics_source.rs:30`). The path runs once per arrow/bullet that ends on scenery (the New Vegas sarsaparilla-bottle `OnHitWith` case it was written for, but it executes for every scenery stop with a `PhysicsSourceForm` collider, whether or not a script listens).
- **Impact**: a full `FormIdComponent` scan per scenery impact. Never per frame, so it is a hitch risk only under automatic fire or many simultaneous arrows in a populated cell, not a correctness fault.
- **Suggested fix**: add a handle-keyed lookup beside `entity_with_local` (same cached build, `by_handle: HashMap<FormId, EntityId>` filled in the same pass), call it here, and have a guard test assert no `World::find_by_form_id` caller in `byroredux/src` production text (`source_scan::production_text`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
