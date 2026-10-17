# PHYS-D6-2026-10-16-01: `trap_contact_system` pays an O(all rigid bodies) scan, an O(all physics entities) SipHash map and an unfiltered shape-overlap every tick

Labels: low,physics,performance,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (cost)
- **Location**: `byroredux/src/systems/trap_hits.rs:86-98` (full `query::<RigidBodyData>()` filter on layer 14), `:146-150` (`owners` `HashMap` over every `RapierHandles`), `:170-177` (`bodies_overlapping_body` per trap), `crates/physics/src/world/queries.rs:577-601`; registered unconditionally at `byroredux/src/boot/schedule/update.rs:567`
- **Status**: NEW (hand to `/audit-performance` for the number; not measured here)
- **Trigger Conditions**: every tick, every game and cell. The scan runs even when no trap exists (the early return at `:143` comes after it). The map and the overlaps are built only while a scripted Trap-layer body exists or existed last tick (Skyrim dungeons with dart/swing traps, some exteriors).
- **Description**: (a) the Trap-layer discovery iterates every `RigidBodyData` (statics included, tens of thousands in an exterior) each frame to find a handful of layer-14 bodies that only change at cell load. (b) When traps exist, `owners` inserts every `(body, entity)` of the whole world into a std `HashMap` each tick, only to look up the few `others` returned by the overlap. (c) `bodies_overlapping_body` intersects the trap's colliders with every solid collider in its AABB, including the wall or floor it is mounted in, then the caller discards all but live actors; an actor-only `QueryFilter` predicate (interaction groups) would skip the trap-vs-world-mesh tests entirely.
- **Evidence**: code structure above; `trap_layer` is collected before the `traps.is_empty()` return, `owners` is built before the overlap block, `bodies_overlapping_body` filters only `exclude_sensors().exclude_rigid_body(body)`.
- **Impact**: a steady per-frame CPU tax scaling with loaded body count (order of 0.1-0.5 ms scan on a 95 k-body exterior by analogy to the #4682 / PHYS-D2-2026-10-12-02 numbers; several ms for the SipHash map when a trap is present in an exterior). CPU bound is the project's stated bug class, but this is small in interiors.
- **Related**: #4682, #2923 (hot-path hashing), PHYS-D2-2026-10-12-02 (same walk-the-arena shape, fixed).
- **Suggested Fix**: maintain the trap-body list incrementally (rebuild only when `RapierHandles` / `RigidBodyData` structure changes, or on cell load), resolve `owners` lazily for the few overlap results, and give the overlap query an actor-group predicate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
