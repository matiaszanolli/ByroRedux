**HEAD**: `670f81917` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-10-27.md` (@ `fa6be4a6b`, 136 commits ago) · **Audited**: Dim 3 (all three lock-order lanes re-run; new M43 system bodies: crafting, mines, attraction, dead_body, iron_sights, random_conversation, lockpick, jail, Story Manager producers), Dim 4 (scheduler proof, cross-stage HitEvent order, new exclusives), Dim 5 (PhysicsWorld-sink re-scan, #6444 vats fix) · **Unchanged since baseline (skimmed)**: Dim 1 (no sync-primitive change), Dim 2 (the only renderer edits are the #6358 gpu_timers table refactor, the #6415 camera-transmittance push lane and constant renames), Dim 6 (teardown, resize and acceleration untouched), Dim 7 (no new thread, rayon fan-out or shared lock; only #6114 / #6464 main-thread edits)

# Concurrency & Synchronization Audit, 2026-10-31

**Command**: `/audit-concurrency` (all 7 dimensions, depth `deep`), run inside `/audit-suite --preset comprehensive`. Solo run: no sub-agents, no source edits, no engine or GPU process. Built in the repo `target/` with `CARGO_INCREMENTAL=0`.
**Dedup**: `/tmp/audit/issues.json` (98 open), `SUITE_RULES.md` § 6 (safety, renderer and ECS audits read), and the 2026-10-27 report.

**Counts: 0C / 0H / 0M / 0L new.** Two Dup rows (ECS-2026-10-31-01 HIGH, ECS-2026-10-31-02 LOW), kept only to give the concurrency-side evidence.

## Guard runs at HEAD

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` | **RED: 3422 passed, 2 failed, 79 ignored.** One lock-order panic (CONC-2026-10-31-01, Dup). The other failure, `readme_and_roadmap_hold_their_line_ceilings` (ROADMAP.md 808 lines vs the 800 ceiling), is not concurrency. |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib` | core 888/0, physics 270/0, scripting 1307/0. **Green**, so the three 10-27 roots are gone. |
| `cargo test -p byroredux-renderer --lib` | 1503 passed, 0 failed, 1 ignored. Every GPU-sync guard from the skill list ran and passed. |
| Scheduler proof (`build_scheduler_reports_zero_access_conflicts`, `scheduler_access_invariants_hold_on_the_real_schedule`, all `system_access_declaration_tests`, the ordering pins, the CI-lane pins) | all `ok`. No `#[ignore]` in `scheduler_access_tests.rs` or `boot/schedule/mod.rs`. `per_frame_driver_functions_stay_within_their_size_pins` is green again (#6143). |

## 10-27 carry-over

- ECS-2026-10-27-01/-02/-03/-05, which the 10-27 report carried as CONC-2026-10-27-01/-02, are all fixed: #6443 (`apply_pending_group_swaps`), #6444 (`vats_scan`), #6445 (`weather_system`) and #6447 (`player_controller_system` declares the `PendingScriptStoryEvents` write; `raise_story_event` is a listed hop).
- #6444 also removed the standalone `PhysicsWorld → RapierHandles` sink inversion. `visible_fraction` now receives `bodies_by_target` precomputed and takes only the `PhysicsWorld` read for `cast_ray`.

## Findings

