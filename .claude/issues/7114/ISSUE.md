# #7114 — ESM-D7-02: `TravelIndex` keys its cached `SpaceGraph` on `Arc::as_ptr` without holding the index; a recycled address serves another load order's graph

Labels: low,esm-plugin,ai,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D7-02)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D7-02**

- **Severity**: LOW (needs two successive live loads of different plugin sets with no Travel tick in between; the effect is wrong or missing Travel routes until restart)
- **Dimension**: 7 (EsmIndex → ECS handoff, load-order-resolved caches)
- **Record / Sub-record**: none (`LoadedCellIndex` identity)
- **Location**: `byroredux/src/cell_loader/relocated.rs:67` (`TravelIndex(Mutex<Option<(usize, Arc<TravelData>)>>)`) and `:79-103` (`travel_data`, `key = Arc::as_ptr(&index) as usize`); same pattern, lower exposure, at `byroredux/src/music/mod.rs:205` (`current_catalog`) and `byroredux/src/systems/story_events.rs:69` (`context_identity`)
- **Status**: NEW (the cache was added by `7af55a0df`/#6852; `0e8053225` fixed the sibling staleness for six other resources by comparing a `LoadOrderBasis`, but did not touch this one)
- **Description**: `travel_data` rebuilds `SpaceGraph` (every placed load door of every cell, `index.navmesh_door_links()`) and `interior_actors` only when the pointer key changes. The slot stores the derived `TravelData`, not the `Arc<EsmIndex>`, so nothing keeps the old index allocated. `SharedParseCache` also holds only the most recent plugin set. Sequence: load A (cache built for address a) → live load B (new `Arc` allocated while A is still alive, so address b ≠ a; A freed once the resource and the parse cache both drop it) → no Travel actor ticks while B is current (an interior with none) → live load C allocates its `Arc` while B is alive and reuses A's freed chunk at address a (same-size allocation, glibc reuse is the likely outcome) → `travel_data` sees `built_for == key` and returns A's graph for C. The load-order discriminator `LoadOrderBasis` from `load_order.rs:354` is exactly what `0e8053225` introduced to avoid this class.
- **Evidence**: `rg 'Arc::as_ptr' byroredux/src` shows the three sites. `TravelIndex` is only `init_resource`d at boot (`boot/world.rs:462`) and never removed (`rg TravelIndex` outside tests: boot + definition). The music catalog and story memo refresh every frame, so a stale address cannot survive there unless their systems are skipped; only the Travel cache is lazy.
- **Impact**: cross-space Travel (`route_toward`, `plan_journey`) would route through door FormIDs of another load order: `resolve_entity_by_global_form_id(first.door)` misses and the actor reports `Unroutable`, or, if the slot layouts coincide, walks to a wrong door. No save corruption (the cache is `NOT_SAVED_BY_DESIGN`).
- **Suggested fix**: key the cache on something that cannot be recycled: a monotonically increasing generation stamped on `LoadedCellIndex` when it is replaced, or `Weak<EsmIndex>` with `ptr_eq` plus a live `upgrade()`. Do not store the strong `Arc` (it would pin a ~1 GB index).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
