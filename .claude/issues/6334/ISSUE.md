# PERF-D1-2026-10-23-04: archetype_idle_system per-frame Vec + retain

- **GitHub**: #6334
- **Severity**: LOW
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-23.md

Found by `/audit-performance` 2026-10-23 (PERF-D1-04). Derived cost; unmeasured.

## Location
`byroredux/src/systems/archetype_idle.rs` ~55-110: per-frame `actors: Vec<(EntityId, EntityId)>` (~68-77), `seen.unmatched.retain(..)` (~72), per-actor `Dead`/`ArchetypeIdle`/`ArchetypeIdleSeen` probes (~81-87). FO4 only (gated by `populate_archetype_idles`, `asset_provider/archetype_idle.rs`).

## Description
Once an actor has settled (marked `ArchetypeIdle`, or recorded as `unmatched`) the system does nothing for it, yet every frame it collects every `AnimationTarget` entity into a fresh `Vec`, takes the `ArchetypeIdleSeen` write lock to `retain` the unmatched set against live `AnimationTarget` storage (added by `15bf18ab6`, #6292), and does three sparse probes plus a resource-lock probe per actor. The answer can only change when `AnimArchetypeOverrides` changes, an `AnimationTarget` is added, or an actor loses its `AnimationPlayer`/marker.

## Impact
Derived: tens to a few hundred animated actors in FO4 settlements → a few µs to ~30 µs per frame plus one allocation. Negligible on the 16-core target; it is the pattern Dim 1 names ("ambient actor systems run every frame on every NPC; persistent scratch, not fresh `Vec`s"). Related: #6292.

## Suggested Fix
Key the pass on `AnimationTarget`'s structural generation (`TRACK_CHANGES` opt-in) plus the overrides revision and run the walk/`retain` only when the key moves; or hold a persistent scratch `Vec` in `ArchetypeIdleSeen` and `clear()`+`extend`.

## Completeness Checks
- [ ] **LOCK_ORDER**: The `ArchetypeIdleSeen` write lock is not held across the `AnimationTarget` query in a new order
- [ ] **SIBLING**: Same pattern checked in related files/systems
- [ ] **TESTS**: A regression test pins this specific fix
