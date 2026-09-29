# REN-D11-2026-09-29-01: a startup FSR→TAA fallback is persisted to settings.toml, so one failed FSR context permanently turns later no-flag launches into TAA

**Labels**: low,bug,renderer

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: LOW
**Dimension**: FSR/Presentation
**Location**:
- `byroredux/src/app_events.rs` `resumed`: `crate::record_active_upscaler(&self.world, ctx.renderer_config.upscaler, false)`.
- `record_active_upscaler` (`byroredux/src/main.rs`): `if changed || released { settings_io::save(...) }`.
- The FSR→TAA promotion in `VulkanContext::new` (`crates/renderer/src/vulkan/context/init.rs`, the #2480 arm).

## Description
Residual of CLOSED #4974 / #4975. When FSR context creation fails at boot, the renderer promotes to TAA, and `resumed` records that mode with `chosen = false`. `record_active_upscaler` saves whenever the registry value *changed*, regardless of `chosen`. With no CLI pin (the default launch), the fallback is written as `render.upscaler = "taa"`. `active_upscaler_setting_tests` codifies this: `record_active_upscaler(&world, UpscalerMode::Taa, false)` leaves the file holding `taa`. Even without that save, the registry now holds `taa`, so the next unrelated settings save would persist it (the #4974 mechanism).

## Impact
One transient FSR init failure (driver hiccup, device swap, the unexercised FP32 permutation) replaces the engine's default render path for every later launch without `--upscaler`. #4947's `source: settings.toml` log line makes it visible (hence LOW), but it re-opens the harness hazard of a persisted value changing which path "default" exercises.

## Related
#4974, #4975 (closed), #4947, #2480.

## Suggested Fix
Treat a non-`chosen` startup fallback like a CLI override: `pin_stored(UPSCALER_SETTING_ID)` before updating the in-memory registry, so neither this call nor a later unrelated save writes the fallback, and release the pin only on a `chosen` switch. Add the inverse fixture: a fallback with nothing pinned leaves the file untouched.

Validated at HEAD 9fcfdc3fc: `record_active_upscaler` computes `released = chosen && unpin_stored(...)` but saves on `changed || released`, so a `chosen = false` change still saves; `resumed` calls it with `false`.

## Completeness Checks
- [ ] **LOCK_ORDER**: the scoped persistence write lock is still released before the registry read
- [ ] **SIBLING**: other non-chosen setting writes (e.g. FSR quality fallback) checked for the same persistence path
- [ ] **TESTS**: Inverse fixture — an unpinned startup fallback leaves `settings.toml` untouched
