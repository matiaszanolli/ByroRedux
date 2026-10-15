# TD1-2026-10-15-03: Each M47.2 native touches six hub files, and all six grew in three days — `register_late_systems` 615 → 808, `build_save_registry` 259 → 368, `apply_at_depth` 188 → 621

Labels: low,scripting,ecs,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-03`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `byroredux/src/boot/schedule/late.rs:15` (`register_late_systems`, 808 lines) and `update.rs:17` (`register_update_systems`, 763, was 661); `byroredux/src/save_io.rs:394` (`build_save_registry`, 368, was 259); `crates/scripting/src/lib.rs:310` (`register`, 241, was 72); `crates/scripting/src/fragment/effects/deferred.rs:380` (`apply_at_depth`, 621, was 188); `crates/scripting/src/scene/quest_alias.rs:793` (`refresh_scene_actor_bindings`, 484, was 332); `byroredux/src/asset_provider/script.rs:915` (`populate_scene_runtime`, 420, was 278)
- **Status**: NEW (`register_*_systems` is the regrowth of closed #3739/#3855; `build_save_registry` rides on #5585)
- **Age**: all since 2026-10-12; stat of four sampled commits (`a0045b0cf` DamageObject, `8b3b75d4d` SetActorOwner, `5320d4ac9` SetBribed, `f597efa83` SetLocRefType): every one touches `late.rs`, `save_io.rs`, `save/src/snapshot.rs`, `save_io/serde_default_guard_tests.rs`, `scripting/src/lib.rs`, `fragment/effects.rs` (+`vm.rs` or `translate/effects.rs`) and `docs/engine/scripting.md`
- **Effort**: large (a seam, not a move)
- **Description**: the M47.2 subsystems are good modules (`crime_gold.rs`, `ownership.rs`, `loc_ref_types.rs`, `destruction.rs`, …), but each one is wired in by hand-editing six central lists: a `pub use` + a `try_resource().is_none() → insert_resource(T::default())` block in `scripting/lib.rs::register` (37 such blocks), a system row with an `Access` chain in `late.rs`, a registry row in `build_save_registry` + a snapshot-format touch + a serde-default guard row, a queue field in `DeferredFragmentEffects` with its flush block in `apply_at_depth` (45 `if !self.x.is_empty()` blocks, L396–935), and a `fragment/effects.rs` arm. The wiring grows linearly with natives and none of it is owned by the subsystem.
- **Evidence**: `fnscan.py` base-vs-HEAD table (values in Location); `grep -c 'is_none()' crates/scripting/src/lib.rs` region = 37; `git show --stat` of the four commits above.
- **Impact**: the same hot files collide on every native; `apply_at_depth` and the two `register_*` functions are already 600–800 lines each; `save_io.rs` crossed the line because of the registry table (#5585).
- **Related**: #5585 (save_io split), #3739 / #3855 (closed, the five-file `register_*` split), finding 01/02, `TD1-2026-10-15-04`.
- **Suggested Fix**: give each subsystem an `install` entry point (`pub fn install(w: &mut World)` for resources, `pub fn register_systems(s: &mut Scheduler)` for its rows, `pub fn register_saved(r: &mut SaveRegistry)`), and have the hubs call a flat list of `<subsystem>::install(..)` lines. For `apply_at_depth`, the smaller first step is below. Do this incrementally, one subsystem per commit, with the existing `registry_completeness_tests` as the safety net. Dim 2 pointer: the 37-block `if try_resource::<T>().is_none() { insert_resource(T::default()) }` idiom occurs **160 times** workspace-wide (`scripting/lib.rs` 41, `scene/world_setup.rs` 12, `story_manager.rs` 8, `dialogue.rs` 7) and `World` has no `init_resource::<R: Default>()`; adding it removes ~110 lines from `register` alone.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
