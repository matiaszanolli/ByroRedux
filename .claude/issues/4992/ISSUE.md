# #4992: CONC-D3-2026-09-28-04: After a Vulkan init failure, `about_to_wait` still runs the full scheduler against a world that was never set up, and it panics instead of exiting cleanly

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,ecs,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW. This is a failure-path robustness bug. A user without a working Vulkan driver gets an ECS panic and exit code 101 after the real error.
- **Dimension**: ECS Lock Ordering (system execution outside a booted world)
- **Location**:
  - `byroredux/src/app_events.rs:292-295`, the `resumed` `Err` arm: `log::error!` then `event_loop.exit()`.
  - `byroredux/src/app_events.rs:611` and `:908-910`, where `about_to_wait` calls `self.scheduler.run(&self.world, dt)` with no boot-completed gate.
  - `crates/scripting/src/papyrus_demo/mod.rs:294`, the `world.resource::<PapyrusPlayerEntity>()` panic site.
- **Status**: NEW
- **Description**:
  - `event_loop.exit()` only requests an exit. winit still delivers `about_to_wait` for the current iteration.
  - `about_to_wait` runs every stage.
  - `rumble_on_activate_system` gets `Some` from both `query_mut` calls, because the storages are registered at boot. It then calls `world.resource::<PapyrusPlayerEntity>()`, and that resource is inserted only by scene setup (`byroredux/src/scene.rs:1227`/`:1274`). Scene setup never ran.
  - Any other system that assumes scene resources would fail the same way. The rumble system is simply the first to hit it.
- **Evidence**: The CI backtrace. Frame 5 is `rumble_on_activate_system` (`papyrus_demo/mod.rs:294`), frame 6 is `Scheduler::run` (`scheduler.rs:513`), and frame 7 is `App::about_to_wait` (`app_events.rs:910`).
- **Trigger Conditions**: `VulkanContext::new` returns `Err`. This happens with no ICD, an unsupported device, or the CPU device rejected without `BYRO_ALLOW_CPU_VULKAN_DEVICE`.
- **Impact**:
  - The clean "Vulkan init failed" error is followed by an unrelated panic.
  - In CI this panic is what fails the job (CONC-D3-2026-09-28-03), so it masks the real device-selection result.
- **Verification Path**: `cargo run -p byroredux -- --bench-frames 5` with `VK_ICD_FILENAMES=/nonexistent`. Expect the error line and then the panic.
- **Related**: CONC-D3-2026-09-28-03, #4596.
- **Suggested Fix**:
  - Record a boot-failed or `renderer.is_none()` state and return early from `about_to_wait` before `scheduler.run`.
  - Alternatively, exit the process with a non-zero status directly from the `Err` arm.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D3-2026-09-28-04) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix
