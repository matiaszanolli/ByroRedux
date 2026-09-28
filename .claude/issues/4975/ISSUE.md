# #4975: REN-D11-2026-09-27-03: the settings registry and the live upscaler diverge on three paths; two of them persist a mode that is not running

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4975
- **Labels**: low,renderer,tech-debt,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D11-2026-09-27-03**._

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**:
  - `byroredux/src/main.rs`: the setting-change loop (`settings.set` → `PendingUpscalerSwitch::request`, then `settings_io::save`).
  - `byroredux/src/app_step.rs` `step_upscaler_switch`.
  - `crates/renderer/src/vulkan/context/resize.rs` `set_upscaler_mode` (the rollback arm, `self.renderer_config.upscaler = previous`).
  - `byroredux/src/commands/world_info.rs` `UpscalerSwitchCommand::execute` (queues only, never touches the registry).
  - `crates/renderer/src/vulkan/context/init.rs`: the #2480 startup promotion to `UpscalerMode::Taa`.
- **Status**: NEW
- **Description**:
  - (a) A menu upscaler change is saved to disk in the same UI tick it is staged, before `set_upscaler_mode` runs. If the rebuild fails and rolls back (#2156 arm), the renderer runs `previous` while registry and disk keep the failed mode, which is re-attempted at every launch.
  - (b) `r.upscaler` switches the renderer but leaves the registry, and so the menu, on the old choice. The next unrelated settings save writes that stale value back, reverting the operator's console choice on the next launch.
  - (c) The startup FSR-failure promotion to TAA does not update the registry either, so the menu shows an FSR preset while TAA runs.
- **Evidence**: The code paths listed above. None of `step_upscaler_switch`, `set_upscaler_mode`, the console command or the promotion writes `UPSCALER_SETTING_ID`.
- **Impact**: The menu and the persisted default can misstate the active reconstruction path, compounding REN-D11-2026-09-27-01. There is no GPU impact.
- **Related**: REN-D11-2026-09-27-01/02.
- **Suggested Fix**: Make the applied mode the single writer. After `set_upscaler_mode` returns (success or rollback) and after the init promotion, write the actually-active `UpscalerMode` back into the registry and save only then. Route `r.upscaler` through the same setter.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