### CONC-2026-10-31-01: `improve_plan` holds an `Inventory` read guard across an `ItemInstancePool` read, the reverse of the engine's order
- **Severity**: HIGH under the red-lane rule (as the 10-27 cycles were rated); not a live cross-thread deadlock today.
- **Status**: Dup of AUDIT_ECS_2026-10-31 ECS-2026-10-31-01. Not re-filed.
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/crafting.rs:222-236` (`improve_plan` holds `world.get::<Inventory>(player)` while `stack_quality` at `:205-210` takes `ItemInstancePool`). The test helper `quality_of` at `:768` does the same. Introduced in 2c55546f4 (2026-10-30, M43 improving).
- **Evidence**: `improving_raises_a_held_item_in_place_and_never_hands_out_a_copy` panics under `BYRO_LOCK_ORDER_CHECK=1` with `ItemInstancePool → Inventory → ItemInstancePool`. It passes in isolation.
  - The reverse edge comes from `reference_state::tests::unload_captures_do_not_invert_production_lock_orders`. I bisected it by running each `reference_state` test alone before the crafting test.
  - That fixture primes `Pool → Inventory` on purpose, per #4982. Production `validate_inventory_instances` (`crates/save/src/validate.rs:488`, pinned by `validate_inventory_instances_takes_the_pool_before_the_inventory_query`) takes the pool first.
- **Trigger Conditions**: a crafting-menu action (`recipes_at` / `improve_item`) on the main thread racing a save validation on another thread. No such second thread exists today, so the failure is the CI lane going red.
- **Other Inventory/Pool sites checked**: `armor::totals` and `inventory::snapshot` clone the inventory before the pool read; the consume and drop paths release the Inventory guard before `resource_mut::<ItemInstancePool>`. `improve_plan` is the only inversion.
- **Suggested Fix**: copy the rows (`inventory.items.clone()` or `(index, ItemStack)` pairs) and drop the guard before `stack_quality`, in `improve_plan` and `quality_of`.

### CONC-2026-10-31-02: `mine_system` is a new `HitEvent` producer missing from the registration-order pin
- **Severity**: LOW. **Status**: Dup of AUDIT_ECS_2026-10-31 ECS-2026-10-31-02. Not re-filed.
- **Dimension**: Scheduler Access Declarations (cross-stage sequencing)
- **Location**: `byroredux/src/systems/mines.rs:280` writes `HitEvent`. The producer list of `hit_event_consumers_run_after_every_producer` (`byroredux/src/boot/schedule/mod.rs:135-144`) does not name it.
- **Today's order is correct**: `mine_system` is registered in `quest_scene.rs` (segment 3). `hit_event_system`, `reference_script_hit_system` and `obscript_hit_system` are in `script_runtime.rs` (segment 4), and `combat_damage_system` is in `player_animation.rs` (segment 5). A segment reorder (#6359) would silently drop mine blasts, because `event_cleanup_system` clears the marker in Late.
- **Suggested Fix**: add `"crate::systems::mines::mine_system"` to the producer list.

## Per-dimension notes

### Dim 1: Vulkan queue and AS sync
`git diff fa6be4a6b..HEAD -- crates/renderer/src` adds or removes no `queue_submit`, `queue_present`, `wait_for_fences`, `device_wait_idle`, `destroy_acceleration_structure`, barrier or thread. The `gpu_timers.rs` change (#6358, 1190 lines) is a `TimerPass` row table, with the same stage and bit per pass. Source-scan guards were updated to read the table rows. The queue-guard, one-time-fence-wait, FIF-contract, `render_finished` and `dynamic_rgba` guards all passed. The open #6506 (`moved_surface_buffers` not destroyed) is a cleanup defect and is not re-filed. No findings. This dimension was read only; there was no `BYRO_VALIDATION` run.

### Dim 2: compute chains
The non-timer edits are:
- the camera cloud-shadow transmittance, packed into `CompositeParams::sky_aperture_count.y` (#6415);
- `WATER_CLEAN_IOR` and the shore constants, renamed into core (#6418, #6419);
- a combustion-light luma log line.

None adds a descriptor, dispatch or barrier. The transmittance is read back per-FIF behind the existing fence, and a non-finite value is clamped to 1.0. The skin-publish, TAA-order, skip-clear-latch and ground-cover promote guards passed. No findings.

### Dim 3: ECS lock ordering
Lane results are in the table. New system bodies read for guard lifetimes:
- `mine_system`, `live_actors`, `placed_mines`, `attraction_system`, `dead_body_discovery_system`, `iron_sights_system`: snapshot-then-use shapes. The `DeadBodyState` and `AttractionState` guards are dropped before the storage scans.
- `random_conversation_system`: `RandomConversationState` is held across `StoryManagerRng` draws. The RNG is a leaf, so this is a safe order and is documented in the code.
- `collect_participants`: the `AmbientPackageRuntime` guard is released before `PackageRegistry`, and the registry is dropped before `npc_refuses_dialogue`.
- All new systems are Update exclusives registered ahead of the Story Manager dispatch.

`extensions/` has no commits in the window. Only CONC-2026-10-31-01 is a lock-order finding.

### Dim 4: scheduler proof
Live and non-vacuous: 9 `add_to_with_access` sites; `PARALLEL_SYSTEMS` coverage and cross-file hops pass. The new M43 systems are exclusives, so they are outside the pairwise analysis. See CONC-2026-10-31-02 for the one registration-order pin gap.

### Dim 5: RwLock patterns and physics
The physics lane is green (270/0). A grep of added lines for `PhysicsWorld` finds only test fixtures. `eat_sleep.rs` has a comment-only change. `crates/physics/src/water.rs` changed in constants only. The #6444 fix keeps `visible_fraction` a pure `cast_ray` holder.

### Dim 6: resource lifecycle
No changes under `context/teardown.rs`, `resize.rs`, `acceleration/` or `scene_buffer/`. The #6146 / #6343 / #6348 layout-walk guards passed in the renderer run.

### Dim 7: worker threads
- No new `thread::spawn`, `rayon`, `mpsc`, `static Mutex` or `OnceLock` in the window.
- #6114 adds `eviction_deferred` to the `NifImportRegistry`. It is main-thread state, set from `cell_load_in_progress()`, and the worker only sees the `cached_keys` snapshot.
- #6464 widens `needs_exterior_processing`, which gates the same predicate as `SharedParseCache`. The lock scope is unchanged.
- `--bench-capture-every` reuses the existing `ScreenshotBridge` handshake and adds no thread.

No findings.

Next step: nothing to publish from this report. Both rows are Dups of ECS-2026-10-31-01 and -02.
