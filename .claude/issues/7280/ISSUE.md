# #7280: PHYS-D5-02: `make_water_interaction_system` → `water_audio_system` is ordered by registration order alone

- **Severity**: LOW
- **Dimension**: Water / Buoyancy (Late-stage chain), test gap
- **Location**: `byroredux/src/boot/schedule/late/audio.rs:22-40`
- **Status**: NEW
- **Description**: The skill's Late chain is submersion → `water_damage_system` → death reconcile → `make_water_interaction_system` → water audio. Only the first links are named `run_after` constraints. The interaction system is registered as an anonymous closure (`make_water_interaction_system()`), so it cannot be named, and the audio system follows it only because it is registered after it. `submersion_runs_after_camera_follow_and_before_water_audio` does not check this pair.
- **Evidence**: Two consecutive `scheduler.add_exclusive_with_access(Stage::Late, …)` calls: the interaction system, which writes `SplashEvent` / `RippleEvent`, then `water_audio_system`, which reads them. There is no `run_after` between them.
- **Trigger Conditions**: any reordering of these registrations, or a scheduler change to how same-stage exclusives are ordered.
- **Impact**: If the order flipped, the splash and ripple markers would be cleaned up before the audio read them, and water sounds would go silent with no failing test.
- **Related**: #5434, #6702
- **Suggested Fix**: Give the interaction system a stable registered name and a `run_after`, or extend the scheduler-order guard to assert this pair.

**Found by**: `docs/audits/AUDIT_PHYSICS_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: other anonymous-closure Late systems checked for unnamed ordering
- [ ] **TESTS**: the order is pinned by a scheduler test
