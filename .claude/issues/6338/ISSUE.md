# PERF-D1-07: Flee, UseWeapon and the UseWeapon runtime resolve a FormID target with an O(all FormIdComponent) scan every frame, per actor

- **Issue**: #6338 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, performance, gameplay, ai, bug
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-24.md

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/flee_package.rs:108-120` (threat and refuge resolve in the per-actor, per-frame pass; `position_of(behavior.refuge_form_id)` is an eager argument to `flight_goal`); `byroredux/src/systems/use_weapon_package.rs:57-83` (target resolve per un-started actor per frame); `byroredux/src/systems/script_requests.rs:54-62` (`use_weapon_system` resolves `run.request.target` per live volley per frame, and builds a fresh `runs: Vec` per frame).
- **Description**: `resolve_entity_by_global_form_id` (`crates/scripting/src/condition/eval.rs:179-203`) walks every `FormIdComponent` entity and calls `FormIdPool::resolve` on each; its own doc says it is "fine for the occasional resolution ... not a per-frame hot path" (`0x14` short-circuits before the scan). Follow and Escort resolve once into `FollowState`; #5724 made Travel, Escort-lead and Guard do the same; `find_package` caches `FindProgress.target`. The three sites above resolve every frame:
  - `flee_package_system_inner` resolves the threat and the refuge for every eligible actor, including one merely holding at a safe distance, and the refuge scan runs even when a live threat decides the goal.
  - `use_weapon_package_system` collects not-yet-started actors into a fresh `Vec` and resolves each target every frame; the module doc says "a package whose target never loads simply never fires", so an actor whose target lives in another cell pays a **full miss scan every frame for as long as the package is active**.
  - `use_weapon_system` resolves the target of every live volley each frame. A package volley with Repeat Fire never finishes, so this is a permanent per-frame cost per active shooter.
- **Evidence**: scan body is `q.iter().find(|(_, fid)| pool.resolve(fid.0).is_some_and(|p| p.local.0 == form_id))`. Measured per call (two independent probes, see the CPU table): about 4.4 µs at 2k, 22–32 µs at 10–15k, 65–131 µs at 30–60k entities. The number of simultaneous such actors in real content is not measured (vanilla FNV authors about 15 Flee packages; the smoke gate has two shooters in ULKitchen).
- **Impact**: derived. Zero unless an actor carries one of these packages. In a 2,000-ref interior, single-digit µs per actor-frame. In a 10–60 k-entity exterior working set, 11–130 µs per call, so four actors resolving 1–2 targets each is roughly 0.1–0.7 ms of serial main-thread time per frame (the systems are exclusive). Linear in actor count and in loaded refs, no cap, and it grows with exactly the entity count the streaming work raises.
- **Related**: #5724 (the once-and-cache fix for the older movers), #2561 (shared resolve helper), #6313 (other per-frame whole-structure work), PERF-D1-08.
- **Suggested Fix**: cache the resolved `EntityId` in the behavior's state the way `FollowState` does (revalidate with `world.has::<GlobalTransform>`) and retry an unresolved target on a coarse clock (1 s). Better: one `FormId -> EntityId` index keyed on `FormIdComponent`'s existing structural generation (`TRACK_CHANGES = true`), which also serves `use_weapon_system`, `faction_hostility::ambush_gates` and `find_package::search`. Compute the refuge only when `flight_goal` needs it; keep `pending`/`runs` as persistent scratch like `FleePackageScratch`.


## Completeness Checks
- [ ] **SIBLING**: the same per-frame pattern is checked in the neighbouring package/AI systems (Follow, Escort, Travel, Guard already resolve once, #5724)
- [ ] **LOCK_ORDER**: if a query/resource lock scope changes, TypeId-sorted acquisition is preserved (the flee/find systems were already reworked once for an ABBA cycle, `ce22fcbcf`)
- [ ] **TESTS**: a regression test pins the fix (a resolve-count / scratch-capacity / settled-pass test in the style of `a_settled_pass_skips_the_walk_until_an_actor_appears`)

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-24.md`. Costs are labelled measured/derived in the report; no GPU frame time is claimed._
