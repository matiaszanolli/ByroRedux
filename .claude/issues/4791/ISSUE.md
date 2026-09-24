# #4791: PERF-D1-2026-09-23b-02: The player water sampler's `?` now returns from the whole function, so without a loaded XWCU marker the player never gets a water state

**Severity**: HIGH
**Labels**: high, water, gameplay, test-gap, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D1-2026-09-23b-02)

- **Severity**: HIGH (correctness, cross-dimension; route to `/audit-physics` / `/audit-gameplay` owners)
- **Dimension**: CPU Hot Paths (found tracing `0e607cbac`)
- **Location**: `byroredux/src/systems/character.rs:1108-1122` (`player_water_state`); test `:1650-1700`
- **Status**: NEW. It was introduced by `0e607cbac`, the fix for #4691 (itself a follow-up to #3974).
- **Description**:
  - At `f97775ca8` the marker lookup sat inside `.or_else(|| { let cq = world.query::<WaterCurrentVolume>()?; … })`, so the `?` ended only the closure.
  - #4691 rewrote it as a block expression, `let marker_flow = { let cq = world.query::<WaterCurrentVolume>()?; … };`. There the `?` propagates out of `fn player_water_state(...) -> Option<PlayerWaterState>`.
  - `World::query` returns `None` when the storage was never created (`crates/core/src/ecs/world.rs:476`, `self.storages.get(&type_id)?`). Storages are created lazily on first `insert`; the scheduler's `.reads::<WaterCurrentVolume>()` only records an access claim (`crates/core/src/ecs/access.rs:79-82`).
  - The only production insert is the REFR XWCU synth-child path (`cell_loader/references/synth_child.rs:18-36`). Nothing registers the storage at boot.
  - The only `register::<WaterCurrentVolume>` is inside the test (`character.rs:1657`), which is why the suite stays green.
- **Evidence**: `git show f97775ca8:byroredux/src/systems/character.rs` (the `?` inside the `or_else` closure) vs HEAD `:1109-1110` (the `?` in a plain block). The orchestrator re-read both, plus `World::query`, `Access::reads` and every `WaterCurrentVolume` insert site.
- **Impact**:
  - In every session that has not yet loaded an XWCU-bearing reference, `player_water_state` returns `None` for every water plane.
  - The kinematic player never swims: no buoyancy/swim mode, breath or drowning, authored water damage, or player `WaterContact`.
  - Whether the W1 smoke still passes depends on whether Lake Mead's cell set loads an XWCU ref; it has not been re-run since the commit.
  - The perf side is minor: the marker query and linear `.find` now run for every plane every frame, not just planes without a flow.
- **Related**: #4691, #3974; `docs/smoke-tests/w1-water-traversal.sh`
- **Suggested Fix**:
  - Hoist `let current_q = world.query::<WaterCurrentVolume>();` next to `flow_q` (`:1061`), and compute `marker_flow` with `current_q.as_ref().and_then(|cq| cq.iter().find(..).map(..))`.
  - Add a test that samples a plane on a world where the `WaterCurrentVolume` storage is **not** registered.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **LOCK_ORDER**: The hoisted `WaterCurrentVolume` query guard is acquired alongside the existing `WaterPlane`/`WaterVolume`/`WaterFlow` guards without violating TypeId-sorted acquisition
- [ ] **TESTS**: A regression test pins this specific fix
