# #6312: PERF-D1-01: `trap_contact_system` still rebuilds its ancestor chains, script owners and a std `HashMap`/`HashSet` every tick while any Trap-layer body is loaded

**Labels**: low,performance,physics,bug

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-21.md · **Severity**: LOW · **Dimension**: CPU Hot Paths · **Location**: `byroredux/src/systems/trap_hits.rs (trap_contact_system)`

- **Location**: `byroredux/src/systems/trap_hits.rs:109-245`. In order: the `chains` `Vec<Vec<EntityId>>` (`:141-156`); the `scripted` resolution with a std `HashSet` plus `source_reference` → `FormIdPool` + `ReferenceScriptIndex::lookup` (`:163-191`); `live_actor_bodies` std `HashMap` over every live actor (`:62-87`, `:211`); `now` std `HashSet` (`:230`).
- **Status**: NEW (residual of #5943, closed). #5943 cached the Trap-layer list on `RigidBodyData`'s structural generation, but the derived chains and owners are recomputed every tick.
- **Description**: the trap → scripted-owner map depends only on `Parent`, `PapyrusReferenceScripts` and `RapierHandles` structure. It is invariant between structural changes, yet it is rebuilt every frame with fresh allocations (one `Vec` per trap body). The actor-body map is rebuilt over all live actors with SipHash on the per-frame path. The skill treats a std map on a per-frame per-entity keyspace as a regression class.
- **Evidence**: code read. Derived: a Skyrim dungeon with ~20–50 trap bodies and ~20 actors costs tens of µs per frame. Unmeasured; no quantitative guard exists.
- **Related**: #5943, #2923.
- **Suggested Fix**: cache `traps: Vec<(owner, body)>` beside `TrapLayerCache`, keyed on the same generation plus the `Parent` / `PapyrusReferenceScripts` generations. Use `FxHashMap` / `FxHashSet` for `live_actor_bodies` and `TrapContacts`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
