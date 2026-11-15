**HEAD**: 3334bbe48 (audited) · **Scope**: the ECS / scheduler half of `/audit-concurrency`: lock order, undeclared acquisitions in parallel systems, re-entrancy. The Vulkan queue and barrier dimensions were skipped; they need captures. · **Previous**: AUDIT_CONCURRENCY_UI_2026-08-12.md, with the 11-08 and 11-09 dimension reports for dedup.

# ECS Concurrency Audit: 2026-11-15

**No cross-thread deadlock.** The parallel batches are:
- Early: player controller, timer tick
- Update: animation
- PostUpdate: transform propagation
- Physics: physics sync
- Late: camera follow, reverb zone, log stats, metrics sample

None of them touches the contested storages. Every cycle below is taken on the main thread, so its cost is a lock-order-detector panic on a real world, not a deadlock.

**Why the lane missed them.** The fixtures on one side never registered or inserted the other storage, so the detector saw only one edge. All findings are fixed in dfcbcafd5.

| ID | Sev | Cycle | Fix |
|---|---|---|---|
| CONC-1 | MEDIUM | `eat_sleep` took `GlobalTransform` → `WalkSpeed`. Five movers take `WalkSpeed` → `GlobalTransform`. | Diner gather takes `WalkSpeed` first. Fixture `the_diner_gather_takes_walk_speed_before_global_transform`. |
| CONC-2 | MEDIUM | `submersion_system` held `WaterBedField` across a `WaterVolume` write. The controller and physics sync take `WaterVolume` first. | `drop(bed_q)`. Fixture `submersion_with_a_bed_field_takes_its_locks_in_one_order`. |
| CONC-3 | MEDIUM | `kf_idle_work_pending` took `ActorCinematicState` → `IdleClipCatalog`. Idle playback takes the reverse. Both run every frame on FNV/FO3/Oblivion. | Copies the requested forms out before reading the catalog. |
| CONC-4 | LOW | The SDK capture took `Inventory` → `InventoryCatalog`. `encumbrance_system` takes the reverse. | Catalog first. |
| CONC-5 | MEDIUM | The unload walk took `AnimatedTextureFlip` after `MaterialTextureHandles` / `TerrainTileSlot`. The render collect takes the reverse. | Flip handles get their own pass (the #6209 pattern). |
| CONC-6 | LOW | `player_controller_system` read `LimbEffects` undeclared, through `locomotion_mult`. | Declared, and added to the parallel-system hop table. |

**Detector runs**:
- `BYRO_LOCK_ORDER_CHECK=1` over the binary crate after the fixes: all pass. The one failure is the unrelated example-ceiling test, which fails locally only because of an untracked file.
- Before the fixes: core 937 passed, scripting 1411 passed, physics 276 passed.

**Not covered**: console commands (dev-only, single-thread); the `extensions/` sync systems beyond a check for reverse paths of `ExtensionHostSlot` → `host.lock()`, which found none.
